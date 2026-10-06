"""Linux PTY integration test of the native RU/EN settings menu; no accounts/network."""
import fcntl
import json
import os
import pathlib
import pty
import re
import select
import shutil
import signal
import socket
import struct
import subprocess
import tempfile
import termios
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BIN = ROOT / 'target' / os.environ.get('MZ_TEST_PROFILE', 'debug')
ANSI = re.compile(r'\x1b\[[0-?]*[ -/]*[@-~]')
PROVIDER = r'''#!/usr/bin/env python3
import json,os,pathlib,sys
root=pathlib.Path(__file__).parent
path=root/'settings.json'
settings=json.loads(path.read_text()) if path.exists() else dict(quality='normal',nested=dict(enabled=True))
command=sys.argv[1]
if command=='info':
    print(json.dumps(dict(protocol=1,id='custom',name='Custom provider',default_playlist='main')))
elif command=='settings':
    print(json.dumps(dict(settings=settings)))
elif command=='set-setting':
    key,value=sys.argv[2:4]
    settings[key]=json.loads(value) if key=='nested' else value
    path.write_text(json.dumps(settings))
    print(json.dumps(dict(settings=settings)))
elif command=='playlists':
    print(json.dumps(dict(playlists=[dict(id='main',name='Custom playlist')])))
elif command=='login':
    print('Test login: enter done',flush=True)
    assert input()=='done'
    assert os.environ['MZ_LANGUAGE']=='en'
    settings['authorized']=True
    path.write_text(json.dumps(settings))
'''


class Terminal:
    def __init__(self, env, *args):
        self.master, self.slave = pty.openpty()
        self.original = termios.tcgetattr(self.slave)
        self.resize(110, 40)
        def setup():
            os.setsid()
            fcntl.ioctl(self.slave, termios.TIOCSCTTY, 0)
        self.process = subprocess.Popen([str(BIN / 'mz'), 'config', *args], env=env,
                                        stdin=self.slave, stdout=self.slave, stderr=self.slave,
                                        preexec_fn=setup)
        self.raw = b''
        self.all_output = b''

    def resize(self, cols, rows):
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', rows, cols, 0, 0))

    def screen(self):
        last = self.raw.decode(errors='replace').split('\x1b[2J')[-1]
        last = re.sub(r'\x1b\[\d+;\d+H', '\n', last)
        return ANSI.sub('', last)

    def wait(self, predicate, timeout=10):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if select.select([self.master], [], [], .05)[0]:
                try:
                    data = os.read(self.master, 65536)
                except OSError:
                    data = b''
                self.raw += data
                self.all_output += data
                continue
            screen = self.screen()
            if predicate(screen):
                return screen
            if self.process.poll() is not None:
                raise AssertionError('TUI exited: ' + screen)
        raise AssertionError('TUI timed out: ' + self.screen())

    def expect(self, text):
        return self.wait(lambda screen: text in screen)

    def key(self, value):
        self.raw = b''
        os.write(self.master, value.encode())

    def pick(self, label):
        self.key('\x1b[H')
        for _ in range(100):
            self.wait(lambda screen: any(line.startswith('> ') for line in screen.splitlines()))
            selected = next(line for line in self.screen().splitlines() if line.startswith('> '))
            if label in selected:
                self.key('\r')
                return
            self.key('\x1b[B')
        raise AssertionError('Menu item not found: ' + label)

    def edit(self, label, value):
        screen = self.wait(lambda screen: '↑↓' in screen and 'Ctrl+U' not in screen)
        title = screen.splitlines()[0]
        self.pick(label)
        self.expect('Ctrl+U')
        self.key('\x15' + value + '\r')
        self.wait(lambda screen: screen.startswith(title) or re.search(r'/\s*(Error|Ошибка)', screen))

    def close(self):
        if self.process.poll() is None:
            self.key('\x03')
            self.process.wait(timeout=10)
        assert self.process.returncode == 0, self.screen()
        assert termios.tcgetattr(self.slave) == self.original, 'Terminal mode was not restored'
        os.close(self.master)
        os.close(self.slave)


