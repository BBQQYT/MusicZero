"""Linux tray/service CLI and PTY checks; optional isolated real user-systemd playback."""
import json
import os
import pathlib
import shutil
import socket
import subprocess
import tempfile
import time
import uuid
from smoke_config import Terminal

ROOT = pathlib.Path(__file__).resolve().parents[1]
BIN = ROOT / 'target' / os.environ.get('MZ_TEST_PROFILE', 'debug')

SYSTEMCTL = r'''#!/usr/bin/env python3
import json,pathlib,sys
root=pathlib.Path(__file__).parent.parent
args=sys.argv[1:]
assert args.pop(0)=='--user'
with (root/'systemctl.jsonl').open('a') as output: output.write(json.dumps(args)+'\n')
statefile=root/'service-state.json'
state=json.loads(statefile.read_text()) if statefile.exists() else dict(active=False,enabled=False)
if (root/'fail-manager').exists():
    print('No user manager',file=sys.stderr);sys.exit(1)
if args[0]=='show':
    print('ActiveState='+('active' if state['active'] else 'inactive'))
    print('UnitFileState='+('enabled' if state['enabled'] else 'disabled'))
elif args[0]=='enable':
    if (root/'fail-enable').exists():
        print('Enable refused',file=sys.stderr);sys.exit(1)
    state['enabled']=True
elif args[0] in ('start','restart'): state['active']=True
elif args[0]=='stop': state['active']=False
elif args[0]=='disable':
    state['enabled']=False
    if '--now' in args: state['active']=False
statefile.write_text(json.dumps(state))
'''

PROVIDER = r'''#!/usr/bin/env python3
import json,pathlib,sys
root=pathlib.Path(__file__).parent
if sys.argv[1]=='info': print(json.dumps(dict(protocol=1,id=root.name,name='Demo '+root.name,default_playlist='all')))
elif sys.argv[1]=='tracks': print(json.dumps(dict(tracks=[dict(id='one',title='Tone',artist='Test')])) )
elif sys.argv[1]=='audio': sys.stdout.buffer.write((root.parent.parent/'tone.flac').read_bytes())
elif sys.argv[1]=='settings': print('{"settings":{}}')
'''


