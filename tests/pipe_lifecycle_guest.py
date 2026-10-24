"""Benign fixed fixtures only: streaming failures/signals and managed removal."""
import pathlib,subprocess,os,json,hashlib,tempfile,signal,time,pty
assert pathlib.Path('/.dockerenv').exists() and not pathlib.Path('/opt/kairo').exists()
root=pathlib.Path(tempfile.mkdtemp());tools=root/'bin';tools.mkdir();paths=pathlib.Path('/tmp/stable-fixtures');paths.mkdir()
for p in pathlib.Path('/assets').iterdir():
 if p.is_file():(paths/p.name).write_bytes(p.read_bytes())
(tools/'systemctl').write_text('''#!/bin/sh
case "$1" in
 show)
  case "$*" in
   *LoadState*) test ! -f /etc/systemd/system/kairo.service && echo not-found || echo loaded;;
   *FragmentPath*) echo /etc/systemd/system/kairo.service;;
   *ControlGroup*) echo '';;
   *MainPID*) echo 0;;
   *ActiveState*) echo inactive;;
  esac;;
 is-active) exit 3;;
 disable|stop|start|daemon-reload|reset-failed|kill) exit 0;;
esac
''')
(tools/'curl').write_text('''#!/usr/bin/python3
import pathlib,sys,json,time
args=sys.argv[1:];out=pathlib.Path(args[args.index('--output')+1]);url=args[-1]
assert '=https' in args and '--disable' in args
if pathlib.Path('/tmp/network-fail').exists():sys.exit(22)
if pathlib.Path('/tmp/network-block').exists():
 pathlib.Path('/tmp/network-ready').touch()
 while True:time.sleep(1)
if url.endswith('/releases/latest'):
 mode=pathlib.Path('/tmp/release-mode').read_text() if pathlib.Path('/tmp/release-mode').exists() else 'stable'
 out.write_text(json.dumps({'tag_name':'v0.2.1' if mode!='bad' else 'unexpected','draft':mode=='draft','prerelease':mode=='prerelease'}));sys.exit(0)
p=pathlib.Path('/tmp/stable-fixtures')/url.rsplit('/',1)[-1]
if not p.is_file():sys.exit(22)
out.write_bytes(p.read_bytes())
''')
for p in tools.iterdir():p.chmod(0o755)
os.environ.update(PATH=str(tools)+':'+os.environ['PATH'],KAIRO_OFFLINE='1')
pathlib.Path('/run/systemd/system').mkdir(parents=True,exist_ok=True);pathlib.Path('/usr/local/bin').mkdir(exist_ok=True)
codex=pathlib.Path('/usr/local/bin/codex');codex.write_text('#!/bin/sh\necho existing-codex-fixture\n');codex.chmod(0o755);codex_hash=hashlib.sha256(codex.read_bytes()).hexdigest()
if not pathlib.Path('/etc/localtime').exists():pathlib.Path('/etc/localtime').symlink_to('/usr/share/zoneinfo/Etc/UTC')
install=pathlib.Path('/source/install.sh').read_bytes();uninstall=pathlib.Path('/source/uninstall.sh').read_bytes();count=0
assert install.split(b"<<'KAIRO_UNINSTALL_EOF'\n",1)[1].split(b'KAIRO_UNINSTALL_EOF\n',1)[0]==uninstall

def run(content,args=(),ok=True):
 global count
 p=subprocess.run(['bash','-s','--',*args],input=content,capture_output=True,timeout=45)
 assert (p.returncode==0)==ok,(p.returncode,p.stdout[-500:],p.stderr[-1200:]);count+=1;return p
