"""Explicit account integration check; uses saved app login, never browser profiles.

Run: python3 tests/check_services.py --host
Not run in CI: requires Yandex/YouTube accounts, network and yt-dlp.
"""
import argparse
import json
import os
import pathlib
import shutil
import signal
import socket
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BIN = ROOT / 'target' / os.environ.get('MZ_TEST_PROFILE', 'debug')


def run(args, env, *, output=subprocess.PIPE, timeout=90):
    with tempfile.TemporaryFile() as errors:
        process = subprocess.Popen([str(arg) for arg in args], env=env, stdout=output,
                                   stderr=errors, start_new_session=True)
        try:
            stdout, _ = process.communicate(timeout=timeout)
            if process.returncode:
                errors.seek(0)
                # Module messages contain no protocol stdout or authentication headers.
                message = errors.read(4096).decode(errors='replace')
                raise RuntimeError(f'{pathlib.Path(args[0]).name} {args[1]} failed: {message}')
            return stdout
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()


def account_check(module, env, root):
    binary = BIN / (module + '-module')
    def metadata(*args):
        return json.loads(run([binary, *args], env))
    assert metadata('info')['id'] == module
    playlists = metadata('playlists')['playlists']
    default = 'wave' if module == 'ymz' else 'RDMM'
    tracks = metadata('tracks', default)['tracks']
    assert tracks, f'{module}: empty default playlist'
    audio = root / (module + '-audio')
    with audio.open('wb') as output:
        run([binary, 'audio', tracks[0]['id']], env, output=output, timeout=180)
    assert 0 < audio.stat().st_size <= 512 * 1024 * 1024
    run(['ffmpeg', '-nostdin', '-v', 'error', '-i', audio, '-f', 'null', '-'], env, timeout=120)
    print(f'{module}: {len(playlists)} playlists, {len(tracks)} default tracks, '
          f'{audio.stat().st_size} audio bytes, full decode OK', flush=True)


