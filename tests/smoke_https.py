import pathlib,tempfile,subprocess,os,http.server,ssl,threading,time,select,json
repo=pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='mz-https-') as tmp:
    root=pathlib.Path(tmp)
    cert=root/'ca.pem'; key=root/'key.pem'
    subprocess.run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost','-days','1','-keyout',str(key),'-out',str(cert)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True)
    audio=subprocess.run(['ffmpeg','-nostdin','-v','error','-f','lavfi','-i','sine=frequency=440:duration=1','-c:a','libmp3lame','-f','mp3','pipe:1'],stdout=subprocess.PIPE,check=True).stdout
    class Station(http.server.BaseHTTPRequestHandler):
        def log_message(self,*args): pass
        def do_GET(self):
            self.send_response(200); self.send_header('Content-Type','audio/mpeg');self.end_headers()
            try:
                while True: self.wfile.write(audio);self.wfile.flush();time.sleep(.1)
            except (BrokenPipeError,ConnectionResetError,ssl.SSLError): pass
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Station)
    ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(cert,key);server.socket=ctx.wrap_socket(server.socket,server_side=True)
    threading.Thread(target=server.serve_forever,daemon=True).start()
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'))
    binary=str(repo/'target'/os.environ.get('MZ_TEST_PROFILE','debug')/'icecast-module')
    for name,value in [('url',f'https://localhost:{server.server_port}/live.mp3'),('ca_file',str(cert)),('reconnect','false')]:
        r=subprocess.run([binary,'set-setting',name,value],env=env,capture_output=True);assert r.returncode==0,r.stderr
    proc=subprocess.Popen([binary,'audio','default'],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    try:
        assert select.select([proc.stdout],[],[],15)[0], 'HTTPS produced no audio'
        assert len(os.read(proc.stdout.fileno(),3840))>0,proc.stderr.read().decode()
        print('HTTPS radio: verified custom CA certificate and produced live PCM.')
    finally:
        proc.kill();proc.wait();server.shutdown()