# Consumers of no bytes cannot detect a failed producer; the documented wrapper must.
p=subprocess.run(['bash','-c','(exit 22) | bash'],capture_output=True);assert p.returncode==0
p=subprocess.run(['bash','-o','pipefail','-c','(exit 22) | bash'],capture_output=True);assert p.returncode==22;count+=1
run(install[:len(install)//2],ok=False);assert not pathlib.Path('/opt/kairo').exists()
for mode in ['draft','prerelease','bad']:
 pathlib.Path('/tmp/release-mode').write_text(mode);run(install,ok=False);assert not pathlib.Path('/opt/kairo').exists()
pathlib.Path('/tmp/release-mode').unlink()
pathlib.Path('/tmp/network-fail').touch();run(install,ok=False);pathlib.Path('/tmp/network-fail').unlink()
run(install);binary=pathlib.Path('/opt/kairo/current/bin/kairo');expected=hashlib.sha256(binary.read_bytes()).hexdigest()
run(install);run(install,('--version','0.2.1'))
subprocess.run(['kairo','schedule','add','03:37'],check=True,capture_output=True)
auth=pathlib.Path('/home/kairo/.codex/auth.json');auth.parent.mkdir(parents=True,exist_ok=True);auth.write_text('{"synthetic":"not-a-real-credential"}\n');auth_hash=hashlib.sha256(auth.read_bytes()).hexdigest()
ledger=pathlib.Path('/var/lib/kairo-account-guards/synthetic-ledger');ledger.write_text('synthetic request-history marker\n');ledger_hash=hashlib.sha256(ledger.read_bytes()).hexdigest();config=pathlib.Path('/var/lib/kairo/config.json');config_hash=hashlib.sha256(config.read_bytes()).hexdigest()
subprocess.run(['kairo','update','online'],check=True,capture_output=True);count+=1
manifest=paths/'SHA256SUMS';original=manifest.read_bytes();manifest.write_bytes(b'0'*64+b'  kairo-0.2.1-x86_64-unknown-linux-gnu.tar.gz\n');run(install,ok=False);manifest.write_bytes(original)
assert hashlib.sha256(binary.read_bytes()).hexdigest()==expected
# Interrupted producer is a transport test, not a file-validation race script.
for sig in [signal.SIGHUP,signal.SIGINT,signal.SIGTERM]:
 before=set(pathlib.Path('/tmp').glob('kairo-online.*'));pathlib.Path('/tmp/network-block').touch();pathlib.Path('/tmp/network-ready').unlink(missing_ok=True)
 p=subprocess.Popen(['bash','-s'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True);p.stdin.write(install);p.stdin.close()
 for _ in range(200):
  if pathlib.Path('/tmp/network-ready').exists():break
  if p.poll() is not None:raise AssertionError(p.stderr.read())
  time.sleep(.02)
 assert pathlib.Path('/tmp/network-ready').exists();os.killpg(p.pid,sig);p.wait(timeout=10);assert p.returncode!=0
 pathlib.Path('/tmp/network-block').unlink()
 for _ in range(200):
  if set(pathlib.Path('/tmp').glob('kairo-online.*'))==before:break
  time.sleep(.02)
 assert set(pathlib.Path('/tmp').glob('kairo-online.*'))==before,(sig,p.returncode,p.stderr.read()[-600:])
 assert hashlib.sha256(binary.read_bytes()).hexdigest()==expected;count+=1
# Preserve unknown business files and explicitly report incomplete cleanup.
foreign=pathlib.Path('/opt/kairo/manager/customer-note.txt');foreign.write_text('unrelated file')
run(uninstall,ok=False);assert foreign.read_text()=='unrelated file';foreign.unlink()
run(uninstall);run(uninstall)
assert not pathlib.Path('/opt/kairo').exists() and not pathlib.Path('/usr/local/bin/kairo').exists()
assert hashlib.sha256(auth.read_bytes()).hexdigest()==auth_hash and hashlib.sha256(ledger.read_bytes()).hexdigest()==ledger_hash and hashlib.sha256(config.read_bytes()).hexdigest()==config_hash
assert hashlib.sha256(codex.read_bytes()).hexdigest()==codex_hash
run(install)
assert hashlib.sha256(auth.read_bytes()).hexdigest()==auth_hash and hashlib.sha256(config.read_bytes()).hexdigest()==config_hash
# New embedded helper also gives the existing v0.2.1 menu/CLI the improved remover.
p=subprocess.run(['kairo','system-uninstall','--yes'],capture_output=True);assert p.returncode==0,(p.stdout,p.stderr);count+=1
run(uninstall,('--purge-data',),ok=False);assert auth.exists()
run(uninstall,('--purge-data','--confirm','DELETE-KAIRO-DATA'));assert not auth.exists() and not config.exists() and not ledger.exists() and codex.exists()
assert not list(pathlib.Path('/tmp').glob('kairo-online.*'))
print(f'{count} stdin/latest-stable/signals/checksum/update/uninstall/data-preservation checks passed; container with mocked systemd, no real model requests')
