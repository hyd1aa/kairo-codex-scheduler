"""Real Rust scheduler + synthetic CLI: requested timed-call scenarios."""
import fcntl,json,os,pathlib,signal,subprocess,sys,time
from scheduled_support import Fixture,BASE,DUE,SECOND
binary=sys.argv[1];checks=0

def scenario(name,body):
 global checks
 f=Fixture(binary)
 try:body(f);checks+=1;print('PASS:',name)
 finally:f.close()
def success(f):
 f.replay([DUE,DUE,DUE+1]);assert len(f.invocations())==1 and f.calls()[-1]['result']=='success'
scenario('fresh install and OAuth-before-install: no deployment/state/guard prerequisites',success)
def late(f):
 f.settings(login=False);f.replay([DUE]);assert not f.invocations() and f.calls()[-1]['reason']=='login_required'
 f.settings(login=True);f.replay([SECOND],DUE,False);assert len(f.invocations())==1
scenario('install-before-login automatically uses later login without restart/enable',late)
def reinstall(f):
 for name in ['schedule.json','.schedule.json.previous','deployment.json','state.json','cursor.json','onboarding.json']:(f.data/name).unlink(missing_ok=True)
 success(f)
scenario('reinstall with no old history: future slot executes immediately',reinstall)
def window(f):
 f.write('quota-cache.json',{'five_hour':{'used':95,'reset_at':SECOND+18000},'weekly':{'used':100},'server_denied':True})
 f.write('deployment.json',{'enabled':False});f.write('state.json',{'positive_until':SECOND+18000});f.write('recovery.json',{'not_before':SECOND+18000});success(f)
scenario('active/exhausted windows, weekly cache, recovery and other-device history do not gate',window)
def expired_window(f):f.write('quota-cache.json',{'five_hour':{'used':0,'reset_at':DUE-1}});success(f)
scenario('expired window executes the same path',expired_window)
def weekly(f):
 f.settings(mode='weekly');f.replay([DUE,DUE+1]);assert len(f.invocations())==1 and f.calls()[-1]['reason']=='weekly_limit'
scenario('official weekly rejection recorded with no retry',weekly)
def two(f):
 f.replay([DUE,SECOND,SECOND]);assert len(f.invocations())==2
 f.replay([DUE,SECOND],BASE,False);assert len(f.invocations())==2
scenario('two consecutive slots, duplicate ticks and process restart',two)
def failure(f):
 for n,mode in enumerate(['failed','network','expired','empty','incomplete','tool','malformed']):
  f.settings(mode=mode);start=BASE+n*86400;due=DUE+n*86400
  f.replay([due,due+1],start,n==0);assert f.calls()[-1]['result']=='failed' and len(f.invocations())==n+1
scenario('CLI/network/OAuth failures and missing/invalid/tool completion evidence',failure)
def pause(f):
 f.call('pause');f.replay([DUE,SECOND]);assert not f.invocations() and f.read('schedule.json')['paused']
 f.call('migrate');assert f.read('schedule.json')['paused'];f.call('enable');f.replay([SECOND+86400],SECOND,False);assert len(f.invocations())==1
scenario('explicit pause persists across restart/migration until explicit start',pause)
def overlap(f):
 with (f.data/'call.lock').open('w') as lock:
  fcntl.flock(lock,fcntl.LOCK_EX);f.replay([DUE]);assert not f.invocations() and f.calls()[-1]['reason']=='call_in_progress'
 f.replay([DUE+1],DUE,False);assert not f.invocations()
scenario('overlapping call lock skips and consumes the slot',overlap)
def disk(f):
 os.symlink('/dev/null',f.data/'.schedule.json.tmp');f.raw('daemon','--fixture',str(f.timeline([DUE])),ok=False);assert not f.invocations()
scenario('pre-request durable write failure cannot launch Codex',disk)
def missing(f):
 f.call('runtime','--codex','/nonexistent-kairo-synthetic');f.replay([DUE,DUE+1]);assert not f.invocations()
scenario('missing/unusable official CLI records failure without retries',missing)
def skipped(f):
 f.replay([DUE+500,SECOND]);assert len(f.invocations())==1 and f.calls()[0]['reason']=='missed_not_replayed'
scenario('missed slots are not replayed in a burst',skipped)
def boot(f):
 f.replay([DUE]);before=(f.data/'config.json').read_bytes();f.raw('daemon','--once');f.replay([DUE],BASE,False)
 assert len(f.invocations())==1 and (f.data/'config.json').read_bytes()==before
scenario('daemon restart/boot baseline and custom settings retain deduplication',boot)
def legacy_pause(f):
 (f.data/'schedule.json').unlink();f.write('onboarding.json',{'requested':False});f.call('migrate');assert f.read('schedule.json')['paused']