def main():
    real_systemctl = shutil.which('systemctl')
    with tempfile.TemporaryDirectory(prefix='mz-service-') as tmp:
        root = pathlib.Path(tmp)
        bin_dir = root / 'Музыка $literal %h "quote"' / 'bin'
        bin_dir.mkdir(parents=True)
        host = bin_dir / 'mz'
        shutil.copy2(BIN / 'mz', host)
        fake = root / 'fakebin'
        fake.mkdir()
        (fake / 'systemctl').write_text(SYSTEMCTL)
        (fake / 'systemctl').chmod(0o755)
        for id in ['demo', 'other']:
            folder = root / 'modules' / id
            folder.mkdir(parents=True)
            (folder / 'module.json').write_text(json.dumps(dict(protocol=1, id=id, name='Demo '+id,
                                                               binary='provider', default_playlist='all')))
            (folder / 'provider').write_text(PROVIDER)
            (folder / 'provider').chmod(0o755)
        subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi', '-i',
                        'sine=frequency=440:duration=180', str(root / 'tone.flac')], check=True)
        alsa = root / 'alsa.conf'
        alsa.write_text('pcm.!default { type null }\n')
        env = dict(os.environ, TERM='xterm-256color', PATH=str(fake)+os.pathsep+os.environ['PATH'],
                   XDG_CONFIG_HOME=str(root / 'config'), XDG_CACHE_HOME=str(root / 'cache'), XDG_RUNTIME_DIR=tmp,
                   MZ_MODULES_DIR=str(root / 'modules'), ALSA_CONFIG_PATH=str(alsa),
                   DBUS_SESSION_BUS_ADDRESS='unix:path='+tmp+'/no-bus', YM_TOKEN='do-not-save-this-test-token',
                   RUST_LOG='warn,mz=info')
        service = root / 'config/systemd/user/musiczero.service'
        preferences = root / 'config/mz/settings.json'

        def cli(*args, success=True):
            result = subprocess.run([str(host), *args], env=env, capture_output=True, text=True, timeout=40)
            assert (result.returncode == 0) == success, (args, result.stdout, result.stderr)
            return result.stdout

        def ipc(action):
            with socket.socket(socket.AF_UNIX) as channel:
                channel.settimeout(5)
                channel.connect(str(root / 'musiczero.sock'))
                channel.sendall(json.dumps(dict(action=action)).encode()+b'\n')
                with channel.makefile('rb') as reader:
                    return json.loads(reader.readline())

        def wait_audio():
            deadline = time.monotonic()+15
            while time.monotonic() < deadline:
                try:
                    if ipc('status').get('can_seek'):
                        return
                except (OSError, ValueError):
                    pass
                time.sleep(.03)
            raise RuntimeError('Service did not start audio: '+(root / 'host.log').read_text(errors='replace'))

        assert cli('tray').strip() == 'on'
        cli('tray', 'off')
        assert cli('tray').strip() == 'off'
        cli('tray', 'wrong', success=False)
        assert not json.loads(cli('service', 'status'))['installed']
        cli('service', 'install', 'demo')
        assert service.stat().st_mode & 0o777 == 0o600
        text = service.read_text()
        assert 'ExecStart=:' in text and 'serve "demo"' in text
        assert '%%h' in text and '$literal' in text and '\\"quote\\"' in text
        assert 'do-not-save-this-test-token' not in text and 'YM_TOKEN=' not in text
        assert json.loads(preferences.read_text())['tray_enabled'] is False
        assert json.loads(cli('service', 'status'))['enabled']
        cli('service', 'install', 'absent', success=False)
        assert service.read_text() == text
        # Failed changes restore the existing unit and selected provider.
        (root / 'fail-enable').touch()
        cli('service', 'install', 'other', success=False)
        assert service.read_text() == text
        assert json.loads(preferences.read_text())['service_module'] == 'demo'
        (root / 'fail-enable').unlink()
        with (root / 'host.log').open('w+') as log:
            process = subprocess.Popen([str(host), 'demo'], env=env, stdout=log, stderr=log)
            try:
                wait_audio()
                cli('service', 'start', success=False)
                cli('serve', 'other', success=False)
                assert ipc('status')['module'] == 'demo'
                cli('quit')
                assert process.wait(timeout=10) == 0
                log.seek(0)
                assert 'mz::tray' not in log.read(), 'Disabled tray spawned a task'
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
        for action, active in [('start', True), ('restart', True), ('stop', False)]:
            cli('service', action)
            assert json.loads(cli('service', 'status'))['active'] is active

        cli('tray', 'on')
        with (root / 'tray.log').open('w+') as log:
            process = subprocess.Popen([str(host), 'demo'], env=env, stdout=log, stderr=log)
            try:
                wait_audio()
                deadline = time.monotonic()+5
                while time.monotonic() < deadline:
                    log.seek(0)
                    if 'mz::tray' in log.read():
                        break
                    time.sleep(.03)
                else:
                    raise AssertionError('Enabled tray never attempted to connect')
                cli('quit')
                assert process.wait(timeout=10) == 0
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
        cli('tray', 'off')

        # Start the generated unit under a unique runtime name, leaving the user's
        # actual musiczero.service, autostart links and playing host untouched.
        manager = subprocess.run([real_systemctl, '--user', 'is-system-running'], capture_output=True,
                                 timeout=10) if real_systemctl else None
        if manager and manager.returncode == 0:
            runtime = pathlib.Path(os.environ.get('XDG_RUNTIME_DIR') or f'/run/user/{os.getuid()}') / 'systemd/user'
            runtime.mkdir(parents=True, exist_ok=True)
            name = 'musiczero-smoke-'+uuid.uuid4().hex+'.service'
            test_unit = runtime / name
            def systemctl(*args):
                result = subprocess.run([real_systemctl, '--user', *args], capture_output=True, text=True,
                                        timeout=25)
                if result.returncode:
                    raise RuntimeError(result.stderr)
                return result
            extra = ''.join('Environment='+json.dumps(key+'='+env[key], ensure_ascii=False).replace('%','%%')+'\n'
                            for key in ['XDG_RUNTIME_DIR', 'ALSA_CONFIG_PATH', 'DBUS_SESSION_BUS_ADDRESS'])
            test_unit.write_text(text.replace('\n[Install]', '\n'+extra+'\n[Install]'))
            try:
                systemctl('daemon-reload')
                systemctl('start', name)
                wait_audio()
                assert ipc('status')['module'] == 'demo'
                systemctl('stop', name)
                assert systemctl('show', name, '--property=ActiveState', '--value').stdout.strip() == 'inactive'
                try:
                    ipc('status')
                except OSError:
                    pass
                else:
                    raise AssertionError('Stopped service still accepts control requests')
                print('Service: generated unit played synthetic audio under real systemd --user; clean stop OK')
            finally:
                subprocess.run([real_systemctl, '--user', 'stop', name], capture_output=True, timeout=25)
                test_unit.unlink(missing_ok=True)
                subprocess.run([real_systemctl, '--user', 'daemon-reload'], capture_output=True, timeout=25)
        else:
            print('Service: no real user manager; command/UI regression checks remain available')

        cli('service', 'remove')
        assert not service.exists() and preferences.exists()
        # Both languages expose installation and optional immediate startup.
        for language, install_label, source_label, later, remove_label, cancel, confirm in [
            ('ru','Установить сервис','Источник для сервиса','Позже','Удалить сервис','Отмена','Удалить'),
            ('en','Install service','Service source','Later','Remove service','Cancel','Remove')]:
            prefs = json.loads(preferences.read_text())
            prefs['language'] = language
            preferences.write_text(json.dumps(prefs))
            tui = Terminal(env, '--service')
            try:
                tui.expect(install_label)
                tui.pick(install_label)
                tui.expect(source_label)
                tui.pick('Demo demo')
                tui.expect(later)
                tui.pick(later)
                tui.expect(install_label)
                assert service.exists() and json.loads(cli('service','status'))['enabled']
                tui.pick(remove_label)
                tui.pick(cancel)
                tui.expect(install_label)
                assert service.exists()
                tui.pick(remove_label)
                tui.pick(confirm)
                tui.expect(install_label)
                assert not service.exists()
            finally:
                tui.close()
        (root / 'fail-manager').touch()
        cli('service', 'install', 'demo', success=False)
        assert not service.exists()
        print('Service: tray persistence/runtime, install/autostart/start/stop/remove, rollback, manual-host protection, '
              'literal paths, unavailable manager and RU/EN TUI: OK')


if __name__ == '__main__':
    main()
