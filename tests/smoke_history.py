"""Real Linux host: persistent five-track history, replay, queue and wave feedback."""
import json
import os
import pathlib
import shutil
import socket
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BIN = ROOT / 'target' / os.environ.get('MZ_TEST_PROFILE', 'debug')

PROVIDER = r'''#!/usr/bin/env python3
import json,pathlib,sys,time
root=pathlib.Path(__file__).parent
module=root.name
command=sys.argv[1]
log=root/'events.jsonl'
def record(value):
    with log.open('a') as output: output.write(json.dumps(value)+'\n')
if command=='info':
    print(json.dumps(dict(protocol=1,id=module,name=module,default_playlist='all')))
elif command=='playlists':
    print(json.dumps(dict(playlists=[dict(id='all',name='All')])))
elif command=='tracks':
    after=sys.argv[3] if len(sys.argv)>3 else None
    record(dict(kind='tracks',after=after))
    start=int(after)+1 if after else 1
    print(json.dumps(dict(continuous=module=='demo',tracks=[dict(id=str(i),title='Track '+str(i),artist=module,stream=module=='radio',feedback='batch-'+str(start) if module=='demo' else '') for i in range(start,start+5)])))
elif command=='audio':
    record(dict(kind='audio',id=sys.argv[2]))
    if module=='radio':
        while True:
            sys.stdout.buffer.write(bytes(1920));sys.stdout.buffer.flush();time.sleep(.01)
    else:
        if (root/'slow').exists(): time.sleep(.4)
        sys.stdout.buffer.write((root/'tone.flac').read_bytes())
elif command=='feedback':
    record(dict(kind=sys.argv[2],id=sys.argv[3],batch=sys.argv[4]))
    print('{"ok":true}')
'''


