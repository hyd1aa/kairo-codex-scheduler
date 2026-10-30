"""Focused CLI wiring regression; no helper execution, OAuth or service calls."""
import pathlib,subprocess,sys,tempfile,os,pty
binary=str(pathlib.Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory() as d:
 def call(*args,input=None):
  if input is None:
   p=subprocess.run([binary,'--data-dir',d,*args],text=True,capture_output=True,env={**os.environ,'KAIRO_OFFLINE':'1'},timeout=10)
  else:
   master,slave=pty.openpty()
   process=subprocess.Popen([binary,'--data-dir',d,*args],stdin=slave,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env={**os.environ,'KAIRO_OFFLINE':'1'})
   os.close(slave);os.write(master,input.encode());stdout,stderr=process.communicate(timeout=10);os.close(master)
   p=subprocess.CompletedProcess(process.args,process.returncode,stdout,stderr)
  assert p.returncode==0,(args,p.stderr);return p.stdout
 call('init')
 call('runtime','--home',d,'--codex-home',d,'--codex','/nonexistent-kairo-synthetic')
 assert 'online' in call('--lang','en-US','update','--help')
 assert '--yes' in call('--lang','en-US','system-uninstall','--help')
 call('lang','en-US')
 en=call('--lang','en-US',input='06\n08\nyes\nno\n\n00\n00\n');assert 'Uninstall Kairo' in en and 'Type uninstall' in en
 call('lang','zh-CN')
 zh=call('--lang','zh-CN',input='06\n08\nyes\nno\n\n00\n00\n');assert '卸载 Kairo' in zh and '输入 uninstall' in zh
print('4 online-update/uninstall CLI/menu checks passed; no system helper execution')
