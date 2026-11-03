"""Isolated PTY acceptance: no production state, OAuth, services or requests."""
import fcntl, struct, json, os, pathlib, pty, re, select, signal, subprocess, sys, tempfile, time, termios
binary=str(pathlib.Path(sys.argv[1]).resolve())
checks=0
with tempfile.TemporaryDirectory(prefix='kairo-panel-') as temp:
 root=pathlib.Path(temp); data=root/'data'; tools=root/'tools';tools.mkdir()
 def tool(name,body):
  p=tools/name;p.write_text('#!/usr/bin/env python3\n'+body);p.chmod(0o700)
 tool('systemctl', '''import os,pathlib,sys
p=pathlib.Path(os.environ['MOCK_SERVICE']); a=sys.argv[1]
if 'codex-window-scheduler.service' in sys.argv:print('inactive');sys.exit(0)
if a in ('start','restart','stop'): p.write_text('inactive' if a=='stop' or os.environ.get('MOCK_START_FAIL') else 'active')
if a=='show': print('ActiveState='+ (p.read_text() if p.exists() else 'inactive')+'\\nLoadState=loaded\\nMainPID=42\\nExecStart=daemon --observe')
if a=='is-enabled':print('enabled')
''')
 tool('journalctl', '''import json
for n in range(65):print(json.dumps({'MESSAGE':json.dumps({'event':'observation_only','n':n}),'PRIORITY':'3' if n==0 else '6','__REALTIME_TIMESTAMP':'1793300000000000'}))
''')
 env={**os.environ,'PATH':str(tools)+':'+os.environ['PATH'],'KAIRO_OFFLINE':'1','MOCK_SERVICE':str(root/'service'),'TERM':'xterm-256color','COLUMNS':'32'}
 def direct(*args,ok=True):
  p=subprocess.run([binary,'--data-dir',str(data),*args],env=env,capture_output=True,text=True,timeout=10)
  assert (p.returncode==0)==ok,(args,p.stdout,p.stderr);return p
 (root/'auth.json').write_text(json.dumps({'tokens':{'account_id':'synthetic-panel','access_token':'synthetic-panel-placeholder'}}));(root/'auth.json').chmod(0o600)
 direct('init');expected_version=direct('--version').stdout.strip().split()[-1];direct('runtime','--home',temp,'--codex-home',temp,'--codex','/nonexistent-kairo-synthetic')
 def menu(text,extra=None,interrupt=False,modern=False):
  global checks
  m,s=pty.openpty(); fcntl.ioctl(s,termios.TIOCSWINSZ,struct.pack("HHHH",24,24,0,0)); before=termios.tcgetattr(s)
  p=subprocess.Popen([binary,'--data-dir',str(data)],stdin=s,stdout=s,stderr=s,env={**env,**(extra or {})},start_new_session=True)
  os.write(m,text.encode());out=b'';deadline=time.monotonic()+15
  if interrupt: time.sleep(.2);p.send_signal(signal.SIGINT)
  while time.monotonic()<deadline:
   if select.select([m],[],[],.05)[0]:
    try:out+=os.read(m,65536)
    except OSError:break
   if p.poll() is not None:
    while select.select([m],[],[],0)[0]:
     try:out+=os.read(m,65536)
     except OSError:break
    break
  if p.poll() is None:p.kill();raise AssertionError(('menu hung',text,out[-1500:]))
  assert p.returncode in ((-signal.SIGINT,130) if interrupt else (0,)),(text,p.returncode,out[-2000:])
  assert termios.tcgetattr(s)==before,'Terminal modes changed'
  os.close(m);os.close(s);checks+=1
  decoded=out.decode(errors='replace');assert 'synthetic-panel-placeholder' not in decoded,'Credential leaked';return decoded
 def plain(s):return re.sub(r'\x1b\[[0-9;]*[mHJ]','',s).replace('\r','')
 for lang in ('zh-CN','en-US'):
  direct('lang',lang)
  # All seven home actions; returning never exits accidentally.
  for n in (1,2,3,4,5):
   out=menu(f'{n}\n0\n0\n');assert 'KAIRO / ' in out
   assert '\x1b[2J\x1b[H' in out
  menu('4\n3\n\n2\n\n0\n0\n')
  menu('3\n1\n\n0\n0\n');assert json.loads((data/'onboarding.json').read_text())['requested']
  menu('3\n2\n\n0\n0\n');assert not json.loads((data/'onboarding.json').read_text())['requested']
  menu('3\n1\n\n0\n0\n')
  out=plain(menu('2\n\n99\n00\n1\n02:00\n\n2\n02:00\n03:00\n\n3\n03:00\n\n5\n\n6\n\n4\n\n0\n0\n'))
  assert out.count('[ KAIRO ]')==2
  assert ('菜单选项无效' if lang=='zh-CN' else 'Invalid selection') in out
  assert len(json.loads(direct('--json','schedule','list').stdout)['anchors'])==4
  # Every nested page returns through advanced -> home, including legacy maintenance wizard.
  for n in (1,2,3,4,5,6,8):
   menu(f'5\n{n}\ninvalid\n00\n0\n0\n0\n')
  for path in ['5\n1\n6\n\n0\n0\n0\n','5\n3\n1\n\n0\n0\n0\n','5\n4\n1\n\n0\n0\n0\n','5\n5\n1\n\n0\n0\n0\n','5\n8\n1\n\n0\n0\n0\n','5\n6\n9\n0\n0\n0\n0\n']:
   menu(path)
  menu('5\n2\n1\n2\n3\n4\n5\n0\n0\n0\n')
  out=menu('5\n7\n\n0\n0\n')
  assert '"prewarm_conditional": true' in out and f'"version": "{expected_version}"' in out
  out=plain(menu('7\n\n0\n'))
  assert ('默认保留 OAuth' if lang=='zh-CN' else 'Preserves OAuth') in out
  assert ('不会删除已有的 Codex CLI' if lang=='zh-CN' else 'Keeps your existing Codex CLI') in out
  assert out.count('Type uninstall' if lang=='en-US' else '输入 uninstall')==1
  out=plain(menu('garbage\n00\n8\n0\n'))
  assert out.count('[ KAIRO ]')==1
  for internal in ['deployment.json','FRESH','RECOVER','ENABLE','生产切换','安全恢复复查']:
   assert internal not in out
  assert not re.search(r'^0[0-9]\.',out,re.M)
  assert len(re.findall(r'^0\. ',out,re.M))==1
  assert ('下次预热机会' if lang=='zh-CN' else 'Next warmup opportunity') in out
  # Actual ioctl width takes precedence; exercise 24 columns, NO_COLOR and dumb terminals.
  out=menu('0\n',{'NO_COLOR':'1'});assert not re.search(r'\x1b\[[0-9;]*m',out)
  out=menu('0\n',{'TERM':'dumb'});assert '\x1b' not in out
  out=menu('6\n0\n');assert ('Status and quota' if lang=='zh-CN' else '查看状态与额度') in out
  assert json.loads((data/'config.json').read_text())['language']==('en-US' if lang=='zh-CN' else 'zh-CN')
  out=menu('0\n');assert ('Status and quota' if lang=='zh-CN' else '查看状态与额度') in out
 for path in ['', '1\n', '2\n', '3\n', '4\n', '5\n1\n', '5\n2\n', '5\n6\n9\n', '7\n']:
  menu(path+'\x04');menu(path,interrupt=True)
 p=direct(ok=False);assert '\x1b' not in p.stdout+p.stderr
 v=json.loads(direct('--json','status').stdout);assert v['version']==expected_version and 'state_valid' in v
 assert len(json.loads(direct('--json','schedule','list').stdout)['anchors'])==4
 direct('--json','enable')
 assert json.loads((data/'onboarding.json').read_text())['requested']
 assert not (data/'deployment.json').exists() and not (data/'state.json').exists()
print(f'{checks} bilingual PTY sessions passed; all home/nested menus, persistence, JSON, EOF, SIGINT and terminal modes verified')