def main():
    with tempfile.TemporaryDirectory(prefix='mz-config-') as temporary:
        root = pathlib.Path(temporary)
        env = dict(os.environ, TERM='xterm-256color', XDG_CONFIG_HOME=str(root / 'config'),
                   XDG_CACHE_HOME=str(root / 'cache'), XDG_RUNTIME_DIR=temporary,
                   MZ_MODULES_DIR=str(root / 'modules'))
        for key in ['YM_TOKEN', 'YOUMZ_SESSION', 'YOUMZ_PROXY', 'YOUMZ_BROWSER']:
            env.pop(key, None)
        for module in ['local', 'icecast', 'ymz', 'youmz']:
            folder = root / 'modules' / module
            folder.mkdir(parents=True)
            shutil.copy2(ROOT / 'modules' / module / 'module.json', folder)
            shutil.copy2(BIN / (module + '-module'), folder)
        custom = root / 'modules/custom'
        custom.mkdir()
        (custom / 'module.json').write_text(json.dumps(dict(protocol=1, id='custom', name='Custom provider',
                                                          binary='provider', default_playlist='main')))
        (custom / 'provider').write_text(PROVIDER)
        (custom / 'provider').chmod(0o755)
        def cli(*args):
            result = subprocess.run([str(BIN / 'mz'), *args], env=env, capture_output=True, text=True, timeout=15)
            assert result.returncode == 0, result.stderr
            return result.stdout
        def settings(module):
            return json.loads(cli('settings', module))
        music = root / 'Музыка с пробелами'
        music.mkdir()
        audio = root / 'audio temporary'
        audio.mkdir()
        tui = Terminal(env, 'local')
        try:
            tui.expect('Локальная музыка')
            # Every Local option is reachable, including ones below the viewport.
            tui.edit('Папка музыки', str(music))
            tui.expect(str(music))
            assert settings('local')['path'] == str(music)
            tui.pick('Включать подпапки')
            tui.pick('Нет')
            tui.expect('Включать подпапки: Нет')
            assert settings('local')['recursive'] is False
            tui.edit('Лимит сканирования', '0')
            tui.expect('Ошибка')
            assert settings('local')['scan_limit'] == 10000
            tui.key('\x1b')
            tui.expect('Локальная музыка')
            tui.pick('Лимит сканирования')
            tui.expect('Ctrl+U')
            tui.key('\x15333\x1b')
            tui.expect('Локальная музыка')
            assert settings('local')['scan_limit'] == 10000
            tui.key('\x1b')
            tui.expect('Язык / Language')
            tui.pick('Язык / Language')
            tui.pick('English')
            tui.expect('Settings')
            assert json.loads((root / 'config/mz/settings.json').read_text())['language'] == 'en'
            tui.resize(38, 10)
            tui.expect('Settings')
            tui.resize(110, 40)
            tui.pick('Player and folders')
            tui.expect('Temporary audio folder')
            tui.edit('Temporary audio folder', str(audio))
            tui.edit('Modules folder', str(root / 'modules'))
            tui.edit('Log filter', 'warn,mz=info')
            tui.pick('Show tray icon')
            tui.pick('No')
            tui.expect('Show tray icon: No')
            assert cli('tray').strip() == 'off'
            prefs = json.loads((root / 'config/mz/settings.json').read_text())
            assert prefs['temp_dir'] == str(audio) and prefs['modules_dir'] == str(root / 'modules')
            assert (root / 'config/mz/settings.json').stat().st_mode & 0o777 == 0o600
            tui.key('\x1b')
            tui.expect('Settings')
            tui.pick('Icecast Radio')
            tui.expect('Radio stations')
            # Secret update must never echo the value or overwrite it with its mask.
            tui.pick('Radio stations')
            tui.pick('Icecast (default)')
            tui.edit('Password', 'test-radio-secret')
            tui.expect('Password:')
            tui.edit('Name', 'Название станции')
            tui.expect('Название станции')
            stored = json.loads((root / 'config/mz-icecast/settings.json').read_text())
            assert stored['stations'][0]['password'] == 'test-radio-secret'
            assert b'test-radio-secret' not in tui.all_output
            tui.pick('Identifier')
            tui.expect('Ctrl+U')
            tui.key('\x15renamed\r')
            tui.expect('Station / renamed')
            assert settings('icecast')['station'] == 'renamed'
            saved_playlist = root / 'config/mz/icecast.playlist'
            assert not saved_playlist.exists() or saved_playlist.read_text().strip() == 'default'
            tui.key('\x1b')
            tui.expect('Radio stations')
            tui.pick('+ Add station')
            for value in ['second', 'Second station', 'https://example.invalid/live.mp3']:
                tui.expect('Ctrl+U')
                tui.key(value + '\r')
            tui.expect('Second station')
            assert len(settings('icecast')['stations']) == 2
            tui.pick('Second station')
            tui.pick('Delete station')
            tui.expect('Delete this station?')
            tui.pick('> Delete')
            tui.expect('Radio stations')
            assert len(settings('icecast')['stations']) == 1
        finally:
            tui.close()
        # Language persists across processes; all YouMZ settings work before login.
        tui = Terminal(env, 'youmz')
        try:
            tui.expect('Login browser')
            tui.edit('Login browser', '/example path/firefox')
            tui.expect('/example path/firefox')
            assert settings('youmz')['browser'] == '/example path/firefox'
            tui.edit('Proxy', 'socks5h://127.0.0.1:2080')
            tui.expect('YouTube Music')
            assert settings('youmz')['proxy'] == 'socks5h://127.0.0.1:2080'
            tui.edit('YouTube session', 'SID=test-session')
            tui.expect('YouTube session')
            assert settings('youmz')['session'] == '***'
            assert b'SID=test-session' not in tui.all_output
        finally:
            tui.close()
        # Offline YMZ still presents every preference and login, without any token.
        tui = Terminal(env, 'ymz')
        try:
            screen = tui.expect('Yandex Music')
            for text in ['Wave mood', 'Wave variety', 'Song language', 'Yandex Music token', 'Log in']:
                assert text in screen, screen
        finally:
            tui.close()
        # Unknown scalar/nested fields remain editable, and external login restores the TUI.
        tui = Terminal(env, 'custom')
        try:
            tui.expect('Custom provider')
            tui.edit('quality', 'high')
            tui.edit('nested', '{"enabled":false}')
            stored = json.loads((custom / 'settings.json').read_text())
            assert stored['quality'] == 'high' and stored['nested'] == {'enabled': False}
            tui.pick('Log in / renew')
            tui.expect('Test login: enter done')
            tui.key('done\r')
            tui.expect('authorized: Yes')
        finally:
            tui.close()
        # Real playback: saving through the TUI applies settings to the active host via IPC.
        subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi', '-i',
                        'sine=frequency=440:duration=120', str(music / 'tone.flac')], check=True)
        alsa = root / 'alsa.conf'
        alsa.write_text('pcm.!default { type null }\n')
        env.update(ALSA_CONFIG_PATH=str(alsa), DBUS_SESSION_BUS_ADDRESS='unix:path=' + str(root / 'no-bus'))
        with (root / 'host.log').open('w+') as log:
            host = subprocess.Popen([str(BIN / 'mz'), 'local'], env=env, stdout=log, stderr=log)
            def ipc(action, **extra):
                with socket.socket(socket.AF_UNIX) as channel:
                    channel.settimeout(10)
                    channel.connect(str(root / 'musiczero.sock'))
                    channel.sendall(json.dumps(dict(action=action, **extra)).encode() + b'\n')
                    with channel.makefile('rb') as reader:
                        return json.loads(reader.readline())
            try:
                deadline = time.monotonic() + 20
                while time.monotonic() < deadline:
                    try:
                        if ipc('status').get('can_seek'):
                            break
                    except OSError:
                        pass
                    time.sleep(.05)
                else:
                    log.seek(0)
                    raise AssertionError('Local host failed: ' + log.read())
                cli('pause')
                assert list(audio.iterdir()), 'Host ignored the configured temporary audio folder'
                tui = Terminal(env, 'local')
                try:
                    tui.expect('Local music')
                    tui.pick('Shuffle tracks')
                    tui.pick('Yes')
                    tui.expect('Shuffle tracks: Yes')
                    assert settings('local')['shuffle'] is True
                    assert ipc('status')['track_id'] == '', 'TUI did not reset the active generation'
                    assert 'error' in ipc('set-setting', module='icecast', key='path', value='/wrong')
                    assert settings('local')['path'] == str(music)
                    tui.pick('Playlist:')
                    tui.expect('Playlists')
                    tui.pick(str(music))
                    tui.expect('Local music')
                    assert (root / 'config/mz/local.playlist').read_text().strip() == 'all'
                finally:
                    tui.close()
                cli('quit')
                assert host.wait(timeout=10) == 0
            finally:
                if host.poll() is None:
                    host.kill()
                    host.wait()
        print('Config TUI: host preferences/active IPC, custom fields/login and playlist selection: OK')
        print('Config TUI: RU/EN persistence, all provider forms, Unicode, validation/cancel, station secrets/add/delete and terminal restoration: OK')


if __name__ == '__main__':
    main()
