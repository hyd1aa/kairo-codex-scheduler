"""Fixed benign fixtures in a disposable container; no real model/account requests."""
import functools,hashlib,http.server,json,os,pathlib,shutil,ssl,subprocess,tempfile,threading,tarfile,io
assert pathlib.Path('/.dockerenv').exists() and not pathlib.Path('/opt/kairo').exists()
work=pathlib.Path(tempfile.mkdtemp());web=work/'web';web.mkdir();assets=web/'release';shutil.copytree('/assets',assets)
cert=work/'cert.pem';key=work/'tls.pem'
subprocess.run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost','-keyout',str(key),'-out',str(cert)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
class Handler(http.server.SimpleHTTPRequestHandler):
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Handler,directory=web));ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(cert,key);server.socket=ctx.wrap_socket(server.socket,server_side=True);threading.Thread(target=server.serve_forever,daemon=True).start()
base=f'https://localhost:{server.server_port}/release'
mock=work/'tools';mock.mkdir();(work/'base').write_text(base)
(mock/'systemctl').write_text('''#!/bin/sh
printf '%s\\n' "$*" >> /tmp/systemctl-calls
case "$1" in
 is-active) test -f /tmp/service-active;;
 stop|disable) rm -f /tmp/service-active;;
 start) test ! -f /tmp/fail-start || exit 1; touch /tmp/service-active;;
 *) exit 0;;
esac
''')
(mock/'curl').write_text('''#!/usr/bin/python3
import os,pathlib,subprocess,sys,json
args=sys.argv[1:];output=args[args.index('--output')+1];url=args[-1]
assert '--proto' in args and args[args.index('--proto')+1]=='=https'
assert '--proto-redir' in args and args[args.index('--proto-redir')+1]=='=https'
assert '--disable' in args
if pathlib.Path('/tmp/fail-network').exists():sys.exit(22)
if url.endswith('/releases/latest'):
 pathlib.Path(output).write_text(json.dumps({'tag_name':'v0.2.1','draft':False,'prerelease':False}));sys.exit(0)
if url.startswith('https://github.com/openai/codex/'):
 if pathlib.Path('/official-codex.tar.gz').exists():
  import shutil;shutil.copyfile('/official-codex.tar.gz',output);sys.exit(0)
 sys.exit(22)
if url.startswith('https://github.com/hyd1aa/kairo-codex-scheduler/releases/download/v0.2.1/'):
 url=os.environ['FIXTURE_BASE']+'/'+url.rsplit('/',1)[1]
args[-1]=url
sys.exit(subprocess.call(['/usr/bin/curl',*args]))
''')
for p in mock.iterdir():p.chmod(0o755)
os.environ.update(PATH=str(mock)+':'+os.environ['PATH'],CURL_CA_BUNDLE=str(cert),FIXTURE_BASE=base,KAIRO_OFFLINE='1')
pathlib.Path('/run/systemd/system').mkdir(parents=True,exist_ok=True)
if not pathlib.Path('/etc/localtime').exists():pathlib.Path('/etc/localtime').symlink_to('/usr/share/zoneinfo/Etc/UTC')
pathlib.Path('/usr/local/bin').mkdir(parents=True,exist_ok=True)
codex=pathlib.Path('/usr/local/bin/codex');codex.write_text('#!/bin/sh\nprintf "codex fixture\\n"\n');codex.chmod(0o755)
count=0
def call(args,ok=True,env=None):
 global count
 p=subprocess.run(args,capture_output=True,text=True,env=env,timeout=60)
 assert (p.returncode==0)==ok,(args,p.returncode,p.stdout[-500:],p.stderr[-1000:]);count+=1;return p
