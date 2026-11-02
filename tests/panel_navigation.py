"""Isolated PTY acceptance: no production state, OAuth, services or requests."""
import json, os, pathlib, pty, re, select, signal, subprocess, sys, tempfile, time, termios
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
  # Legacy feature-case paths mapped to the simplified shipping navigation.
  first, sep, rest = text.partition('\n')
  mapping={'01':'01','02':'01','03':'02','04':'05\n05','05':'05\n01','06':'05\n09','07':'05\n02','08':'05\n04','09':'05\n03','10':'05\n06','11':'05\n07','12':'05\n08'}
  normalized=f'{int(first):02}' if first.isdigit() else first
  if normalized in mapping and not modern:
   text=mapping[normalized]+sep+rest
   if normalized in ('04','05','06','07','08','11','12'):text+='00\n'

  global checks
  m,s=pty.openpty(); before=termios.tcgetattr(s)
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
  menu('04\n03\n\n02\n\n00\n00\n',modern=True)
  menu('03\n01\n\n00\n00\n',modern=True)
  assert json.loads((data/'onboarding.json').read_text())['requested']
  menu('03\n02\n\n00\n00\n',modern=True)
  assert not json.loads((data/'onboarding.json').read_text())['requested']
  menu('03\n01\n\n00\n00\n',modern=True)
  assert json.loads((data/'onboarding.json').read_text())['requested']
  for n in (1,2,3,4,5):
   out=menu(f'{n:02}\n00\n00\n',modern=True);assert 'Kairo / ' in out
  out=plain(menu('02\n\n99\n01\n\n02\n02:00\n\n03\n02:00\n03:00\n\n04\n03:00\n\n05\n\n06\n\n07\n\n00\n00\n',modern=True))
  assert out.count('[ K A I R O ]')==2
  for path in ['05\n01\n00\n00\n00\n','05\n02\n01\n\n00\n00\n00\n','05\n03\n06\n\n00\n00\n00\n','05\n04\n01\n\n00\n00\n00\n','05\n07\n01\n\n00\n00\n00\n']:
   menu(path,modern=True)
  out=menu('05\n10\n\n00\n00\n',modern=True)
  assert '"prewarm_conditional": true' in out and f'"version": "{expected_version}"' in out
  out=menu('00\n',modern=True)
  for internal in ['deployment.json','FRESH','RECOVER','ENABLE','生产切换','安全恢复复查']:
   assert internal not in out
  assert ('下次预热机会' if lang=='zh-CN' else 'Next warmup opportunity') in out
  out=menu('00\n',{'NO_COLOR':'1'},modern=True);assert not re.search(r'\x1b\[[0-9;]*m',out)
  out=menu('00\n',{'TERM':'dumb'},modern=True);assert '\x1b' not in out
  menu('06\n',modern=True)
 menu('\x04',modern=True);menu('',interrupt=True,modern=True)
 p=direct(ok=False);assert '\x1b' not in p.stdout+p.stderr
 v=json.loads(direct('--json','status').stdout);assert v['version']==expected_version and 'state_valid' in v
 assert len(json.loads(direct('--json','schedule','list').stdout)['anchors'])==4
 direct('--json','enable')
 assert json.loads((data/'onboarding.json').read_text())['requested']
 assert not (data/'deployment.json').exists() and not (data/'state.json').exists()
print(f'{checks} bilingual PTY sessions passed; start/pause without confirmation, navigation, JSON, EOF, SIGINT and terminal modes verified')
