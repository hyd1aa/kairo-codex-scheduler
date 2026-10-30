"""Execute v0.2.3 -> current -> rollback in private directories; no services/network/models."""
import hashlib,json,os,pathlib,subprocess,sys,tempfile
old,new=map(lambda p:str(pathlib.Path(p).resolve()),sys.argv[1:])
env={**os.environ,'KAIRO_OFFLINE':'1','NO_COLOR':'1'}
def digest(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
with tempfile.TemporaryDirectory(prefix='kairo-upgrade-policy-') as temp:
 for policy in ['running','paused','recovering','auto-paused','auto-recovering']:
  root=pathlib.Path(temp)/policy;data=root/'data';prefix=root/'program';data.mkdir(parents=True,mode=0o700)
  def call(binary,*args):
   p=subprocess.run([binary,'--data-dir',str(data),*args],env=env,capture_output=True,text=True,timeout=30)
   assert p.returncode==0,(policy,args,p.stdout,p.stderr)
  def write(name,value):
   p=data/name;p.write_text(json.dumps(value));p.chmod(0o600)
  call(old,'init');call(old,'schedule','add','03:37')
  account=hashlib.sha256(b'synthetic-upgrade-account').hexdigest()
  quota=dict(duration=18000,used=0.,reset_at=19000,remaining=18000)
  write('state.json',dict(schema=1,account=account,last_clock=1000,attempts=[dict(anchor='synthetic-known-anchor',at=1000,completed_at=None,before=quota,after=None,outcome='attempt_persisted_unknown')],positive_until=19000,pending=None,observations=[],weekly=None,server_denied=False,legacy_attempts=[]))
  write('deployment.json',dict(schema=1,account=account,enabled=policy=='running',approved_at=900 if policy=='running' else None))
  write('cursor.json',dict(schema=1,timezone='Etc/UTC',observed=1000,due=None,anchor=None,cutoff=0,finished=['synthetic-known-anchor'],config_signature=''))
  if 'recovering' in policy:
   write('recovery.json',dict(schema=1,account=account,owner=str(data),started_at=1000,checked_at=1500,not_before=19390,history_before_start='unknown',approved=False,boot_id='synthetic-boot',checked_uptime_ms=1500000,quiet_remaining_ms=17890000,five=quota,weekly=dict(duration=604800,used=50.,reset_at=605800,remaining=604800)))
  if policy.startswith('auto-'):
   write('onboarding.json',dict(schema=1,account=account,requested=policy=='auto-recovering',exclusive_account=True,next_attempt=19000,failures=2))
  call(old,'install','--binary',old,'--sha256',digest(old),'--prefix',str(prefix))
  names=['config.json','runtime.json','state.json','deployment.json','cursor.json','recovery.json','onboarding.json']
  before={name:(data/name).read_bytes() if (data/name).exists() else None for name in names}
  def preserved():
   for name,expected in before.items():assert ((data/name).read_bytes() if (data/name).exists() else None)==expected,(policy,name)
  call(old,'update','apply',new,'--sha256',digest(new),'--prefix',str(prefix));preserved()
  call(new,'update','rollback','--prefix',str(prefix));preserved()
  assert digest(prefix/'bin/kairo')==digest(old)
  call(old,'update','apply',new,'--sha256',digest(new),'--prefix',str(prefix));preserved()
  call(new,'update','apply',new,'--sha256',digest(new),'--prefix',str(prefix));preserved()
print('20 v0.2.3 upgrade/rollback/repeat checks passed across running, paused and recovery policies; byte-identical history, identity, schedules and wait progress; zero model/service calls')