def host_check(modules, env, root, wave_tracks=0, history=False):
    folder = root / 'modules'
    for module in modules:
        target = folder / module
        target.mkdir(parents=True)
        shutil.copy2(ROOT / 'modules' / module / 'module.json', target)
        shutil.copy2(BIN / (module + '-module'), target)
    alsa = root / 'alsa.conf'
    alsa.write_text('pcm.!default { type null }\n')
    env.update(MZ_MODULES_DIR=str(folder), ALSA_CONFIG_PATH=str(alsa),
               DBUS_SESSION_BUS_ADDRESS='unix:path=' + str(root / 'no-bus'))
    host = BIN / 'mz'
    def ipc(action, value='', key=''):
        with socket.socket(socket.AF_UNIX) as channel:
            channel.settimeout(100)
            channel.connect(str(root / 'musiczero.sock'))
            channel.sendall(json.dumps(dict(action=action, value=value, key=key)).encode() + b'\n')
            with channel.makefile('rb') as reader:
                result = json.loads(reader.readline())
        if 'error' in result:
            raise RuntimeError(result['error'])
        return result
    with (root / 'host.log').open('w+') as log:
        process = subprocess.Popen([str(host), modules[0]], env=env, stdout=log, stderr=log,
                                   start_new_session=True)
        try:
            for index, module in enumerate(modules):
                if index:
                    run([host, 'switch', module], env)
                deadline = time.monotonic() + 180
                while time.monotonic() < deadline:
                    try:
                        status = ipc('status')
                        if status['module'] == module and status.get('can_seek'):
                            break
                    except (OSError, ValueError):
                        pass
                    if process.poll() is not None:
                        raise RuntimeError('Audio host exited; service did not start')
                    time.sleep(.05)
                else:
                    raise RuntimeError(f'{module}: host did not start finite audio within 180 seconds')
                run([host, 'pause'], env)
                initial = ipc('status')
                run([host, 'seek', '30'], env)
                after = ipc('status')
                assert abs(after['position_ms'] - 30_000) < 100, after
                assert after['status'] == 'Paused' and after['title'] == initial['title']
                run([host, 'seek', '-10'], env)
                assert abs(ipc('status')['position_ms'] - 20_000) < 100
                run([host, 'seek', '+5'], env)
                assert abs(ipc('status')['position_ms'] - 25_000) < 100
                run([host, 'play'], env)
                time.sleep(.1)
                assert ipc('status')['position_ms'] > 25_000
                print(f'{module}: real host playback, paused absolute/relative seek, resume OK', flush=True)
                if module == 'ymz' and wave_tracks:
                    seen = {ipc('status')['track_id']}
                    while len(seen) < wave_tracks:
                        run([host, 'next'], env)
                        deadline = time.monotonic() + 180
                        while time.monotonic() < deadline:
                            status = ipc('status')
                            if status.get('can_seek') and status.get('track_id') not in seen:
                                seen.add(status['track_id'])
                                break
                            if process.poll() is not None:
                                raise RuntimeError('Audio host exited during wave continuation')
                            time.sleep(.1)
                        else:
                            raise RuntimeError(f'Wave stopped after {len(seen)} distinct tracks')
                    print(f'ymz: {len(seen)} distinct real wave tracks downloaded and started '
                          'across batch boundaries using next: OK', flush=True)
                if history:
                    original_id = ipc('status')['track_id']
                    if not ipc('history')['tracks']:
                        run([host, 'next'], env)
                        deadline = time.monotonic() + 180
                        while time.monotonic() < deadline:
                            status = ipc('status')
                            if status.get('can_seek') and status['track_id'] != original_id:
                                original_id = status['track_id']
                                break
                            time.sleep(.1)
                        else:
                            raise RuntimeError(f'{module}: next track failed before history check')
                    tracks = ipc('history')['tracks']
                    assert 1 <= len(tracks) <= 5, tracks
                    previous_id = tracks[0]['id']
                    for action, value, expected in [('previous', '', previous_id), ('next', '', original_id),
                                                     ('replay', '1', previous_id), ('next', '', original_id)]:
                        ipc(action, value)
                        deadline = time.monotonic() + 180
                        while time.monotonic() < deadline:
                            status = ipc('status')
                            if status.get('can_seek') and status['track_id'] == expected:
                                break
                            if process.poll() is not None:
                                raise RuntimeError('Audio host exited while replaying history')
                            time.sleep(.1)
                        else:
                            raise RuntimeError(f'{module}: {action} did not start historical audio')
                        ipc('pause')
                        assert ipc('status')['position_ms'] < 2_000
                    print(f'{module}: real audio Previous, selected replay and return to interrupted track OK',
                          flush=True)
            run([host, 'quit'], env)
            assert process.wait(timeout=10) == 0
            log.seek(0)
            messages = log.read()
            assert 'feedback:' not in messages and 'feedback timed out' not in messages, messages
            assert 'radioStarted:' not in messages, messages
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--module', choices=['ymz', 'youmz', 'both'], default='both')
    parser.add_argument('--host', action='store_true', help='also check host playback/seeking with ALSA null')
    parser.add_argument('--wave-tracks', type=int, default=0,
                        help='with --host, start this many distinct real wave tracks using next (at least 6)')
    parser.add_argument('--history', action='store_true', help='with --host, replay real previous tracks')
    args = parser.parse_args()
    if args.wave_tracks and (args.wave_tracks < 6 or not args.host or args.module == 'youmz'):
        parser.error('--wave-tracks requires --host, YMZ and a count of at least 6')
    if args.history and not args.host:
        parser.error('--history requires --host')
    modules = ['ymz', 'youmz'] if args.module == 'both' else [args.module]
    original = pathlib.Path(os.environ.get('XDG_CONFIG_HOME') or pathlib.Path.home() / '.config')
    with tempfile.TemporaryDirectory(prefix='mz-services-') as tmp:
        root = pathlib.Path(tmp)
        for module in modules:
            target = root / 'config' / module
            target.mkdir(parents=True)
            # Only existing app credentials, not cookie exports or browser profile data.
            for name in (['token'] if module == 'ymz' else ['session', 'proxy']):
                source = original / module / name
                if source.is_file():
                    shutil.copy2(source, target / name)
                    (target / name).chmod(0o600)
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_CACHE_HOME=str(root / 'cache'),
                   XDG_RUNTIME_DIR=tmp, TMPDIR=tmp, RUST_LOG='warn,mz=info')
        env.pop('MZ_MODULES_DIR', None)
        for module in modules:
            account_check(module, env, root)
        if args.host:
            host_check(modules, env, root, args.wave_tracks, args.history)


if __name__ == '__main__':
    main()
