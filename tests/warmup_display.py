"""Read-only UI and EPIPE regressions using synthetic evidence in private temp data."""
import hashlib,json,os,pathlib,pty,subprocess,sys,tempfile,time,datetime,zoneinfo
binary=str(pathlib.Path(sys.argv[1]).resolve());count=0
with tempfile.TemporaryDirectory(prefix='kairo-evidence-') as temp:
 root=pathlib.Path(temp);data=root/'data';tools=root/'tools';tools.mkdir()
 (tools/'timedatectl').write_text('#!/bin/sh\nprintf "Asia/Shanghai\\n"\n');(tools/'timedatectl').chmod(0o700)
 env={**os.environ,'PATH':str(tools)+':'+os.environ['PATH'],'KAIRO_OFFLINE':'1','NO_COLOR':'1','RUST_BACKTRACE':'1'}
 def direct(*args,ok=True):
  p=subprocess.run([binary,'--data-dir',str(data),*args],env=env,text=True,capture_output=True,timeout=15)
  assert (p.returncode==0)==ok,(args,p.stdout,p.stderr);return p
 def write(name,v):p=data/name;p.write_text(json.dumps(v));p.chmod(0o600)
 def menu(text='00\n'):
  global count
  master,slave=pty.openpty();p=subprocess.Popen([binary,'--data-dir',str(data)],stdin=slave,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,text=True)
  os.close(slave);os.write(master,text.encode());out,err=p.communicate(timeout=20);os.close(master);assert p.returncode==0,(out,err)
  assert '\x1b' not in out and 'synthetic-display-placeholder' not in out+err;count+=1;return out
 direct('init');direct('runtime','--home',temp,'--codex-home',temp,'--codex','/nonexistent-synthetic-codex')
 account=hashlib.sha256(b'synthetic-display-account').hexdigest()
 auth=root/'auth.json';auth.write_text(json.dumps({'tokens':{'account_id':'synthetic-display-account','access_token':'synthetic-display-placeholder'}}));auth.chmod(0o600)
 done=int(time.time())-300
 shown_request=datetime.datetime.fromtimestamp(done,zoneinfo.ZoneInfo("Asia/Shanghai")).strftime("%H:%M")
 shown_confirmation=datetime.datetime.fromtimestamp(done+152,zoneinfo.ZoneInfo("Asia/Shanghai")).strftime("%H:%M")
 before=dict(duration=18000,used=0.,reset_at=done-2+18000,remaining=18000)
 attempt=dict(anchor='synthetic-06:00',at=done-2,completed_at=done,before=before,after=None,outcome='request_ok_window_unconfirmed')
 state=dict(schema=1,account=account,last_clock=done+152,attempts=[attempt],positive_until=0,pending=None,observations=[],weekly=None,server_denied=False,legacy_attempts=[])
 task=dict(anchor=attempt['anchor'],request_at=attempt['at'],completed_at=done,status='countdown_confirmed_time_correlated',request_success=True,countdown_confirmed=True,time_correlated=True,attribution_uncertain=True,next_check=None,deadline=done+18000,checks=2,samples=[dict(checked_at=done+d,reset_at=done+18000,remaining=18000-d,used_percent=0.) for d in [60,152]],events=[])
 write('state.json',state);write('confirmations.json',dict(schema=1,account=account,tasks=[task]))
 write('recovery.json',dict(approved=True,not_before=done-10000))
 write('quota-cache.json',dict(account=account,at=int(time.time())-600,five_hour=dict(used=0.,reset_at=done+18000),weekly=dict(used=40.,reset_at=done+604800)))
 out=menu();assert '五小时倒计时已确认（时间关联，归因不确定）' in out and shown_request in out and shown_confirmation in out
 assert '剩余100.0%' in out and '剩余60.0%' in out and '旧缓存' in out and '下次调度检查' in out
 assert '预计复查' not in out and '安全恢复复查' not in out and '2026-' not in out
 out=menu('01\n02\n\n03\n\n04\n00\n');assert '仅列出已保存记录' in out and 'request_ok_window_unconfirmed' not in out and 'countdown_confirmed_time_correlated' not in out
 # A newer failure never inherits the older confirmation.
 failed={**attempt,'anchor':'synthetic-failure','at':done+200,'completed_at':done+202,'before':{**before,'reset_at':done+200+18000},'outcome':'request_failed'}
 state['last_clock']=done+202;state['attempts'].append(failed);write('state.json',state)
 out=menu();assert '✗ 预热请求失败' in out and '预热结果: ✓' not in out
 out=menu('01\n03\n\n00\n00\n');assert '模型请求未成功' in out
 state['attempts']=[attempt];write('state.json',state);(data/'confirmations.json').unlink()
 out=menu();assert '请求成功，等待倒计时确认' in out and '预热结果: ✓' not in out
 state['attempts']=[];state['last_clock']=0;write('state.json',state);(data/'recovery.json').unlink()
 out=menu();assert '尚未进行过预热' in out
 direct('lang','en-US');out=menu();assert 'No warmup performed yet' in out and 'Next scheduled check' in out
 # Closed consumers: successful Unix termination, no panic/backtrace. JSON/business errors unchanged otherwise.
 for args in [('--json','status'),('--help',),('--version',)]:
  read,write_fd=os.pipe();os.close(read)
  p=subprocess.Popen([binary,'--data-dir',str(data),*args],env=env,stdout=write_fd,stderr=subprocess.PIPE,text=True);os.close(write_fd)
  _,err=p.communicate(timeout=15);assert p.returncode==0 and 'panic' not in err.lower() and 'backtrace' not in err.lower(),(args,p.returncode,err);count+=1
 for args,expected in [(('--json','schedule','add','bad'),1),(('--json','--invalid-kairo-option'),2)]:
  read,write_fd=os.pipe();os.close(read)
  p=subprocess.Popen([binary,'--data-dir',str(data),*args],env=env,stdout=write_fd,stderr=subprocess.PIPE,text=True);os.close(write_fd)
  _,err=p.communicate(timeout=15);assert p.returncode==expected and 'panic' not in err.lower(),(args,p.returncode,err);count+=1
 assert json.loads(direct('--json','status').stdout)['version']
 assert 'error' in json.loads(direct('--json','schedule','add','bad',ok=False).stdout)
print(f'{count} warmup-evidence/cache/localization/closed-pipe regressions passed; zero model/service calls')
