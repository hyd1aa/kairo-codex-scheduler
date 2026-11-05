"""Bilingual real PTY tests; every external CLI/service boundary is synthetic."""
import fcntl,json,os,pathlib,pty,re,select,signal,struct,subprocess,sys,termios,time
from scheduled_support import Fixture
f=Fixture(sys.argv[1]);checks=0
try:
 def menu(text,extra=None,interrupt=False):
  global checks
  m,s=pty.openpty();fcntl.ioctl(s,termios.TIOCSWINSZ,struct.pack('HHHH',24,24,0,0));before=termios.tcgetattr(s)
  env={**f.env,'TERM':'xterm-256color',**(extra or {})};env.pop('NO_COLOR',None)
  if extra and 'NO_COLOR' in extra:env['NO_COLOR']=extra['NO_COLOR']
  p=subprocess.Popen([f.binary,'--data-dir',str(f.data)],stdin=s,stdout=s,stderr=s,env=env,start_new_session=True)
  os.write(m,text.encode());out=b'';deadline=time.monotonic()+15
  if interrupt:time.sleep(.2);p.send_signal(signal.SIGINT)
  while time.monotonic()<deadline:
   if select.select([m],[],[],.05)[0]:
    try:out+=os.read(m,65536)
    except OSError:break
   if p.poll() is not None:
    while select.select([m],[],[],0)[0]:
     try:out+=os.read(m,65536)
     except OSError:break
    break
  if p.poll() is None:p.kill();raise AssertionError(('menu hung',text,out[-2000:]))
  assert p.returncode in ((-signal.SIGINT,130) if interrupt else (0,)),(text,p.returncode,out[-2000:])
  assert termios.tcgetattr(s)==before;os.close(m);os.close(s);checks+=1
  return out.decode(errors='replace').replace('\r','')
 for lang in ['zh-CN','en-US']:
  f.call('lang',lang)
  for n in [1,2,3,4,5]:
   out=menu(f'{n}\ninvalid\n00\n0\n0\n');assert 'KAIRO /' in out
  menu('3\n2\n\n0\n0\n');assert f.read('schedule.json')['paused']
  menu('3\n1\n\n0\n0\n');assert not f.read('schedule.json')['paused']
  menu('2\n1\n02:00\n\n2\n02:00\n03:00\n\n3\n03:00\n\n4\n\n5\n\n6\n\n0\n0\n')
  for path in ['4\n2\n\n0\n0\n','5\n1\n0\n0\n0\n','5\n2\n\n0\n0\n','5\n3\n\n0\n0\n','5\n4\n30\n\n0\n0\n','5\n5\n0\n0\n0\n','5\n6\n\n0\n0\n','5\n7\n\n0\n0\n','1\n1\n\n0\n0\n','1\n2\n\n0\n0\n']:
   menu(path)
  out=menu('7\n\n0\n');assert ('默认保留 OAuth' if lang=='zh-CN' else 'Preserves OAuth') in out
  assert out.count('输入 uninstall' if lang=='zh-CN' else 'Type uninstall')==1
  out=menu('bad\n00\n8\n0\n');assert out.count('[ KAIRO ]')==1 and not re.search(r'^0\d\.',out,re.M)
  assert len(re.findall(r'^0\. ',out,re.M))==1 and '\x1b[2J\x1b[H' in out
  for internal in ['deployment.json','FRESH','RECOVER','ENABLE','生产授权缺失','安全恢复']:assert internal not in out
  assert not re.search(r'\x1b\[[0-9;]*m',menu('0\n',{'NO_COLOR':'1'}))
  assert '\x1b' not in menu('0\n',{'TERM':'dumb'})
  out=menu('6\n0\n');assert ('Call history and quota' if lang=='zh-CN' else '查看调用记录与额度') in out
  assert f.call('status')['config']['language']==('en-US' if lang=='zh-CN' else 'zh-CN')
 for path in ['', '1\n', '2\n','3\n','4\n','5\n1\n','7\n']:
  menu(path+'\x04');menu(path,interrupt=True)
 assert '\x1b' not in f.raw(ok=False).stdout
 assert f.call('status')['version']
 print(f'{checks} bilingual PTY sessions passed: 0–7, submenu returns, language, 24 columns, NO_COLOR, EOF and SIGINT')
finally:f.close()
