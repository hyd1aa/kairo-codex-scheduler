#!/usr/bin/env python3
"""Offline executable acceptance. No real OAuth, Codex, HTTP or service control."""
import hashlib,json,os,pathlib,pty,re,select,subprocess,sys,tempfile,time,signal,shutil,fcntl,termios,struct
from datetime import datetime
from zoneinfo import ZoneInfo
running=[]
import atexit
def cleanup():
    for process in running:
        if process.poll() is None:
            process.terminate()
            try:process.wait(timeout=3)
            except subprocess.TimeoutExpired:process.kill();process.wait()
atexit.register(cleanup)
binary=str(pathlib.Path(sys.argv[1]).resolve());repo=pathlib.Path(__file__).resolve().parents[1];checks=0
with tempfile.TemporaryDirectory(prefix='kairo-v02-') as temp:
    base=pathlib.Path(temp);stable=base/'kairo-under-test';shutil.copy2(binary,stable);binary=str(stable);root=base/'state';tools=base/'tools';tools.mkdir();fakeauth=base/'auth';fakeauth.mkdir()
    (fakeauth/'auth.json').write_text(json.dumps({'tokens':{'account_id':'synthetic-account','access_token':'synthetic-not-a-real-token'}}))
    (fakeauth/'auth.json').chmod(0o600)
    def tool(name,body):
        p=tools/name;p.write_text('#!/bin/sh\n'+body+'\n');p.chmod(0o700);return p
    codex=tool('codex', '''if [ "$1" = exec ] && [ "$2" = --help ]; then echo '--ignore-user-config --sandbox --ephemeral --skip-git-repo-check'; exit 0; fi
if [ "$1" = login ]; then exit 0; fi
exit 99''')
    service_log=base/'service-calls'
    tool('systemctl',f'printf "%s\\n" "$*" >> "{service_log}"\necho ActiveState=inactive\nexit 0')
    tool('journalctl', '''echo '{"MESSAGE":"synthetic log"}' ''')
    tool('curl', '''while [ "$#" -gt 0 ]; do if [ "$1" = --output ]; then shift; cp "$KAIRO_MOCK_BINARY" "$1"; exit 0; fi; shift; done; exit 1''')
    env=dict(os.environ,PATH=str(tools)+':'+os.environ['PATH'],CODEX_HOME=str(fakeauth),KAIRO_MOCK_BINARY=binary,NO_COLOR='1',KAIRO_OFFLINE='1')
    def raw(args,where=root,ok=True,timeout=30,custom=None):
        global checks
        p=subprocess.run([binary,'--data-dir',str(where),*args],env=custom or env,capture_output=True,text=True,timeout=timeout)
        assert (p.returncode==0)==ok,(args,p.returncode,p.stdout,p.stderr)
        assert 'synthetic-not-a-real-token' not in p.stdout+p.stderr
        checks+=1;return p
    def call(*args,where=root,ok=True):
        p=raw(['--json',*args],where,ok);assert '\x1b' not in p.stdout
        return json.loads(p.stdout)
    call('init');assert len(call('schedule','list')['anchors'])==4
    call('schedule','add','02:00');call('schedule','edit','11:00','12:30');call('schedule','remove','21:00')
    before=(root/'config.json').read_bytes()
    for t in ['02:00','25:00','2:00','02:05',' 1:00']:
        call('schedule','add',t,ok=False);assert (root/'config.json').read_bytes()==before
    processes=[subprocess.Popen([binary,'--data-dir',str(root),'schedule','add',t],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE) for t in ['03:00','04:00']]
    for p in processes:assert p.wait(timeout=10)==0
    assert len(call('schedule','list')['anchors'])==6
    call('schedule','next');call('schedule','preview');call('schedule','reset')
    call('lang','en-US');assert call('status')['config']['language']=='en-US'
    for lang in ['en-US','zh-CN']:
        for args in [('schedule','add','25:00'),('schedule','remove','19:00'),('auth','verify'),('activate','--account','bad')]:
            message=call('--lang',lang,*args,ok=False)['error']['message']
            assert bool(re.search('[\u4e00-\u9fff]',message))==(lang=='zh-CN'),message
        helptext=raw(['--lang',lang,'--help']).stdout
        assert ('用法:' in helptext)==(lang=='zh-CN')
        if lang=='zh-CN':assert all(x not in helptext for x in ['Commands:','Options:','Arguments:','Print help','[default:'])
        if lang=='en-US':assert not re.search('[\u4e00-\u9fff]',helptext),helptext
        nested=raw(['--lang',lang,'schedule','--help']).stdout
        assert ('激活' in nested)==(lang=='zh-CN')
    call('lang','zh-CN')
    for b in ['-1','301']:call('prewarm','--buffer='+b,ok=False)
    call('prewarm','--idle','false','--buffer','60');call('prewarm','--reset')
    call('runtime','--home',str(base),'--codex-home',str(fakeauth),'--codex',str(codex))
    assert call('doctor')['codex_safety_flags'];call('doctor','--repair')
    call('auth','status');call('auth','add','sample','--home',str(fakeauth));call('auth','select','sample');call('auth','remove','sample',ok=False)
    call('auth','add','spare','--home',str(fakeauth));call('auth','remove','spare');assert 'sample' in call('auth','list')['profiles']
    for action in ['status','start','stop','restart','enable','disable']:assert call('service',action)['success']
    calls=service_log.read_text();assert all('kairo.service' in line for line in calls.splitlines())
    call('service','stop codex-window-scheduler.service',ok=False);call('logs');call('logs','--lines','10001',ok=False)
    call('auth','login',ok=False);call('help-info');call('validate');call('migrate')
    # Actual numbered menu, submenu, persistent language toggle and EOF handling.
    master,slave=pty.openpty();menuenv=dict(env);menuenv.pop('NO_COLOR',None)
    p=subprocess.Popen([binary,'--data-dir',str(root)],stdin=slave,stdout=slave,stderr=slave,env=menuenv);os.close(slave)
    os.write(master,b'02\n1\n\n00\n06\n06\n01\n00\n00\n');out=b'';deadline=time.monotonic()+15
    while time.monotonic()<deadline:
        if select.select([master],[],[],.1)[0]:
            try:out+=os.read(master,65536)
            except OSError:break
        if p.poll() is not None:break
    assert p.wait(timeout=5)==0;os.close(master);text=out.decode(errors='replace')
    assert '查看额度和预热记录' in text and 'Quota and warmup history' in text and '\x1b[34m' in text
    checks+=1
    for language,label in [('zh-CN','查看额度和预热记录'),('en-US','Quota and warmup history')]:
        call('lang',language)
        master,slave=pty.openpty()
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',24,40,0,0))
        p=subprocess.Popen([binary,'--data-dir',str(root)],stdin=slave,stdout=slave,stderr=slave,env=menuenv);os.close(slave)
        os.write(master,b'01\n00\n00\n');out=b'';deadline=time.monotonic()+15
        while time.monotonic()<deadline:
            if select.select([master],[],[],.1)[0]:
                try:out+=os.read(master,65536)
                except OSError:break
            if p.poll() is not None:break
        assert p.wait(timeout=5)==0;os.close(master)
        assert label in out.decode(errors='replace'),(language,out.decode(errors='replace'))
        checks+=1
    call('lang','zh-CN')
    cfg=json.loads((root/'config.json').read_text());cfg['schema']=99;(root/'config.json').write_text(json.dumps(cfg));call('schedule','add','09:00',ok=False);assert json.loads((root/'config.json').read_text())['schema']==99;cfg['schema']=1;(root/'config.json').write_text(json.dumps(cfg))
    # Python 1.3 wire schema conversion is independent and never reads production files.
    old=base/'legacy.json';old.write_text(json.dumps(dict(v=1,account='a'*16,attempts={'2026-10-19T11:00':dict(at=1792387044,outcome='request_ok_window_unconfirmed',cli_success=True)},processed={'2026-10-19T06:00':True},pending=dict(due=1792387050))))
    converted=call('migrate','--legacy',str(old),'--legacy-account','a'*16,'--legacy-timezone','Asia/Shanghai','--account','b'*64,'--dry-run')
    assert len(converted['state']['legacy_attempts'])==2 and converted['state']['pending'] is None and not (root/'state.json').exists()
    # Full offline daemon, identical engine and shared durable account gate.
    n=1792387044
    def usage(now,used=0,weekly=49,denied=False,active_reset=None):
        def window(d,u):return dict(limit_window_seconds=d,used_percent=u,reset_at=now+d,reset_after_seconds=d)
        f=window(18000,used)
        if active_reset is not None:f.update(reset_at=active_reset,reset_after_seconds=max(0,active_reset-now))
        return dict(rate_limit=dict(allowed=not denied,limit_reached=denied,primary_window=f,secondary_window=window(604800,weekly)))
    preview=base/'preview.json';preview.write_text(json.dumps(dict(now=n,cutoff=n+10000,anchor='synthetic',state=dict(schema=1,account='synthetic',last_clock=0,attempts=[],positive_until=0,pending=None),usage=usage(n))))
    simulation=call('simulate',str(preview));assert simulation['model_requests']==0 and simulation['decision']['eligible'] and simulation['state']['pending'] is None
    def fixture(name,ticks,account=None,due=None):
        where=base/name;call('init',where=where);account=account or hashlib.sha256(name.encode()).hexdigest();now=ticks[0]['now']
        state=dict(schema=1,account=account,last_clock=0,attempts=[],positive_until=0,pending=None)
        cursor=dict(schema=1,timezone=ticks[0]['timezone'],observed=now,due=due if due is not None else now,anchor='2026-10-19/11:00',cutoff=1792425600,finished=[],config_signature='')
        path=base/(name+'.json');path.write_text(json.dumps(dict(state=state,cursor=cursor,ticks=ticks)));return where,path
    def tick(now,**options):return dict(now=now,timezone=options.pop('timezone','Asia/Shanghai'),usage=options.pop('usage',usage(now)),**options)
    where,f=fixture('timeline',[tick(n-8214,usage=usage(n-8214,used=2,active_reset=n-30)),tick(n,completed_at=n+5,after=usage(n+5)),tick(n+5)])
    p=raw(['daemon','--fixture',str(f)],where);events=[json.loads(l) for l in p.stdout.splitlines()]
    assert events[0]['event']=='deferred' and events[1]['simulated_calls']==1 and events[-1]['simulated_calls']==1
    state=json.loads((where/'state.json').read_text());assert state['attempts'][0]['at']==n and state['attempts'][0]['completed_at']==n+5 and state['attempts'][0]['outcome']=='request_ok_window_unconfirmed' and state['pending'] is None
    acceptance=call('acceptance',where=where)
    assert acceptance['pending_confirmation'] is True and acceptance['request_success'] is True and acceptance['countdown_confirmed'] is False
    assert acceptance['confirmation_details']['check_count']==0 and acceptance['confirmation_details']['samples']==[]
    status=call('status',where=where)
    acceptance_timezone=status['timezone']
    expected_request_at=datetime.fromtimestamp(n,ZoneInfo(acceptance_timezone)).isoformat()
    assert acceptance['recent_request_at']==expected_request_at and (where/'confirmations.json').exists()
    assert status['acceptance']['pending_confirmation'] is True
    assert 'confirmation' in raw(['--lang','en-US','acceptance'],where).stdout.lower()
    call('validate',where=where)
    confirmation_before=(where/'confirmations.json').read_bytes()
    confirm_prefix=base/'confirm-install'
    call('install','--binary',binary,'--sha256',hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest(),'--prefix',str(confirm_prefix),where=where)
    assert (where/'confirmations.json').read_bytes()==confirmation_before
    call('update','apply',binary,'--sha256',hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest(),'--prefix',str(confirm_prefix),where=where)
    call('update','rollback','--prefix',str(confirm_prefix),where=where)
    assert (where/'confirmations.json').read_bytes()==confirmation_before
    assert all(e['model_requests']==0 for e in events)
    dense_time=n-8214+3600
    dense,df=fixture('dense',[tick(n-8214,usage=usage(n-8214,used=2,active_reset=n-30)),tick(dense_time-1,usage=usage(dense_time-1,used=2,active_reset=n-30)),tick(dense_time,usage=usage(dense_time,used=2,active_reset=n-30)),tick(n,completed_at=n+5,after=usage(n+5)),tick(n+5)])
    call('schedule','add','12:00',where=dense)
    events=[json.loads(l) for l in raw(['daemon','--fixture',str(df)],dense).stdout.splitlines()]
    assert events[2]['event']=='deferred' and events[-1]['simulated_calls']==1
    assert '2026-10-19/12:00' in json.loads((dense/'cursor.json').read_text())['finished']
    for name,u in [('weekly',usage(n,weekly=100)),('denial',usage(n,denied=True))]:
        where,f=fixture(name,[tick(n,usage=u)]);p=raw(['daemon','--fixture',str(f)],where);assert json.loads(p.stdout)['simulated_calls']==0
    shared=hashlib.sha256(b'shared').hexdigest()
    a,af=fixture('cross-a',[tick(n)],account=shared);raw(['daemon','--fixture',str(af)],a)
    b,bf=fixture('cross-b',[tick(n+1)],account=shared);raw(['daemon','--fixture',str(bf)],b);assert len(json.loads((b/'state.json').read_text())['attempts'])==0 # shared gate defers before consuming a local attempt
    # Crash after both reservations: restart never replays the attempt.
    where,f=fixture('crash',[tick(n,crash_after_reserve=True)]);raw(['daemon','--fixture',str(f)],where,ok=False)
    state=json.loads((where/'state.json').read_text());assert state['attempts'][0]['outcome']=='attempt_persisted_unknown'
    v=json.loads(f.read_text());v['ticks']=[tick(n+5)];f.write_text(json.dumps(v));p=raw(['daemon','--fixture',str(f)],where);assert json.loads(p.stdout)['simulated_calls']==0
    (where/'state.json').unlink();raw(['daemon','--fixture',str(f)],where,ok=False);assert not (where/'state.json').exists()
    # Running daemon permits config changes, replans timezone, denies a second daemon.
    where,f=fixture('hot',[tick(n),tick(n+1),tick(n+2,timezone='Asia/Tokyo')],due=n+100)
    p=subprocess.Popen([binary,'--data-dir',str(where),'daemon','--fixture',str(f),'--poll-ms','500','--hold'],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    running.append(p)
    assert json.loads(p.stdout.readline())['event']=='waiting'
    call('schedule','edit','11:00','12:30',where=where)
    assert json.loads(p.stdout.readline())['event']=='config_replanned'
    assert json.loads(p.stdout.readline())['event']=='timezone_replanned'
    raw(['daemon','--fixture',str(f)],where,ok=False)
    p.send_signal(signal.SIGTERM);assert p.wait(timeout=5)==0
    # Adding a future anchor while an earlier anchor waits must preserve that reset.
    keep,kf=fixture('hot-keep',[tick(n-8214,usage=usage(n-8214,used=2,active_reset=n-30)),tick(n-8213)])
    p=subprocess.Popen([binary,'--data-dir',str(keep),'daemon','--fixture',str(kf),'--poll-ms','500','--hold'],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True);running.append(p)
    assert json.loads(p.stdout.readline())['event']=='deferred'
    call('schedule','add','14:00',where=keep)
    assert json.loads(p.stdout.readline())['event']=='config_replanned'
    assert json.loads((keep/'cursor.json').read_text())['due']==n and json.loads((keep/'state.json').read_text())['pending']==n
    p.send_signal(signal.SIGTERM);assert p.wait(timeout=5)==0
    # Transactional installation and failure recovery.
    prefix=base/'install';data=prefix/'var/lib/kairo';checksum=hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest()
    def shell(script,*args,ok=True):
        global checks
        p=subprocess.run([str(repo/'packaging'/script),*map(str,args)],env=env,capture_output=True,text=True,timeout=45)
        assert (p.returncode==0)==ok,(script,p.stdout,p.stderr);checks+=1;return p
    nohome=base/'install-no-home';nohome_env=dict(env);nohome_env.pop('HOME',None);nohome_env.pop('CODEX_HOME',None)
    p=subprocess.run([str(repo/'packaging/install.sh'),binary,checksum,str(nohome)],env=nohome_env,capture_output=True,text=True,timeout=45)
    assert p.returncode==0,(p.stdout,p.stderr)
    runtime=json.loads((nohome/'var/lib/kairo/runtime.json').read_text())
    assert runtime['home']=='/' and runtime['codex_home']=='/.codex',runtime
    checks+=1
    shell('install.sh',binary,checksum,prefix);assert (prefix/'bin/kairo').exists()
    installed=subprocess.run([str(prefix/'bin/kairo'),'--json','status'],env=env,capture_output=True,text=True,timeout=15);assert installed.returncode==0 and json.loads(installed.stdout)['config']['anchors']
    shell('install.sh',binary,checksum,prefix);assert (prefix/'.upgrade-snapshot/binary').exists()
    current=(prefix/'bin/kairo').read_bytes();config=(data/'config.json').read_bytes()
    shell('install.sh',binary,'0'*64,prefix,ok=False);assert (prefix/'bin/kairo').read_bytes()==current
    bad=tool('bad-candidate','''[ "$1" = --version ] && exit 0
printf '{"schema":99}' > "$2/config.json"
exit 9''');badsha=hashlib.sha256(bad.read_bytes()).hexdigest()
    raw(['update','apply',str(bad),'--sha256',badsha,'--prefix',str(prefix)],data,ok=False)
    assert (data/'config.json').read_bytes()==config and (prefix/'bin/kairo').read_bytes()==current
    assert not (prefix/'.upgrade-pending').exists() and not (data/'.upgrade-lock').exists()
    # Reinstall/upgrade must not migrate an old data directory before its snapshot.
    olddata=base/'old-data';oldprefix=base/'old-prefix';call('init',where=olddata);(olddata/'runtime.json').unlink();(oldprefix/'bin').mkdir(parents=True);shutil.copy2(binary,oldprefix/'bin/kairo')
    call('migrate','--dry-run',where=olddata);assert not (olddata/'runtime.json').exists()
    call('install','--binary',str(bad),'--sha256',badsha,'--prefix',str(oldprefix),where=olddata,ok=False)
    assert not (olddata/'runtime.json').exists() and json.loads((olddata/'config.json').read_text())['schema']==1

    crash=tool('crash-candidate','''[ "$1" = --version ] && exit 0
printf '{"schema":99}' > "$2/config.json"
kill -KILL "$PPID"
exit 86''');crashsha=hashlib.sha256(crash.read_bytes()).hexdigest()
    raw(['update','apply',str(crash),'--sha256',crashsha,'--prefix',str(prefix)],data,ok=False)
    assert (prefix/'.upgrade-pending').exists();call('status',where=data,ok=False)
    call('update','recover','--prefix',str(prefix),where=data);assert (data/'config.json').read_bytes()==config and not (data/'.upgrade-lock').exists()
    shell('install.sh',binary,checksum,prefix)
    state=dict(schema=1,account='synthetic',last_clock=n,attempts=[],positive_until=n,pending=None);(data/'state.json').write_text(json.dumps(state));(data/'state.json').chmod(0o600)
    shell('rollback.sh',prefix);assert json.loads((data/'state.json').read_text())['positive_until']==n
    shell('uninstall.sh',prefix);assert not (prefix/'bin/kairo').exists() and (data/'state.json').exists()
    # Explicit repair restores configuration but does not erase attempt history.
    (data/'config.json').write_text('{"schema":99}')
    call('doctor','--repair',where=data)
    assert json.loads((data/'state.json').read_text())['positive_until']==n
    shell('bootstrap.sh','example/kairo','0.2.1',checksum,prefix)
    stage=base/'system-stage';shell('system-install.sh',binary,checksum,'root',stage);assert (stage/'etc/systemd/system/kairo.service').exists()
    # Re-provisioning an existing live unit must not reintroduce observation mode.
    mode=stage/'etc/systemd/system/kairo.service.d/20-mode.conf';mode.unlink()
    shell('system-install.sh',binary,checksum,'root',stage);assert not mode.exists()
    versioned=stage/'opt/kairo';vd=stage/'var/lib/kairo'
    old_target=os.readlink(versioned/'current');assert old_target.startswith('releases/'+raw(['--version']).stdout.strip().split()[-1]+'-')
    assert (versioned/'manager/kairo').exists()
    valid=tool('versioned-candidate', '[ "$1" = --version ] && { echo "kairo 9.9.9"; exit 0; }\nexec '+str(binary)+' "$@"')
    raw(['update','apply',str(valid),'--sha256',hashlib.sha256(valid.read_bytes()).hexdigest(),'--prefix',str(versioned)],vd)
    assert os.readlink(versioned/'current') != old_target and (versioned/old_target/'bin/kairo').exists()
    raw(['update','rollback','--prefix',str(versioned)],vd)
    assert os.readlink(versioned/'current') == old_target
    raw(['update','apply',str(bad),'--sha256',badsha,'--prefix',str(versioned)],vd,ok=False)
    assert os.readlink(versioned/'current') == old_target and json.loads((vd/'config.json').read_text())['schema']==1
    postbad=tool('post-switch-invalid', '[ "$1" = --version ] && { echo "kairo 9.9.8"; exit 0; }\n[ "$3" = validate ] && exit 7\nexec '+str(binary)+' "$@"')
    raw(['update','apply',str(postbad),'--sha256',hashlib.sha256(postbad.read_bytes()).hexdigest(),'--prefix',str(versioned)],vd,ok=False)
    assert os.readlink(versioned/'current') == old_target and not (versioned/'.upgrade-pending').exists()
    shell('system-uninstall.sh',stage);assert not (stage/'etc/systemd/system/kairo.service').exists() and (stage/'var/lib/kairo/config.json').exists()
    assert all('codex-window-scheduler.service' not in l for l in service_log.read_text().splitlines())
    # A hung timedatectl (including a child) must fall back to local timezone files.
    timezone_child=base/'timezone-child.pid'
    tool('timedatectl',f'sleep 60 &\nchild=$!\nprintf "%s\\n" "$child" > "{timezone_child}"\nwait "$child"')
    started=time.monotonic()
    fallback=json.loads(raw(['--json','doctor'],timeout=8).stdout)
    assert fallback['timezone']!='timezone_unknown' and time.monotonic()-started<7
    child=pathlib.Path('/proc')/timezone_child.read_text().strip()/'stat'
    assert not child.exists() or child.read_text().split()[2]=='Z', 'Timezone helper child survived timeout'
print(f'{checks} CLI/process/transaction checks passed; mock-only Codex/OAuth/systemd, zero real model requests; system timezone={acceptance_timezone}')
