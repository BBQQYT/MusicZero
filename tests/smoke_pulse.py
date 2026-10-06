"""Exercise Termux startup, locale conversion failure, PCM, controls and cleanup."""
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
# Reproduce Bionic's failed locale conversion, including pacat's default
# media-name fallback. Reading a filename avoids that fallback.
if any(arg.startswith(('--client-name','--stream-name','--property')) for arg in sys.argv[1:]):
    sys.exit("Invalid client name 'MusicZero'")
if sys.argv[-1].startswith('--'): sys.exit('Failed to set media name.')
if (root/'fail-client').exists(): sys.exit(7)
if (root/'stall-client').exists(): time.sleep(600)
with open(sys.argv[-1],'rb',buffering=0) as source, (root/'pcm').open('wb',buffering=0) as output:
    while block:=source.read(3840): output.write(block)
''')
    pacat.chmod(0o755)
    # The same startup code runs on Android and in the pulse-output test build
    # when TERMUX_VERSION is present. All commands below are private fake tools.
    for name in ['pactl', 'pulseaudio']:
        tool = fake / name
        tool.write_text('''#!/usr/bin/env python3
import json,os,pathlib,sys
root=pathlib.Path(os.environ['MZ_PULSE_CAPTURE'])
program=pathlib.Path(sys.argv[0]).name
args=sys.argv[1:]
with (root/'server.calls').open('a') as log: log.write(json.dumps([program]+args)+'\\n')
state=root/'server.json'
null=dict(name='auto_null',driver='module-null-sink.c')
if program=='pulseaudio':
    if (root/'fail-start').exists() or ((root/'fail-config').exists() and '-n' not in args):
        sys.exit('Failed to load module-sles-sink: OpenSL ES error 12')
    state.write_text(json.dumps(dict(sinks=[null],default='auto_null')))
elif not state.exists(): sys.exit('Connection refused')
else:
    data=json.loads(state.read_text())
    if args==['--format=json','list','sinks']: print(json.dumps(data['sinks']))
    elif args==['get-default-sink']: print(data['default'])
    elif args[0]=='load-module':
        if (root/('fail-'+args[1])).exists(): sys.exit('Module initialization failed')
        name=args[2].split('=',1)[1]
        data['sinks'].append(dict(name=name,driver=args[1]+'.c'))
        state.write_text(json.dumps(data))
        print(42)
    else: sys.exit('Unexpected command: '+repr(args))
''')
        tool.chmod(0o755)
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
               DBUS_SESSION_BUS_ADDRESS='unix:path='+tmp+'/no-bus', RUST_LOG='warn,mz=info',
               TERMUX_VERSION='mz-test')
    for key in ['PULSE_SERVER', 'PULSE_SINK']:
        env.pop(key, None)

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
            assert '--device=musiczero_aaudio' in (root / 'client.args').read_text()
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

    def calls():
        return [json.loads(line) for line in (root / 'server.calls').read_text().splitlines()]

    assert sum(call[0] == 'pulseaudio' for call in calls()) == 1
    assert sum('load-module' in call for call in calls()) == 1  # Reuse the existing server/sink.

    def startup(sinks=None, default='auto_null', markers=(), overrides=None, device=None, error=None):
        (root / 'server.calls').write_text('')
        (root / 'server.json').unlink(missing_ok=True)
        (root / 'client.pid').unlink(missing_ok=True)
        if sinks is not None:
            (root / 'server.json').write_text(json.dumps(dict(sinks=sinks, default=default)))
        for marker in markers:
            (root / marker).touch()
        with (root / 'host.log').open('w+') as log:
            process = subprocess.Popen([str(BIN / 'mz'), 'demo'], env=dict(env, **(overrides or {})),
                                       stdout=log, stderr=log)
            try:
                if error:
                    assert process.wait(timeout=10) != 0
                    assert error in (root / 'host.log').read_text()
                    assert not (root / 'client.pid').exists()
                else:
                    wait_track('one')
                    args = (root / 'client.args').read_text()
                    assert ('--device='+device in args) if device else '--device=' not in args
                    ipc('quit')
                    assert process.wait(timeout=5) == 0
            finally:
                if process.poll() is None:
                    process.kill(); process.wait()
                for marker in markers:
                    (root / marker).unlink()
        return calls()

    null = dict(name='auto_null', driver='module-null-sink.c')
    sles = dict(name='existing_sles', driver='module-sles-sink.c')
    observed = startup([null, sles], default='existing_sles')
    assert not any(call[0] == 'pulseaudio' or 'load-module' in call for call in observed)
    observed = startup([null, sles], device='existing_sles')
    assert not any(call[0] == 'pulseaudio' or 'load-module' in call for call in observed)
    observed = startup([null], markers=['fail-module-aaudio-sink'], device='musiczero_sles')
    assert [call[2] for call in observed if 'load-module' in call] == ['module-aaudio-sink', 'module-sles-sink']
    startup([null], markers=['fail-module-aaudio-sink', 'fail-module-sles-sink'],
            error='PulseAudio has no Android audio output')
    observed = startup(markers=['fail-config'], device='musiczero_aaudio')
    assert len([call for call in observed if call[0] == 'pulseaudio']) == 2
    assert any('-n' in call for call in observed)
    startup(markers=['fail-start'], error='PulseAudio startup failed')
    for override in [dict(PULSE_SERVER='unix:custom-server'), dict(PULSE_SINK='auto_null')]:
        observed = startup([null], overrides=override)
        assert not any(call[0] == 'pulseaudio' or 'load-module' in call for call in observed)
    observed = startup(overrides=dict(PULSE_SERVER='unix:unreachable'), error='Connection refused')
    assert not any(call[0] == 'pulseaudio' for call in observed)
    print('Termux startup: server reuse, AAudio/OpenSL ES fallback, broken config, hardware routing and explicit overrides: OK')

    if os.environ.get('MZ_PULSE_REAL') == '1':
        # A private PulseAudio null sink tests the real client without audible output.
        server_env = dict(env, PATH=os.environ['PATH'], PULSE_SERVER='unix:'+str(root / 'pulse.sock'))
        server_env.pop('TERMUX_VERSION', None)
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
                # Force the real libpulse conversion function to fail exactly as
                # Bionic does. The old invocation must fail before connecting;
                # the fixed host must still run real pacat playback and controls.
                shim = root / 'locale.c'
                shim.write_text('char *pa_locale_to_utf8(const char *s) { (void)s; return 0; }\n')
                library = root / 'locale.so'
                subprocess.run(['cc', '-shared', '-fPIC', str(shim), '-o', str(library)], check=True)
                server_env['LD_PRELOAD'] = str(library)
                broken = subprocess.run(['pacat', '--raw', '--client-name=MusicZero'], input=b'',
                                        env=server_env, capture_output=True, timeout=10)
                assert broken.returncode != 0 and b'Invalid client name' in broken.stderr
                fallback = subprocess.run(['pacat', '--raw'], input=b'', env=server_env,
                                          capture_output=True, timeout=10)
                assert fallback.returncode != 0 and b'Failed to set media name' in fallback.stderr
                subprocess.run([sys.executable, str(ROOT / 'tests/smoke_seek.py')], env=server_env, check=True)
                subprocess.run([sys.executable, str(ROOT / 'tests/smoke_history.py')], env=server_env, check=True)
                print('Pulse audio: real pacat with failed locale conversion, null sink, seek and history: OK')
            finally:
                server.terminate()
                server.wait(timeout=10)
