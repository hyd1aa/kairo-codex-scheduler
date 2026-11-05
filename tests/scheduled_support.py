"""Private fake official CLI; no credentials, network or production services."""
import datetime, json, os, pathlib, subprocess, tempfile
MOCK = r'''#!/usr/bin/env python3
# KAIRO_SYNTHETIC_CODEX: contains no network/model implementation.
import json, os, pathlib, sys, time
root=pathlib.Path(os.environ['HOME']); settings=json.loads((root/'mock-settings.json').read_text());args=sys.argv[1:]
if args==['login','status']:sys.exit(0 if settings.get('login') else 1)
if args==['exec','--help']:
 print('--ignore-user-config --ignore-rules --sandbox --ephemeral --json --skip-git-repo-check');sys.exit(0)
assert args[0]=='exec',args
assert '--ignore-user-config' in args and '--ignore-rules' in args and '--ephemeral' in args and '--json' in args
assert args[args.index('--sandbox')+1]=='read-only'
assert 'model_providers.openai.request_max_retries=0' in args and 'model_providers.openai.stream_max_retries=0' in args
assert 'features.shell_tool=false' in args and 'approval_policy="never"' in args
assert not list(pathlib.Path.cwd().iterdir()),'Workspace was not empty'
assert not os.environ.get('OPENAI_API_KEY') and not os.environ.get('OPENAI_BASE_URL')
with (root/'invocations.jsonl').open('a') as f:
 f.write(json.dumps({'args':args,'cwd':str(pathlib.Path.cwd()),'pid':os.getpid()})+'\n');f.flush();os.fsync(f.fileno())
mode=settings.get('mode','ok')
if mode=='sleep':time.sleep(60)
if mode in ('failed','network','weekly','expired'):
 message={'failed':'synthetic-private-output','network':'network connection failed','weekly':'weekly quota exhausted HTTP 429','expired':'401 unauthorized'}[mode]
 print(json.dumps({'type':'turn.failed','error':{'message':message}}));sys.exit(7)
if mode=='empty':sys.exit(0)
if mode=='malformed':print('synthetic non-json');sys.exit(0)
if mode=='tool':print(json.dumps({'type':'item.completed','item':{'type':'command_execution'}}))
print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':'Kairo ping'}}))
if mode!='incomplete':print(json.dumps({'type':'turn.completed','usage':{'input_tokens':12,'output_tokens':3}}))
'''
def stamp(s):return int(datetime.datetime.fromisoformat(s).timestamp())
BASE=stamp('2026-11-10T05:59:50+00:00')
DUE=stamp('2026-11-10T06:00:30+00:00')
SECOND=stamp('2026-11-10T11:02:30+00:00')
class Fixture:
 def __init__(self,binary,login=True):
  self.temp=tempfile.TemporaryDirectory(prefix='kairo-scheduled-');self.root=pathlib.Path(self.temp.name);self.data=self.root/'data';self.binary=str(pathlib.Path(binary).resolve())
  self.env={**os.environ,'KAIRO_OFFLINE':'1','NO_COLOR':'1','HOME':str(self.root),'CODEX_HOME':str(self.root/'empty-auth')}
  tools=self.root/'tools';tools.mkdir()
  for name,body in [('systemctl', 'printf "ActiveState=active\\nLoadState=loaded\\nMainPID=42\\n"'), ('journalctl', 'printf \'%s\\n\' \'{"MESSAGE":"synthetic log"}\''), ('timedatectl', 'printf "Asia/Shanghai\\n"')]:
   p=tools/name;p.write_text('#!/bin/sh\n'+body+'\n');p.chmod(0o700)
  self.env['PATH']=str(tools)+':'+self.env['PATH']
  self.mock=self.root/'mock-codex';self.mock.write_text(MOCK);self.mock.chmod(0o700);self.settings(login=login)
  self.call('init');self.call('runtime','--home',str(self.root),'--codex-home',str(self.root/'empty-auth'),'--codex',str(self.mock))
  c=self.read('config.json');c['anchors']=['06:00','11:02','16:04','21:06'];self.write('config.json',c)
 def settings(self,**kw):
  path=self.root/'mock-settings.json';v=json.loads(path.read_text()) if path.exists() else {};v.update(kw);path.write_text(json.dumps(v))
 def raw(self,*args,ok=True):
  p=subprocess.run([self.binary,'--data-dir',str(self.data),*args],env=self.env,text=True,capture_output=True,timeout=20)
  assert (p.returncode==0)==ok,(args,p.returncode,p.stdout,p.stderr)
  assert 'synthetic-private-output' not in p.stdout+p.stderr
  return p
 def call(self,*args,ok=True):return json.loads(self.raw('--json',*args,ok=ok).stdout)
 def read(self,name):return json.loads((self.data/name).read_text())
 def write(self,name,v):p=self.data/name;p.write_text(json.dumps(v));p.chmod(0o600)
 def timeline(self,ticks,start=BASE,initialize=True):
  p=self.root/'timeline.json';p.write_text(json.dumps({'start':start,'initialize':initialize,'ticks':[{'now':n,'timezone':'Etc/UTC'} for n in ticks]}));return p
 def replay(self,ticks,start=BASE,initialize=True):self.raw('daemon','--fixture',str(self.timeline(ticks,start,initialize)))
 def calls(self):return self.read('schedule.json')['calls']
 def invocations(self):
  p=self.root/'invocations.jsonl';return [json.loads(s) for s in p.read_text().splitlines()] if p.exists() else []
 def close(self):self.temp.cleanup()
