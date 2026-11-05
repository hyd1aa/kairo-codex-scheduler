"""Scheduled-call evidence and cache presentation, no window-activation claims."""
import hashlib,json,os,pathlib,pty,subprocess,sys,time
from scheduled_support import Fixture
f=Fixture(sys.argv[1]);count=0
try:
 def menu():
  global count
  m,s=pty.openpty();p=subprocess.Popen([f.binary,'--data-dir',str(f.data)],stdin=s,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=f.env);os.close(s);os.write(m,b'0\n');out,err=p.communicate(timeout=10);os.close(m);assert p.returncode==0,(out,err);assert '\x1b' not in out;count+=1;return out
 now=int(time.time());state=f.read('schedule.json');state['watermark']=now
 auth=f.root/'empty-auth';auth.mkdir();(auth/'auth.json').write_text(json.dumps({'tokens':{'account_id':'synthetic-display','access_token':'synthetic-private-output'}}));(auth/'auth.json').chmod(0o600)
 f.write('quota-cache.json',{'account':hashlib.sha256(b'synthetic-display').hexdigest(),'at':now-600,'five_hour':{'used':0},'weekly':{'used':66}})
 for result,reason,zh,en in [('success','completed','✓ 成功','✓ Success'),('unknown','started','结果未知','Result unknown'),('failed','completion_evidence_missing','未返回有效完成证据','completion evidence missing'),('failed','weekly_limit','每周额度限制','weekly limit'),('failed','network_error','网络请求失败','Network request failed'),('skipped','call_in_progress','上一调用仍在运行','Previous call still running')]:
  state['calls']=[dict(slot='synthetic',due=now-30,started_at=now-30,completed_at=now,result=result,reason=reason,exit_code=0 if result=='success' else 7)];f.write('schedule.json',state)
  for language,label in [('zh-CN',zh),('en-US',en)]:
   f.call('lang',language);out=menu();assert label in out,(label,out)
   assert ('100.0%' in out and '34.0%' in out and ('旧缓存' if language=='zh-CN' else 'stale cache') in out)
   if result!='success':assert '✓ 成功' not in out and '✓ Success' not in out
   assert '新窗口已激活' not in out and 'new window activated' not in out
 # External login changes are seen by the official CLI status checker.
 f.settings(login=False);f.call('lang','zh-CN');assert '需要登录 Codex' in menu()
 f.settings(login=True);f.call('pause');assert '已暂停' in menu()
 # A closed output consumer remains a successful Unix termination.
 for args in [('--json','status'),('--help',),('--version',)]:
  r,w=os.pipe();os.close(r);p=subprocess.Popen([f.binary,'--data-dir',str(f.data),*args],env=f.env,stdout=w,stderr=subprocess.PIPE,text=True);os.close(w);_,err=p.communicate(timeout=10);assert p.returncode==0 and 'panic' not in err;count+=1
 print(f'{count} evidence/cache/login/closed-pipe regressions passed; no model or quota requests')
finally:f.close()
