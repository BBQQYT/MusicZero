"""Offline regression: real host plays multiple five-track radio batches."""
import json
import os
import pathlib
import re
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BIN = ROOT / 'target' / os.environ.get('MZ_TEST_PROFILE', 'debug')

PROVIDER = r'''#!/usr/bin/env python3
import json,pathlib,sys
root=pathlib.Path(__file__).parent
module=root.name
command=sys.argv[1]
log=root/'events.jsonl'
def events():
    return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
def record(value):
    with log.open('a') as output: output.write(json.dumps(value)+'\n')
if command=='info':
    print(json.dumps(dict(protocol=1,id=module,name=module,default_playlist='wave')))
elif command=='tracks':
    (root/'playlist').write_text(sys.argv[2])
    after=sys.argv[3] if len(sys.argv)>3 else None
    if after and not any(e.get('kind') in ('trackFinished','skip') and e.get('id')==after for e in events()):
        raise SystemExit('Continuation requested before lifecycle feedback was delivered')
    record(dict(kind='tracks',after=after,playlist=sys.argv[2]))
    start=int(after)+1 if after else 1
    print(json.dumps(dict(continuous=module=='demo', tracks=[dict(id=str(i),title='Track '+str(i),artist='Test',feedback='batch-'+str(start) if module=='demo' else '') for i in range(start,start+5)])))
elif command=='audio':
    long=(root/'playlist').read_text()=='all'
    sys.stdout.buffer.write((root/('long.flac' if long else 'tone.flac')).read_bytes())
elif command=='feedback':
    record(dict(kind=sys.argv[2],id=sys.argv[3],batch=sys.argv[4],seconds=float(sys.argv[5])))
    print('{"ok":true}')
'''


def main():
    with tempfile.TemporaryDirectory(prefix='mz-wave-') as tmp:
        root = pathlib.Path(tmp)
        for module in ['demo', 'other']:
            folder = root / 'modules' / module
            folder.mkdir(parents=True)
            (folder / 'module.json').write_text(json.dumps(dict(protocol=1, id=module, name=module,
                                                               binary='provider', default_playlist='wave')))
            (folder / 'provider').write_text(PROVIDER)
            (folder / 'provider').chmod(0o755)
            subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi', '-i',
                            'sine=frequency=440:duration=0.3', str(folder / 'tone.flac')], check=True)
            subprocess.run(['ffmpeg', '-nostdin', '-v', 'error', '-f', 'lavfi', '-i',
                            'sine=frequency=440:duration=120', str(folder / 'long.flac')], check=True)
        alsa = root / 'alsa.conf'
        alsa.write_text('pcm.!default { type null }\n')
        env = dict(os.environ, MZ_MODULES_DIR=str(root / 'modules'), XDG_CONFIG_HOME=str(root / 'config'),
                   XDG_CACHE_HOME=str(root / 'cache'), XDG_RUNTIME_DIR=tmp, ALSA_CONFIG_PATH=str(alsa),
                   DBUS_SESSION_BUS_ADDRESS='unix:path=' + tmp + '/no-bus')
        host = str(BIN / 'mz')
        def cli(*args):
            result = subprocess.run([host, *args], env=env, capture_output=True, text=True, timeout=15)
            assert result.returncode == 0, result.stderr
        def event_log(module='demo'):
            path = root / 'modules' / module / 'events.jsonl'
            return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
        with (root / 'host.log').open('w+') as log:
            proc = subprocess.Popen([host, 'demo'], env=env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 30
                while time.monotonic() < deadline:
                    log.seek(0)
                    played = list(map(int, re.findall(r'▶ Test — Track (\d+) \[demo\]', log.read())))
                    if len(played) >= 15:
                        break
                    if proc.poll() is not None:
                        log.seek(0)
                        raise RuntimeError(log.read())
                    time.sleep(.02)
                else:
                    log.seek(0)
                    raise RuntimeError('Wave did not continue: ' + log.read())
                assert played[:15] == list(range(1, 16)), played
                requests = [event for event in event_log() if event['kind'] == 'tracks']
                assert len(requests) >= 3 and requests[0]['after'] is None, requests
                assert all(request['after'] for request in requests[1:]), requests
                cli('pause')
                time.sleep(.05)
                cli('playlist', 'all')
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    requests = [event for event in event_log() if event['kind'] == 'tracks']
                    if requests[-1]['playlist'] == 'all':
                        break
                    time.sleep(.02)
                assert requests[-1]['after'] is None, requests
                # Switching must send the old track's feedback to the old provider.
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    if 'Track' in subprocess.check_output([host, 'status'], env=env, text=True):
                        break
                    time.sleep(.02)
                else:
                    raise RuntimeError('Reset playlist did not start its long track')
                cli('pause')
                cli('switch', 'other')
                time.sleep(.2)
                cli('quit')
                assert proc.wait(timeout=10) == 0
                events = event_log()
                started = [event for event in events if event['kind'] == 'trackStarted']
                finished = [event for event in events if event['kind'] == 'trackFinished']
                skipped = [event for event in events if event['kind'] == 'skip']
                assert len(started) >= 15 and len(finished) >= 14 and skipped, events
                assert all(event['seconds'] < 2 for event in finished), finished
                assert not any(event['kind'] in ('skip', 'trackStarted') for event in event_log('other'))
                print('Wave: 15 tracks across five-track batches, ordered feedback, cursor reset and provider switching: OK')
            finally:
                if proc.poll() is None:
                    proc.kill()
                    proc.wait()


if __name__ == '__main__':
    main()
