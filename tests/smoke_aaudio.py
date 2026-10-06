"""Run the real AAudio host against a native ABI mock, never an audio server."""
import json
import os
import pathlib
import socket
import struct
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BIN = ROOT / 'target' / os.environ.get('MZ_TEST_PROFILE', 'aaudio/debug')

with tempfile.TemporaryDirectory(prefix='mz-aaudio-') as tmp:
    root = pathlib.Path(tmp)
    subprocess.run(['cc', '-std=c11', '-D_POSIX_C_SOURCE=200809L', '-Wall', '-Wextra', '-Werror',
                    '-shared', '-fPIC', str(ROOT / 'tests/fixtures/aaudio_mock.c'),
                    '-o', str(root / 'libaaudio.so')], check=True)
    folder = root / 'modules/demo'
    folder.mkdir(parents=True)
    (folder / 'module.json').write_text(json.dumps(dict(protocol=1, id='demo', name='Demo',
                                                        binary='provider', default_playlist='all')))
    (folder / 'provider').write_text('''#!/usr/bin/env python3
import json,pathlib,sys
if sys.argv[1]=='info': print(json.dumps(dict(protocol=1,id='demo',name='Demo',default_playlist='all')))
elif sys.argv[1]=='tracks': print(json.dumps(dict(tracks=[dict(id=i,title=i,artist='Test') for i in ['one','two']])))
elif sys.argv[1]=='audio': sys.stdout.buffer.write((pathlib.Path(__file__).parent/'tone.flac').read_bytes())
''')
    (folder / 'provider').chmod(0o755)
    subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi', '-i',
                    'sine=frequency=440:duration=8', str(folder / 'tone.flac')], check=True)
    env = dict(os.environ, LD_LIBRARY_PATH=tmp, MZ_AAUDIO_CAPTURE=tmp,
               MZ_MODULES_DIR=str(root / 'modules'), XDG_CONFIG_HOME=str(root / 'config'),
               XDG_CACHE_HOME=str(root / 'cache'), XDG_RUNTIME_DIR=tmp,
               DBUS_SESSION_BUS_ADDRESS='unix:path='+tmp+'/no-bus', RUST_LOG='warn,mz=info')
    # Broken PulseAudio settings must have no effect on native Android playback.
    env.update(PULSE_SERVER='unix:/nonexistent', PULSE_SINK='broken')

    def ipc(action, value='', key=''):
        with socket.socket(socket.AF_UNIX) as channel:
            channel.settimeout(3)
            channel.connect(str(root / 'musiczero.sock'))
            channel.sendall(json.dumps(dict(action=action, value=value, key=key)).encode()+b'\n')
            with channel.makefile('rb') as reader:
                return json.loads(reader.readline())

    def wait_track(id):
        deadline = time.monotonic()+10
        while time.monotonic() < deadline:
            try:
                status = ipc('status')
                if status.get('can_seek') and status['track_id'] == id:
                    return status
            except OSError:
                pass
            time.sleep(.02)
        raise RuntimeError((root / 'host.log').read_text())

    def samples():
        data = (root / 'pcm').read_bytes()[-3840:]
        return struct.unpack('<'+'f'*(len(data)//4), data)

    # Four short native writes per mixer block must preserve every frame.
    (root / 'partial-write').touch()
    with (root / 'host.log').open('w+') as log:
        process = subprocess.Popen([str(BIN / 'mz'), 'demo'], env=env, stdout=log, stderr=log)
        try:
            wait_track('one')
            time.sleep(.15)
            assert max(map(abs, samples())) > .01
            ipc('pause')
            time.sleep(.15)
            assert max(map(abs, samples())) == 0
            before = ipc('status')['position_ms']
            time.sleep(.15)
            assert abs(ipc('status')['position_ms'] - before) < 30
            assert 'error' not in ipc('seek', '3000000', 'absolute')
            assert abs(ipc('status')['position_ms'] - 3000) < 30
            started = time.monotonic()
            ipc('play')
            time.sleep(.35)
            advanced = ipc('status')['position_ms'] - 3000
            assert 150 < advanced < (time.monotonic()-started)*1000+150, advanced
            ipc('next'); wait_track('two')
            ipc('previous'); wait_track('one')
            ipc('stop')
            time.sleep(.15)
            assert max(map(abs, samples())) == 0
            ipc('quit')
            assert process.wait(timeout=5) == 0
        finally:
            if process.poll() is None:
                process.kill(); process.wait()
            (root / 'partial-write').unlink()
    assert (root / 'events').read_text().splitlines() == ['create', 'open', 'delete-builder',
                                                       'buffer', 'start', 'stop', 'close']
    print('AAudio: nonzero PCM, partial-write continuity, silence, bounded pace, seek/history and exact cleanup: OK')

    cases = [('fail-create', 'create builder', 0, 0), ('fail-open', 'open stream', 1, 0),
             ('bad-config', 'did not accept', 1, 1), ('fail-start', 'start stream', 1, 1),
             ('fail-write', 'ErrorDisconnected', 1, 1), ('stall-write', 'stalled', 1, 1),
             ('stall-write', None, 1, 1)]
    for marker, error, deleted, closed in cases:
        (root / 'events').write_text('')
        (root / marker).touch()
        with (root / 'host.log').open('w+') as log:
            process = subprocess.Popen([str(BIN / 'mz'), 'demo'], env=env, stdout=log, stderr=log)
            try:
                if error:
                    assert process.wait(timeout=10) != 0
                    assert error in (root / 'host.log').read_text()
                else:
                    wait_track('one')
                    time.sleep(.4)
                    ipc('quit')
                    assert process.wait(timeout=3) == 0
            finally:
                if process.poll() is None:
                    process.kill(); process.wait()
                (root / marker).unlink()
        events = (root / 'events').read_text().splitlines()
        assert events.count('delete-builder') == deleted, events
        assert events.count('close') == closed, events
    print('AAudio: startup/format/disconnect/stall errors and bounded shutdown release native resources: OK')

    # Existing comprehensive host tests use this exact native-output path too.
    for test in ['smoke_notification.py', 'smoke_seek.py', 'smoke_history.py', 'smoke_modules.py']:
        subprocess.run([sys.executable, str(ROOT / 'tests' / test)], env=env, check=True)
    print('AAudio: MPRIS/CLI seek, persistent history, wave feedback, Local formats and live Icecast: OK')
