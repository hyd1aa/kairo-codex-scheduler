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
  mapping={'01':'01','02':'01','03':'02','04':'06\n05','05':'06\n01','06':'03','07':'06\n02','08':'06\n04','09':'05','10':'06\n06','11':'06\n07','12':'06\n08'}
  normalized=f'{int(first):02}' if first.isdigit() else first
  if normalized in mapping and not modern:
   text=mapping[normalized]+sep+rest
   if normalized in ('04','05','07','08','11','12'):text+='00\n'

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
  for name in ('onboarding.json','onboarding-status.json'):(data/name).unlink(missing_ok=True)
  menu('04\n01\nno\n00\n00\n',modern=True)
  assert not (data/'onboarding.json').exists()
  menu('04\n01\nyes\n\n00\n00\n',modern=True)
  intent=json.loads((data/'onboarding.json').read_text());assert intent['requested'] and intent['exclusive_account'] and intent['account']
  menu('04\n02\n\n00\n00\n',modern=True)
  assert not json.loads((data/'onboarding.json').read_text())['requested']
  menu('04\n01\n\n00\n00\n',modern=True) # one-time confirmation is remembered for the same account
  assert json.loads((data/'onboarding.json').read_text())['requested']
  direct('pause');assert not json.loads((data/'onboarding.json').read_text())['requested']
  # Every top-level page returns exactly one level; invalid selection remains local.
  for n in (1,2,3,4,5,6,7,8,9,11):
   out=menu(f'{n:02}\n00\n00\n');assert 'Kairo / ' in out
  out=plain(menu('03\n\n99\n01\n\n02\n02:00\n\n03\n02:00\n03:00\n\n04\n03:00\n\n05\n\n06\n\n07\n\n00\n00\n'))
  assert out.count('Codex Window Scheduler')==2,'Actions returned to main'
  out=menu('04\n02\n\n03\n\n04\n\n01\n\n00\n00\n');assert out.count('Codex Window Scheduler')==2
  out=menu('04\n02\n\n00\n00\n',{'MOCK_START_FAIL':'1'});assert ('尚未运行' if lang=='zh-CN' else 'not active') in out
  out=menu('05\n02\n03\n04\n05\n01\n00\n00\n');assert 'observation only' in out or '只读观察' in out or 'observation_only' in out
  menu('06\n01\n\n02\n\n03\n\n04\n\n05\nsecondary\n'+temp+'\n\n06\nsecondary\n\n07\nsecondary\n\n08\nNO\n\n00\n00\n')
  menu('06\n09\n02\n\n03\nNO\n\n04\nRECOVER\n\n05\n\n06\nENABLE\n\n07\n\n00\n00\n00\n')
  menu('07\n01\n\n02\n\n00\n00\n')
  menu('08\n02\ninvalid\n\n03\n301\n\n01\n\n07\n\n00\n00\n')
  menu('08\n02\ntrue\n\n03\n30\n\n04\n\n05\n\n06\n\n00\n00\n')
  menu('09\n01\n'+temp+'/missing-manifest\n\n02\n'+temp+'/missing-binary\n'+'0'*64+'\n'+temp+'/prefix\n\n04\n'+temp+'/prefix\n\n05\n'+temp+'/prefix\n\n06\n\n00\n00\n') # Offline check fails; never updates.
  menu('11\n01\n\n02\n\n03\n\n00\n00\n')
  menu('12\nno\n\n00\n');menu('12\nyes\nno\n\n00\n') # No uninstall executor invoked.
 # Distinguish valid OAuth discovery from missing binding/deployment and corrupt state.
 (root/'auth.json').write_text(json.dumps({'tokens':{'account_id':'synthetic-panel','access_token':'synthetic-panel-placeholder'}}))
 (root/'auth.json').chmod(0o600)
 out=menu('01\n00\n00\n');assert 'OAuth' in out and 'state.json' in out
 # A saved lifecycle remains visible under 08, while 01 details uses full status JSON.
 (data/'state.json').write_text(json.dumps({'schema':1,'account':'synthetic-panel','last_clock':0,'attempts':[],'positive_until':1793300000,'pending':None,'observations':[],'weekly':None,'server_denied':True,'legacy_attempts':[]}));(data/'state.json').chmod(0o600)
 out=menu('08\n05\n\n00\n00\n');assert 'Window lifecycle (last saved state)' in out and '2026-' in out and 'Denied (automatic requests blocked)' in out
 out=menu('01\n02\n\n00\n00\n');assert '"prewarm_conditional": true' in out and f'"version": "{expected_version}"' in out and '"saved_state"' in out
 (data/'state.json').write_text('{invalid')
 out=menu('01\n00\n00\n');assert 'Invalid' in out or '无效' in out
 (data/'state.json').unlink()
 out=menu('10\n00\n');assert json.loads((data/'config.json').read_text())['language']=='zh-CN'
 out=menu('\n00\n',{'NO_COLOR':'1'});assert not re.search(r'\x1b\[[0-9;]*m',out);assert '\x1b[2J' in out and '\x1b[3J' not in out
 out=menu('00\n',{'TERM':'dumb'});assert '\x1b' not in out
 menu('\x04');menu('',interrupt=True)
 p=direct(ok=False);assert '\x1b' not in p.stdout+p.stderr
 v=json.loads(direct('--json','status').stdout);assert v['version']==expected_version and 'state_valid' in v
 v=json.loads(direct('--json','schedule','list').stdout);assert len(v['anchors'])==4
 v=json.loads(direct('--json','recover','status').stdout);assert v['classification']=='history_unknown' and not v['scheduler_state_ready']
 v=json.loads(direct('--json','doctor').stdout);assert not v['state_valid'] and v['existing_records_valid']
 for command in [('recover','begin'),('recover','begin','--acknowledge-unknown-history'),('recover','check'),('enable','--approve-production-cutover'),('activate','--account','bad','--approve-production-cutover')]:
  p=direct('--json',*command,ok=False);assert 'error' in json.loads(p.stdout)
 assert not (data/'deployment.json').exists() and not (data/'state.json').exists()
print(f'{checks} isolated bilingual menu/PTY sessions passed; JSON, EOF, SIGINT, NO_COLOR and terminal echo verified')
