"""Compare README menu tables to the actual shipping binary, not a planned UI."""
import os,pathlib,pty,subprocess,re,sys,tempfile
repo=pathlib.Path(__file__).resolve().parents[1];binary=str(pathlib.Path(sys.argv[1]).resolve())
def strip(s):return re.sub(r'\x1b\[[0-9;]*m','',s)
with tempfile.TemporaryDirectory() as data:
 env={**os.environ,'KAIRO_OFFLINE':'1','TERM':'xterm-256color'};env.pop('NO_COLOR',None)
 def direct(*args):subprocess.run([binary,'--data-dir',data,*args],check=True,stdout=subprocess.DEVNULL,env=env)
 def menu(text):
  m,s=pty.openpty();p=subprocess.Popen([binary,'--data-dir',data],stdin=s,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env);os.close(s);os.write(m,text.encode());out,err=p.communicate(timeout=15);os.close(m);assert p.returncode==0,err;return strip(out)
 direct('init')
 direct('runtime','--home',data,'--codex-home',data,'--codex','/nonexistent-kairo-mock')
 for language,file in [('zh-CN','README.md'),('en-US','README.en.md')]:
  direct('lang',language);readme=(repo/file).read_text()
  output=menu('00\n');actual=re.findall(r'^(\d{2})\. (.+)$',output,re.M)
  assert [n for n,_ in actual]==[f'{n:02}' for n in range(1,7)]+['00']
  for number,name in actual:assert f'| {number} | {name} |' in readme,(file,number,name)
  output=menu('02\n00\n00\n');sub=re.findall(r'^(\d{2})\. (.+)$',output,re.M)
  # Find the seven actual activation submenu rows, independent of repeated main menus.
print('Both six-entry README menus match shipping CLI')