entry=['bash','/source/install.sh','--version','0.2.1','--base-url',base]
call(entry)
binary=pathlib.Path('/opt/kairo/current/bin/kairo');original=hashlib.sha256(binary.read_bytes()).hexdigest();state=pathlib.Path('/var/lib/kairo')
assert pathlib.Path('/usr/local/bin/kairo').is_symlink() and pathlib.Path('/etc/systemd/system/kairo.service').exists()
assert not pathlib.Path('/tmp/service-active').exists() and 'daemon --observe' in pathlib.Path('/etc/systemd/system/kairo.service.d/20-mode.conf').read_text()
assert not pathlib.Path('/home/kairo/.codex/auth.json').exists()
call(entry);call(['kairo','schedule','add','03:37']);before=(state/'config.json').read_bytes()
call(['kairo','update','online'])
assert (state/'config.json').read_bytes()==before
# Fixed corrupted download rejected before stopping a previously active service.
pathlib.Path('/tmp/service-active').touch();archive=assets/'kairo-0.2.1-x86_64-unknown-linux-gnu.tar.gz';clean=archive.read_bytes();archive.write_bytes(clean+b'changed')
call(entry,False);assert pathlib.Path('/tmp/service-active').exists() and hashlib.sha256(binary.read_bytes()).hexdigest()==original;archive.write_bytes(clean)
manifest=assets/'SHA256SUMS';clean_manifest=manifest.read_bytes();manifest.write_bytes(clean_manifest+clean_manifest.splitlines(keepends=True)[0]);call(entry,False);manifest.write_bytes(clean_manifest)
pathlib.Path('/tmp/fail-network').touch();call(entry,False);pathlib.Path('/tmp/fail-network').unlink()
# TLS failure with a fixed non-trusted CA; no -k or fallback.
env={**os.environ,'CURL_CA_BUNDLE':'/etc/ssl/certs/ca-certificates.crt'};call(entry,False,env)
# Fixed wrong-version package: all hashes match, declared binary version does not.
wrong=assets/'kairo-0.2.2-x86_64-unknown-linux-gnu.tar.gz'
with tarfile.open(fileobj=io.BytesIO(clean)) as old, tarfile.open(wrong,'w:gz') as new:
 for m in old:
  data=old.extractfile(m).read() if m.isfile() else None
  m.name=m.name.replace('kairo-0.2.1/','kairo-0.2.2/',1)
  if m.name=='kairo-0.2.1':m.name='kairo-0.2.2'
  new.addfile(m,io.BytesIO(data) if data is not None else None)
manifest.write_text(hashlib.sha256(wrong.read_bytes()).hexdigest()+'  '+wrong.name+'\n')
call(['bash','/source/install.sh','--version','0.2.2','--base-url',base],False)
manifest.write_bytes(clean_manifest);wrong.unlink()
assert pathlib.Path('/tmp/service-active').exists()
# Inert link entry: rejected by extraction before any system change.
bad=io.BytesIO()
with tarfile.open(fileobj=bad,mode='w:gz') as t:
 m=tarfile.TarInfo('kairo-0.2.1/link');m.type=tarfile.SYMTYPE;m.linkname='fixed-unused';t.addfile(m)
archive.write_bytes(bad.getvalue());manifest.write_text(hashlib.sha256(archive.read_bytes()).hexdigest()+'  '+archive.name+'\n');call(entry,False);archive.write_bytes(clean);manifest.write_bytes(clean_manifest)
# Real transaction and wrapper recovery: install known earlier binary, inject start failure.
old=pathlib.Path('/assets/previous-kairo')
if old.exists():
 pathlib.Path('/tmp/service-active').unlink(missing_ok=True)
 call(['kairo','update','apply',str(old),'--sha256',hashlib.sha256(old.read_bytes()).hexdigest(),'--prefix','/opt/kairo'])
 old_hash=hashlib.sha256(binary.read_bytes()).hexdigest();pathlib.Path('/tmp/service-active').touch();pathlib.Path('/tmp/fail-start').touch()
 call(entry,False);assert hashlib.sha256(binary.read_bytes()).hexdigest()==old_hash
 pathlib.Path('/tmp/fail-start').unlink();call(entry);assert hashlib.sha256(binary.read_bytes()).hexdigest()==original
# Optional actual official binary is used only for --version, never login/models.
if pathlib.Path('/official-codex.tar.gz').exists():
 codex.unlink();call(entry);assert codex.read_bytes()[:4]==b'\x7fELF'
# Controlled copy failure before install: missing source archive cannot change installed bytes.
call(['bash','/source/install.sh','--version','0.2.1','--source-dir','/tmp'],False)
assert hashlib.sha256(binary.read_bytes()).hexdigest()==original
call(['kairo','system-uninstall','--yes'])
assert not pathlib.Path('/usr/local/bin/kairo').exists() and not pathlib.Path('/etc/systemd/system/kairo.service').exists()
assert state.exists() and (state/'config.json').read_bytes()==before
call(entry);assert pathlib.Path('/usr/local/bin/kairo').is_symlink() and (state/'config.json').read_bytes()==before
server.shutdown();key.unlink()
print(f'{count} focused HTTPS/install/update/refusal/recovery/uninstall checks passed; container with mocked systemd, zero real model requests')
