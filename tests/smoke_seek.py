"""Offline CLI/IPC/MPRIS seeking through the real host and decoders (Linux)."""
import json
import os
import pathlib
import socket
import shutil
import sys
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BIN = ROOT / 'target' / os.environ.get('MZ_TEST_PROFILE', 'debug')


def check():
    with tempfile.TemporaryDirectory(prefix='mz-seek-') as tmp:
        root = pathlib.Path(tmp)
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_CACHE_HOME=str(root / 'cache'),
                   XDG_RUNTIME_DIR=tmp, DBUS_SESSION_BUS_ADDRESS=os.environ['DBUS_SESSION_BUS_ADDRESS']
                   if '--in-session' in sys.argv else 'unix:path=' + tmp + '/no-bus')
        alsa = root / 'alsa.conf'
        alsa.write_text('pcm.!default { type null }\n')
        env['ALSA_CONFIG_PATH'] = str(alsa)
        folder = root / 'modules/demo'
        folder.mkdir(parents=True)
        manifest = dict(protocol=1, id='demo', name='Seek test', binary='demo-module', default_playlist='all')
        (folder / 'module.json').write_text(json.dumps(manifest))
        # Finite audio with deliberately absent provider duration: decoding must recover it.
        subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi', '-i',
                        'sine=frequency=440:duration=300', '-c:a', 'aac', str(folder / 'tone.m4a')], check=True)
        (folder / 'demo-module').write_text(
            '#!/usr/bin/env python3\nimport json,sys,pathlib\n'
            'command=sys.argv[1]\n'
            'if command=="info": print(' + repr(json.dumps(manifest)) + ')\n'
            'elif command=="tracks": print(json.dumps({"tracks":[{"id":"one","title":"Synthetic","artist":"Test","duration_ms":0}]}))\n'
            'elif command=="audio": sys.stdout.buffer.write(pathlib.Path(__file__).with_name("tone.m4a").read_bytes())\n')
        (folder / 'demo-module').chmod(0o755)
        env['MZ_MODULES_DIR'] = str(root / 'modules')
        host = str(BIN / 'mz')
        def cli(*args, success=True):
            result = subprocess.run([host, *args], env=env, capture_output=True, text=True, timeout=10)
            assert (result.returncode == 0) == success, (args, result.stdout, result.stderr)
            return result
        def ipc(action, value='', key=''):
            with socket.socket(socket.AF_UNIX) as channel:
                channel.settimeout(10)
                channel.connect(str(root / 'musiczero.sock'))
                channel.sendall(json.dumps(dict(action=action, value=value, key=key)).encode() + b'\n')
                with channel.makefile('rb') as reader:
                    return json.loads(reader.readline())
        with (root / 'host.log').open('w+') as log:
            proc = subprocess.Popen([host, 'demo'], env=env, stdout=log, stderr=log)
            try:
                for _ in range(300):
                    try:
                        status = ipc('status')
                        if status.get('can_seek'):
                            break
                    except (OSError, ValueError):
                        pass
                    if proc.poll() is not None:
                        log.seek(0)
                        raise RuntimeError(log.read())
                    time.sleep(.03)
                else:
                    raise RuntimeError('Host failed to load audio')
                cli('pause')
                status = ipc('status')
                assert status['duration_ms'] > 290_000, status
                assert status['status'] == 'Paused', status
                cli('seek', '1:30.25')
                after = ipc('status')
                assert abs(after['position_ms'] - 90_250) < 50, after
                assert after['status'] == 'Paused', after
                assert after['title'] == status['title'], after
                cli('seek', '-10.25')
                after = ipc('status')
                assert abs(after['position_ms'] - 80_000) < 50, after
                cli('seek', '+5')
                assert abs(ipc('status')['position_ms'] - 85_000) < 50
                if '--in-session' in sys.argv and shutil.which('busctl'):
                    def dbus(method, signature, *values):
                        return subprocess.check_output(['busctl', '--user', 'call', 'org.mpris.MediaPlayer2.mz',
                            '/org/mpris/MediaPlayer2', 'org.mpris.MediaPlayer2.Player', method, signature,
                            *values], env=env, text=True, timeout=10)
                    def wait_position(target):
                        for _ in range(100):
                            if abs(ipc('status')['position_ms'] - target) < 50:
                                return
                            time.sleep(.02)
                        raise RuntimeError('MPRIS did not seek to requested position')
                    dbus('Seek', 'x', '2000000')
                    wait_position(87_000)
                    dbus('SetPosition', 'ox', '/org/mz/Track/wrong', '1000000')
                    time.sleep(.1)
                    assert abs(ipc('status')['position_ms'] - 87_000) < 50
                    track_path = '/org/mz/Track/' + b'demo_one'.hex()
                    dbus('SetPosition', 'ox', track_path, '1000000')
                    wait_position(1_000)
                    print('MPRIS: relative Seek, absolute SetPosition, stale track rejection: OK')
                cli('seek', '-1000')
                assert ipc('status')['position_ms'] < 50
                cli('seek', 'NaN', success=False)
                assert 'error' in ipc('seek', '-1', 'absolute')
                assert 'error' in ipc('seek', '10', 'invalid')
                cli('play')
                time.sleep(.1)
                assert ipc('status')['position_ms'] > 0
                cli('stop')
                assert not ipc('status')['can_seek']
                cli('seek', '10', success=False)
                cli('quit')
                assert proc.wait(timeout=10) == 0
                print('Seek: M4A duration fallback, absolute/relative CLI, paused playback, invalid IPC, resume/stop: OK')
            finally:
                if proc.poll() is None:
                    proc.kill()
                    proc.wait()


if __name__ == '__main__':
    if shutil.which('dbus-run-session') and '--in-session' not in sys.argv:
        subprocess.run(['dbus-run-session', '--', sys.executable, __file__, '--in-session'], check=True)
    else:
        check()
