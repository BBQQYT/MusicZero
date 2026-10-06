"""Exercise the Termux audio path: PCM, pacing, pause/seek/history, client failure and cleanup."""
import json
import os
import pathlib
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BIN = ROOT / 'target' / os.environ.get('MZ_TEST_PROFILE', 'pulse/debug')

with tempfile.TemporaryDirectory(prefix='mz-pulse-') as tmp:
    root = pathlib.Path(tmp)
    fake = root / 'fakebin'
    fake.mkdir()
    pacat = fake / 'pacat'
    pacat.write_text('''#!/usr/bin/env python3
import os,pathlib,sys,time
root=pathlib.Path(os.environ['MZ_PULSE_CAPTURE'])
(root/'client.pid').write_text(str(os.getpid()))
(root/'client.args').write_text(' '.join(sys.argv[1:]))
if (root/'fail-client').exists(): sys.exit(7)
if (root/'stall-client').exists(): time.sleep(600)
with (root/'pcm').open('wb',buffering=0) as output:
    while block:=sys.stdin.buffer.read(3840): output.write(block)
''')
    pacat.chmod(0o755)
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
    env = dict(os.environ, PATH=str(fake)+os.pathsep+os.environ['PATH'], MZ_PULSE_CAPTURE=tmp,
               MZ_MODULES_DIR=str(root / 'modules'), XDG_CONFIG_HOME=str(root / 'config'),
               XDG_CACHE_HOME=str(root / 'cache'), XDG_RUNTIME_DIR=tmp,
               DBUS_SESSION_BUS_ADDRESS='unix:path='+tmp+'/no-bus', RUST_LOG='warn,mz=info')

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

    with (root / 'host.log').open('w+') as log:
        process = subprocess.Popen([str(BIN / 'mz'), 'demo'], env=env, stdout=log, stderr=log)
        try:
            wait_track('one')
            time.sleep(.15)
            assert max(map(abs, samples())) > .01
            assert '--format=float32le' in (root / 'client.args').read_text()
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
            client_pid = int((root / 'client.pid').read_text())
            ipc('quit')
            assert process.wait(timeout=5) == 0
            assert not pathlib.Path('/proc', str(client_pid)).exists()
        finally:
            if process.poll() is None:
                process.kill(); process.wait()
    print('Pulse audio: nonzero PCM, silence on pause/stop, bounded pace, seek, history and client cleanup: OK')

    for marker in ['fail-client', 'stall-client']:
        (root / marker).touch()
        with (root / 'host.log').open('w+') as log:
            process = subprocess.Popen([str(BIN / 'mz'), 'demo'], env=env, stdout=log, stderr=log)
            try:
                if marker == 'fail-client':
                    assert process.wait(timeout=10) != 0
                    assert 'PulseAudio' in (root / 'host.log').read_text()
                else:
                    wait_track('one')
                    time.sleep(.4)  # Fill a client pipe whose reader is stalled.
                    ipc('quit')
                    assert process.wait(timeout=5) == 0
            finally:
                if process.poll() is None:
                    process.kill(); process.wait()
                (root / marker).unlink()
    print('Pulse audio: client failure is surfaced; stalled writer stops without hanging: OK')

    if os.environ.get('MZ_PULSE_REAL') == '1':
        # A private PulseAudio null sink tests the real client without audible output.
        server_env = dict(env, PATH=os.environ['PATH'], PULSE_SERVER='unix:'+str(root / 'pulse.sock'))
        with (root / 'pulse.log').open('w+') as log:
            server = subprocess.Popen(['pulseaudio', '-n', '--daemonize=no', '--use-pid-file=no',
                                       '--exit-idle-time=-1', '--disable-shm=yes',
                                       '--load=module-null-sink sink_name=mz_alpha',
                                       '--load=module-native-protocol-unix socket='+str(root / 'pulse.sock')+' auth-anonymous=1'],
                                      env=server_env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic()+10
                while not (root / 'pulse.sock').exists():
                    if server.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError((root / 'pulse.log').read_text())
                    time.sleep(.05)
                subprocess.run([sys.executable, str(ROOT / 'tests/smoke_seek.py')], env=server_env, check=True)
                subprocess.run([sys.executable, str(ROOT / 'tests/smoke_history.py')], env=server_env, check=True)
                print('Pulse audio: real pacat/PulseAudio null sink, seek and history: OK')
            finally:
                server.terminate()
                server.wait(timeout=10)