def check():
    with tempfile.TemporaryDirectory(prefix='mz-history-') as tmp:
        root = pathlib.Path(tmp)
        subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi', '-i',
                        'sine=frequency=440:duration=180', str(root / 'tone.flac')], check=True)
        for module in ['demo', 'other', 'radio']:
            folder = root / 'modules' / module
            folder.mkdir(parents=True)
            (folder / 'module.json').write_text(json.dumps(dict(protocol=1, id=module, name=module,
                                                               binary='provider', default_playlist='all')))
            (folder / 'provider').write_text(PROVIDER)
            (folder / 'provider').chmod(0o755)
            shutil.copy2(root / 'tone.flac', folder / 'tone.flac')
        alsa = root / 'alsa.conf'
        alsa.write_text('pcm.!default { type null }\n')
        env = dict(os.environ, MZ_MODULES_DIR=str(root / 'modules'), XDG_CONFIG_HOME=str(root / 'config'),
                   XDG_CACHE_HOME=str(root / 'cache'), XDG_RUNTIME_DIR=tmp, ALSA_CONFIG_PATH=str(alsa),
                   DBUS_SESSION_BUS_ADDRESS=os.environ['DBUS_SESSION_BUS_ADDRESS'] if '--in-session' in sys.argv
                   else 'unix:path=' + tmp + '/no-bus')
        host = str(BIN / 'mz')

        def cli(*args, success=True):
            result = subprocess.run([host, *args], env=env, capture_output=True, text=True, timeout=10)
            assert (result.returncode == 0) == success, (args, result.stdout, result.stderr)
            return result.stdout

        def ipc(action, value='', key='', **fields):
            with socket.socket(socket.AF_UNIX) as channel:
                channel.settimeout(10)
                channel.connect(str(root / 'musiczero.sock'))
                channel.sendall(json.dumps(dict(action=action, value=value, key=key, **fields)).encode() + b'\n')
                with channel.makefile('rb') as reader:
                    return json.loads(reader.readline())

        def wait(predicate, label):
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                try:
                    result = predicate()
                    if result:
                        return result
                except (OSError, ValueError):
                    pass
                time.sleep(.02)
            raise RuntimeError(label)

        def track(id, module='demo'):
            def matches():
                status = ipc('status')
                return status['module'] == module and status['track_id'] == str(id)
            wait(matches, f'Failed to play {module}/{id}')
            cli('pause')

        def events():
            path = root / 'modules/demo/events.jsonl'
            return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

        def mpris(method):
            subprocess.run(['busctl', '--user', 'call', 'org.mpris.MediaPlayer2.mz',
                            '/org/mpris/MediaPlayer2', 'org.mpris.MediaPlayer2.Player', method],
                           env=env, check=True, capture_output=True, timeout=10)

        def can_previous():
            return subprocess.check_output(['busctl', '--user', 'get-property', 'org.mpris.MediaPlayer2.mz',
                                            '/org/mpris/MediaPlayer2', 'org.mpris.MediaPlayer2.Player',
                                            'CanGoPrevious'], env=env, text=True, timeout=10).strip()

        use_mpris = '--in-session' in sys.argv and shutil.which('busctl')
        with (root / 'host.log').open('w+') as log:
            proc = subprocess.Popen([host, 'demo'], env=env, stdout=log, stderr=log)
            try:
                track(1)
                cli('prev', success=False)
                assert ipc('status')['track_id'] == '1' and not ipc('status')['can_previous']
                for id in range(2, 8):
                    cli('next')
                    track(id)
                assert [item['id'] for item in ipc('history')['tracks']] == ['6', '5', '4', '3', '2']
                if use_mpris:
                    assert can_previous() == 'b true'
                cli('prev')
                track(6)
                mpris('Previous') if use_mpris else cli('previous')
                track(5)
                for id in [6, 7, 8]:
                    cli('next')
                    track(id)
                cli('replay', '3')
                track(5)
                cli('next')
                track(8)
                for id in [7, 6, 5, 4, 3]:
                    cli('prev')
                    track(id)
                cli('prev', success=False)
                cli('replay', '0', success=False)
                cli('replay', '6', success=False)
                assert ipc('status')['track_id'] == '3' and not ipc('status')['can_previous']
                if use_mpris:
                    assert can_previous() == 'b false'
                for id in range(4, 10):
                    cli('next')
                    track(id)
                # Previous also restores a provider download that is still pending.
                (root / 'modules/demo/slow').touch()
                cli('next')
                track(10)
                wait(lambda: any(item.get('kind') == 'audio' and item.get('id') == '11' for item in events()),
                     'No pending next-track download')
                cli('prev')
                track(9)
                for id in [10, 11, 12]:
                    cli('next')
                    track(id)
                wait(lambda: len([event for event in events() if event['kind'] == 'trackStarted']) >= 12,
                     'Feedback was not delivered')
                started = [event['id'] for event in events() if event['kind'] == 'trackStarted']
                assert started == list(map(str, range(1, 13))), started
                cursors = [int(event['after']) for event in events() if event['kind'] == 'tracks' and event['after']]
                assert cursors == sorted(cursors) and max(cursors) >= 8, cursors
                cli('quit')
                assert proc.wait(timeout=10) == 0
                saved = json.loads((root / 'config/mz/demo.history.json').read_text())
                assert [item['id'] for item in saved['tracks']] == ['12', '11', '10', '9', '8'], saved
                assert 'feedback' not in saved['tracks'][0]
                assert 'Track 12' in cli('history', 'demo')
                proc = subprocess.Popen([host, 'other'], env=env, stdout=log, stderr=log)
                track(1, 'other')
                cli('next')
                track(2, 'other')
                assert [item['id'] for item in ipc('history')['tracks']] == ['1']
                cli('switch', 'demo')
                track(1)
                assert ipc('history')['tracks'][0]['id'] == '12'
                assert 'error' in ipc('replay', '12', 'id', module='other')
                assert 'ok' in ipc('replay', '12', 'id', module='demo')
                track(12)
                cli('next')
                track(1)
                cli('switch', 'radio')
                track(1, 'radio')
                cli('prev', success=False)
                cli('next')
                track(2, 'radio')
                assert ipc('history')['tracks'] == [] and not ipc('status')['can_previous']
                cli('quit')
                assert proc.wait(timeout=10) == 0
                print('History: five tracks, Previous/Next, replay selection, pending preload, wave cursor/feedback, '
                      'restart, provider isolation, live exclusion and MPRIS: OK')
            except Exception:
                log.seek(0)
                print(log.read(), file=sys.stderr)
                raise
            finally:
                if proc.poll() is None:
                    proc.kill()
                    proc.wait()


if __name__ == '__main__':
    if shutil.which('dbus-run-session') and '--in-session' not in sys.argv:
        subprocess.run(['dbus-run-session', '--', sys.executable, __file__, '--in-session'], check=True)
    else:
        check()
