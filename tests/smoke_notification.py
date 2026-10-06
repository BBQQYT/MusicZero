"""Real host/IPC callbacks with fake Termux APIs; no device or account required."""
import json
import os
import pathlib
import socket
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BIN = ROOT / 'target' / os.environ.get('MZ_TEST_PROFILE', 'aaudio/debug')

with tempfile.TemporaryDirectory(prefix="mz-notify ' $` ") as tmp:
    root = pathlib.Path(tmp)
    subprocess.run(['cc', '-std=c11', '-D_POSIX_C_SOURCE=200809L', '-shared', '-fPIC',
                    str(ROOT / 'tests/fixtures/aaudio_mock.c'), '-o', str(root / 'libaaudio.so')], check=True)
    prefix = root / 'prefix'
    (prefix / 'bin').mkdir(parents=True)
    fake = '''#!/usr/bin/env python3
import json,os,pathlib,subprocess,sys,time
root=pathlib.Path(os.environ['MZ_AAUDIO_CAPTURE'])
kind=pathlib.Path(sys.argv[0]).name
with (root/'notifications').open('a') as out: out.write(json.dumps([kind,sys.argv[1:]])+'\\n')
if kind=='termux-notification' and (root/'fail-api').exists(): sys.exit(1)
if kind=='termux-notification' and (root/'hang-api').exists():
    child=subprocess.Popen(['sleep','30'])
    (root/'child-pid').write_text(str(child.pid))
    child.wait()
'''
    for name in ['termux-notification', 'termux-notification-remove']:
        path = prefix / 'bin' / name
        path.write_text(fake)
        path.chmod(0o755)
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
                    'sine=frequency=440:duration=30', str(folder / 'tone.flac')], check=True)
    env = dict(os.environ, PREFIX=str(prefix), LD_LIBRARY_PATH=tmp, MZ_AAUDIO_CAPTURE=tmp,
               MZ_MODULES_DIR=str(root / 'modules'), XDG_CONFIG_HOME=str(root / 'config'),
               XDG_CACHE_HOME=str(root / 'cache'), XDG_RUNTIME_DIR=tmp,
               DBUS_SESSION_BUS_ADDRESS='unix:path='+tmp+'/no-bus')

    def ipc(action):
        with socket.socket(socket.AF_UNIX) as channel:
            channel.settimeout(1)
            channel.connect(str(root / 'musiczero.sock'))
            channel.sendall(json.dumps(dict(action=action)).encode()+b'\n')
            with channel.makefile('rb') as reader:
                return json.loads(reader.readline())

    def wait(predicate):
        deadline = time.monotonic()+8
        while time.monotonic() < deadline:
            try:
                value = predicate()
                if value: return value
            except (OSError, ValueError, KeyError):
                pass
            time.sleep(.03)
        raise AssertionError((root / 'host.log').read_text())

    def records():
        path = root / 'notifications'
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def notification(title, button):
        def latest():
            shown = [args for kind, args in records() if kind=='termux-notification']
            if not shown: return False
            args = shown[-1]
            values = dict(zip(args[6::2], args[7::2]))
            return values if values.get('--title')==title and button in values.get('--button2', '') else False
        return wait(latest)

    def press(values, button):
        # Android uses a fresh shell: deliberately exclude the player's XDG env.
        subprocess.run(['/bin/sh', '-c', values[f'--button{button}-action']],
                       env={'PATH': '/usr/bin:/bin', 'HOME': '/nonexistent'}, check=True, timeout=3)

    def preferences(**settings):
        path = root / 'config/mz/settings.json'
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(settings))

    def run(check):
        (root / 'notifications').write_text('')
        with (root / 'host.log').open('w') as log:
            process = subprocess.Popen([str(BIN / 'mz'), 'demo'], env=env, stdout=log, stderr=log)
            try:
                wait(lambda: ipc('status')['track_id']=='one')
                check()
                started = time.monotonic()
                ipc('quit')
                assert process.wait(timeout=5)==0
                assert time.monotonic()-started < 4
            finally:
                if process.poll() is None:
                    process.kill(); process.wait()
        return records()

    def buttons():
        state = notification('one', 'Пауза')
        assert state['--content']=='Test · Demo · Воспроизведение'
        count = len(records()); time.sleep(.2); assert len(records())==count
        press(state, 2)
        state = notification('one', 'Продолжить')
        assert ipc('status')['status']=='Paused'
        press(state, 2); notification('one', 'Пауза')
        press(state, 3); state = notification('two', 'Пауза')
        assert ipc('status')['track_id']=='two'
        press(state, 1); notification('one', 'Пауза')
        assert ipc('status')['track_id']=='one'

    events = run(buttons)
    assert events[-1]==['termux-notification-remove', ['musiczero-player']]
    preferences(language='en')
    run(lambda: notification('one', 'Pause'))
    preferences(notifications_enabled=False)
    assert run(lambda: None)==[]
    preferences()
    (root / 'fail-api').touch()
    def failing():
        wait(lambda: len(records())>0)
        ipc('next'); wait(lambda: ipc('status')['track_id']=='two')
        assert ipc('status')['position_ms'] < 1000
    run(failing)
    (root / 'fail-api').unlink()
    (root / 'hang-api').touch()
    def hanging():
        wait(lambda: (root/'child-pid').exists())
        started = time.monotonic()
        ipc('pause'); assert time.monotonic()-started < .5
        assert ipc('status')['status']=='Paused'
    run(hanging)
    pid = int((root / 'child-pid').read_text())
    def dead():
        proc = pathlib.Path(f'/proc/{pid}/stat')
        return not proc.exists() or proc.read_text().split(') ')[1].startswith('Z ')
    wait(dead)
    (root / 'hang-api').unlink()
    # A native output error must remove our notification too, not just CLI quit.
    (root / 'notifications').write_text('')
    with (root / 'host.log').open('w') as log:
        process = subprocess.Popen([str(BIN / 'mz'), 'demo'], env=env, stdout=log, stderr=log)
        try:
            wait(lambda: ipc('status')['track_id']=='one')
            notification('one', 'Пауза')
            (root/'fail-write').touch()
            assert process.wait(timeout=5) != 0
            assert records()[-1]==['termux-notification-remove', ['musiczero-player']]
        finally:
            if process.poll() is None:
                process.kill(); process.wait()
            (root/'fail-write').unlink()
    (prefix / 'bin/termux-notification').unlink()
    assert run(lambda: None)==[]
    print('Notification: real callbacks, RU/EN, deduplication, opt-out, absent/failing/hung APIs and child cleanup: OK')
