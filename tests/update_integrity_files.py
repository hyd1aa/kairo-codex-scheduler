"""Fixed harmless files/directories only: rejection and rollback, no races/payloads."""
import pathlib,tempfile,subprocess,os,hashlib,sys
binary=pathlib.Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix='kairo-fixed-integrity-') as tmp:
 root=pathlib.Path(tmp);data=root/'data';prefix=root/'prefix';env=dict(os.environ,KAIRO_OFFLINE='1',HOME=str(root),CODEX_HOME=str(root/'codex'))
 def run(*args,success=True):
  p=subprocess.run([str(binary),'--data-dir',str(data),*args],env=env,capture_output=True,text=True,timeout=30)
  assert (p.returncode==0)==success,(args[0],p.stderr[-300:]);return p
 digest=hashlib.sha256(binary.read_bytes()).hexdigest();run('init');run('install','--binary',str(binary),'--sha256',digest,'--prefix',str(prefix))
 installed=prefix/'bin/kairo';before=installed.read_bytes();config=(data/'config.json').read_bytes()
 plain=root/'fixed.txt';plain.write_text('This is a fixed harmless integrity test document.\n')
 def unchanged():
  assert installed.read_bytes()==before and (data/'config.json').read_bytes()==config
  assert not (prefix/'.upgrade-pending').exists() and not (data/'.upgrade-lock').exists()
 run('update','apply',str(plain),'--sha256','0'*64,'--prefix',str(prefix),success=False);unchanged()
 run('update','apply',str(plain),'--sha256',hashlib.sha256(plain.read_bytes()).hexdigest(),'--prefix',str(prefix),success=False);unchanged()
 run('update','apply',str(root/'missing.txt'),'--sha256',digest,'--prefix',str(prefix),success=False);unchanged()
 blocked=prefix/'bin/.kairo.new';blocked.mkdir()
 run('update','apply',str(binary),'--sha256',digest,'--prefix',str(prefix),success=False);unchanged();blocked.rmdir()
 bindir=prefix/'bin';mode=bindir.stat().st_mode&0o777;bindir.chmod(0o777)
 try:run('update','apply',str(binary),'--sha256',digest,'--prefix',str(prefix),success=False);unchanged()
 finally:bindir.chmod(mode)
 print('5 fixed harmless file/directory rejection and rollback checks passed; no model requests')
