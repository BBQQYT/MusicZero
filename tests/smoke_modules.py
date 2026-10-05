import pathlib,tempfile,subprocess,os,json,threading,http.server,time,base64,struct,math,sys
repo=pathlib.Path(__file__).resolve().parents[1]; bin=repo/'target'/os.environ.get('MZ_TEST_PROFILE','debug')
with tempfile.TemporaryDirectory(prefix='mz-new-modules-') as tmp:
    root=pathlib.Path(tmp); library=root/'music'; library.mkdir()
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_CACHE_HOME=str(root/'cache'),XDG_RUNTIME_DIR=tmp,DBUS_SESSION_BUS_ADDRESS='unix:path='+tmp+'/no-bus')
    def module(name,*args):
        r=subprocess.run([str(bin/(name+'-module')),*args],env=env,capture_output=True,timeout=60)
        if r.returncode: raise RuntimeError(name+': '+r.stderr.decode())
        return r.stdout
    def json_module(name,*args): return json.loads(module(name,*args))
    formats={'wav':'pcm_s16le','flac':'flac','mp3':'libmp3lame','ogg':'libvorbis','opus':'libopus','m4a':'aac','aiff':'pcm_s16be','au':'pcm_mulaw','voc':'pcm_u8','wv':'wavpack','tta':'tta','caf':'alac','wma':'wmav2','ac3':'ac3'}
    for suffix,codec in formats.items():
        subprocess.run(['ffmpeg','-nostdin','-hide_banner','-loglevel','error','-f','lavfi','-i','sine=frequency=440:duration=0.15','-c:a',codec,'-metadata','title=MusicZero-'+suffix,'-metadata','artist=Synthetic',str(library/('tone.'+suffix))],check=True)
    (library/'renamed-no-extension').write_bytes((library/'tone.flac').read_bytes())
    # A synthetic ProTracker MOD with a looped sine sample and a short pattern.
    mod=bytearray(b'MusicZero test'.ljust(20,b'\0'))
    for i in range(31): mod+=b'sine'.ljust(22,b'\0')+struct.pack('>HBBHH',32 if i==0 else 0,0,64 if i==0 else 0,0,32 if i==0 else 1)
    mod+=bytes([1,0])+bytes(128)+b'M.K.'
    pattern=bytearray(1024); pattern[0:4]=bytes([1,172,16,0]);pattern[4*16+2]=13
    mod+=pattern+bytes((int(100*math.sin(i*math.tau/64))%256 for i in range(64)))
    (library/'legacy.mod').write_bytes(mod)
    # Distinct legacy byte filenames must survive the index and resolve to audio.
    byte_names = [b'byte-\xff.flac', b'byte-\xfe.flac'] if os.name == 'posix' else []
    for name in byte_names:
        with open(os.fsencode(library) + b'/' + name, 'wb') as output:
            output.write((library/'tone.flac').read_bytes())
    json_module('local','set-setting','path',str(library))
    tracks=json_module('local','tracks','all')['tracks']
    assert len(tracks)==len(formats)+2+len(byte_names),(len(tracks),len(formats))
    assert len({track['id'] for track in tracks})==len(tracks)
    cached_tracks=json_module('local','tracks','all')['tracks']
    assert {track['id'] for track in cached_tracks}=={track['id'] for track in tracks}
    assert any(t['title']=='MusicZero-ogg' and t['artist']=='Synthetic' for t in tracks)
    for track in tracks:
        audio=module('local','audio',track['id']); assert audio.startswith(b'fLaC')
        r=subprocess.run(['ffmpeg','-nostdin','-v','error','-i','pipe:0','-f','null','-'],input=audio,capture_output=True)
        assert r.returncode==0,r.stderr
    print('Local: scanned and decoded',len(tracks),'files, including FLAC, AU, VOC, AIFF, WavPack, TTA, tracker MOD and a file without extension.',flush=True)
    marker=root/'auth-ok'
    data=(library/'tone.mp3').read_bytes()
    class Radio(http.server.BaseHTTPRequestHandler):
        def log_message(self,*args): pass
        def do_GET(self):
            expected='Basic '+base64.b64encode(b'listener:dummy-test-password').decode()
            if self.headers.get('Authorization')!=expected:
                self.send_response(401);self.end_headers();return
            marker.touch()
            self.send_response(200);self.send_header('Content-Type','audio/mpeg');self.send_header('icy-name','MusicZero Test');self.end_headers()
            try:
                while True: self.wfile.write(data);self.wfile.flush();time.sleep(.08)
            except (BrokenPipeError,ConnectionResetError): pass
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Radio);threading.Thread(target=server.serve_forever,daemon=True).start()
    url=f'http://127.0.0.1:{server.server_port}/radio.mp3'
    for key,val in [('url',url),('username','listener'),('password','dummy-test-password'),('buffer_ms','250')]: json_module('icecast','set-setting',key,val)
    assert json_module('icecast','settings')['settings']['stations'][0]['password']=='***'
    modules=root/'modules';modules.mkdir()
    for name in ('local','icecast'):
        folder=modules/name;folder.mkdir();(folder/'module.json').write_bytes((repo/'modules'/name/'module.json').read_bytes());(folder/(name+'-module')).write_bytes((bin/(name+'-module')).read_bytes());(folder/(name+'-module')).chmod(0o700)
    alsa=root/'alsa.conf';alsa.write_text('pcm.!default { type null }\n')
    env.update(MZ_MODULES_DIR=str(modules),ALSA_CONFIG_PATH=str(alsa))
    host=str(bin/'mz')
    def cli(*args):
        r=subprocess.run([host,*args],env=env,capture_output=True,text=True,timeout=10)
        assert r.returncode==0,r.stderr
        return r.stdout
    with (root/'host.log').open('w+') as log:
        proc=subprocess.Popen([host,'icecast'],env=env,stdout=log,stderr=log)
        try:
            for _ in range(300):
                log.seek(0);output=log.read()
                if '▶ Icecast Radio — Icecast' in output: break
                if proc.poll() is not None: raise RuntimeError(output)
                time.sleep(.05)
            else: raise RuntimeError('Radio failed to play: '+output)
            assert marker.exists()
            print('Icecast: infinite HTTP station with Basic auth started playing before EOF.',flush=True)
            no_seek=subprocess.run([host,'seek','10'],env=env,capture_output=True,text=True,timeout=10)
            assert no_seek.returncode!=0 and 'прямого эфира' in no_seek.stderr,no_seek.stderr
            cli('set','icecast','buffer_ms','500')
            cli('switch','local')
            for _ in range(300):
                if 'Локальная папка' in cli('status') and 'Playing' in cli('status'): break
                time.sleep(.03)
            else:
                log.seek(0);raise RuntimeError('Local playback failed: '+log.read())
            cli('quit');assert proc.wait(timeout=10)==0
            print('Host: live settings, radio-to-local switch, local playback, and quit succeeded.',flush=True)
        finally:
            if proc.poll() is None: proc.kill();proc.wait()
            server.shutdown()