scenario('old explicit pause migrates independently of missing deployment',legacy_pause)
def corrupted_old(f):
 (f.data/'state.json').write_text('{bad old history');success(f);assert (f.data/'state.json').read_text()=='{bad old history'
scenario('unreadable legacy history is retained and cannot block timed calls',corrupted_old)
def clock(f):
 f.replay([DUE,DUE-3600,DUE,DUE+1]);assert len(f.invocations())==1
scenario('clock rollback cannot replay an executed slot',clock)
def buffer_edit(f):
 f.replay([DUE]);f.call('prewarm','--buffer','60');f.replay([DUE+30],DUE,False);assert len(f.invocations())==1
scenario('editing delay cannot execute the same wall-clock slot twice',buffer_edit)

def orphan(f):
 f.settings(mode='sleep');p=subprocess.Popen([f.binary,'--data-dir',str(f.data),'daemon','--fixture',str(f.timeline([DUE]))],env=f.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
 for _ in range(200):
  if f.invocations():break
  time.sleep(.02)
 assert len(f.invocations())==1
 f.raw('daemon','--once',ok=False) # single daemon lease
 p.kill();p.wait(timeout=5);pid=f.invocations()[0]['pid']
 for _ in range(200):
  stat=pathlib.Path(f'/proc/{pid}/stat')
  if not stat.exists() or stat.read_text().split()[2]=='Z':break
  time.sleep(.02)
 else:raise AssertionError('orphan Codex process survived parent death')
 f.settings(mode='ok');f.replay([DUE,DUE+1],BASE,False);assert len(f.invocations())==1
scenario('SSH-independent daemon, crash cleanup, lease exclusion and pre-spawn reservation',orphan)
def responsive_pause(f):
 f.settings(mode='sleep');p=subprocess.Popen([f.binary,'--data-dir',str(f.data),'daemon','--fixture',str(f.timeline([DUE]))],env=f.env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
 try:
  for _ in range(200):
   if f.invocations():break
   time.sleep(.02)
  assert len(f.invocations())==1
  before=time.monotonic();f.call('pause');assert time.monotonic()-before<3
  assert f.call('status')['paused']
 finally:p.terminate();p.wait(timeout=5)
scenario('pause and status stay responsive while a Codex invocation runs',responsive_pause)

def optional(f):
 f.call('quota',ok=False);success(f);assert f.call('status')['quota_optional']
scenario('quota query failure is independent from scheduling',optional)
def compat(f):
 f.call('recover','check');assert f.call('status')['enabled'];assert f.call('recover','status')['wait_seconds']==0
 f.call('auth','bind');f.call('activate','--account','legacy');assert f.call('status')['enabled']
scenario('old commands remain harmless aliases with no multi-stage approval',compat)
def transaction(f):
 import hashlib
 f.replay([DUE]);before=(f.data/'schedule.json').read_bytes();prefix=f.root/'program';sha=hashlib.sha256(pathlib.Path(f.binary).read_bytes()).hexdigest()
 f.call('install','--binary',f.binary,'--sha256',sha,'--prefix',str(prefix));f.call('update','apply',f.binary,'--sha256',sha,'--prefix',str(prefix));f.call('update','rollback','--prefix',str(prefix))
 assert (f.data/'schedule.json').read_bytes()==before
 f.replay([DUE,DUE+1],BASE,False);assert len(f.invocations())==1
scenario('new completed slot survives binary update and rollback byte-identically',transaction)
def disconnect(f):
 p=subprocess.Popen([f.binary,'--data-dir',str(f.data),'daemon','--fixture',str(f.timeline([BASE,DUE])),'--poll-ms','1000','--hold'],env=f.env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True)
 try:
  time.sleep(.2);p.send_signal(signal.SIGHUP)
  for _ in range(200):
   if f.invocations():break
   time.sleep(.02)
  assert len(f.invocations())==1 and p.poll() is None
 finally:p.terminate();p.wait(timeout=5)
scenario('SSH-style SIGHUP does not stop scheduled daemon calls',disconnect)
def restore_runtime(f):
 original=f.read('runtime.json');original['home']=str(f.root/'unmounted-old-home');original['codex_home']=str(f.root/'custom-login');f.write('.runtime.json.previous',original);(f.data/'runtime.json').unlink()
 env={k:v for k,v in f.env.items() if k!='HOME'}
 p=subprocess.run([f.binary,'--data-dir',str(f.data),'migrate'],env=env,text=True,capture_output=True,timeout=10)
 assert p.returncode==0,(p.stdout,p.stderr)
 assert f.read('runtime.json')==original and f.read('.runtime.json.previous')==original
scenario('valid runtime backup survives absent HOME and temporarily unavailable paths',restore_runtime)
print(f'{checks} scheduled-call scenarios passed; mock CLI only, zero real model requests')
