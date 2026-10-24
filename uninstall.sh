#!/usr/bin/env bash
[ -n "${BASH_VERSION:-}" ] || exec bash "$0" "$@"
# Entire program is parsed before execution, including when streamed on stdin.
kairo_uninstall_main() (
set -euo pipefail
lang=${KAIRO_LANG:-zh-CN}
[[ $(id -u) == 0 ]] || { echo '请使用 sudo / Run with sudo' >&2; exit 1; }
purge=0; confirmation=
while (($#)); do
 case $1 in
  --purge-data) purge=1; shift;;
  --confirm) (($#>=2)) || exit 2; confirmation=$2; shift 2;;
  --help) echo 'uninstall.sh [--purge-data --confirm DELETE-KAIRO-DATA]'; exit 0;;
  *) echo "Unknown option: $1" >&2; exit 2;;
 esac
done
if ((purge)) && [[ $confirmation != DELETE-KAIRO-DATA ]]; then
 echo '拒绝清除：需要 --purge-data --confirm DELETE-KAIRO-DATA；将永久删除指定账户凭据和请求历史 / Purge requires explicit confirmation; it deletes credentials and request history.' >&2
 exit 2
fi
uninstall_exit() {
 rc=$?
 trap - EXIT
 if ((rc)); then
  echo '卸载未完成；检查以下仍存在的安装路径 / Uninstall incomplete; remaining installation paths:' >&2
  for path in /opt/kairo /etc/systemd/system/kairo.service /etc/systemd/system/kairo.service.d /usr/local/bin/kairo; do
   if [[ -e $path || -L $path ]]; then printf '%s\n' "$path" >&2; fi
  done
 fi
 exit "$rc"
}
trap uninstall_exit EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
if [[ ! -e /opt/kairo && ! -L /opt/kairo && ! -e /etc/systemd/system/kairo.service && ! -L /usr/local/bin/kairo && ! -e /var/lib/kairo-installer/managed-user && $purge == 0 ]]; then
 echo 'Kairo 未安装或已卸载 / Kairo is not installed or already removed.'
 echo '保留 / Preserved: /var/lib/kairo /var/lib/kairo-account-guards /home/kairo/.codex (if present); Codex CLI unchanged.'
 exit 0
fi
command -v python3 >/dev/null || { echo '缺少 python3；未执行清理。残留 / Missing python3; no cleanup performed. Remaining: /opt/kairo /etc/systemd/system/kairo.service /usr/local/bin/kairo' >&2; exit 1; }
python3 - "$purge" "$lang" <<'PY'
import fcntl,os,pathlib,pwd,grp,stat,subprocess,sys,time,shutil,re
purge=sys.argv[1]=='1';prefix=pathlib.Path('/opt/kairo');marker=pathlib.Path('/var/lib/kairo-installer/managed-user');unit=pathlib.Path('/etc/systemd/system/kairo.service');link=pathlib.Path('/usr/local/bin/kairo');drop=unit.parent/'kairo.service.d'
had_install=any(p.exists() or p.is_symlink() for p in [prefix,unit,link,drop])
retained=['/var/lib/kairo','/var/lib/kairo-account-guards','/home/kairo/.codex','/var/lib/kairo-installer/managed-user']
def exists(p):return p.exists() or p.is_symlink()
def safe_directory(p):
 s=p.lstat()
 if not stat.S_ISDIR(s.st_mode) or s.st_uid!=0 or s.st_mode&0o022:raise RuntimeError('Unsafe installer-owned directory: '+str(p))
def system(*args,check=True):
 p=subprocess.run(['systemctl',*args],capture_output=True,text=True,timeout=75)
 if check and p.returncode:raise RuntimeError('systemctl '+' '.join(args)+': '+p.stderr.strip())
 return p
def show(prop):
 p=system('show','kairo.service','--property='+prop,'--value',check=False)
 if p.returncode and not (prop=='LoadState' and p.stdout.strip()=='not-found'):raise RuntimeError('Cannot query kairo.service: '+p.stderr.strip())
 return p.stdout.strip()
def ancestors():
 result=set();pid=os.getpid()
 while pid and pid not in result:
  result.add(pid)
  try:pid=int(next(s.split()[1] for s in pathlib.Path(f'/proc/{pid}/status').read_text().splitlines() if s.startswith('PPid:')))
  except (OSError,StopIteration):break
 return result
def running():
 result=[];allowed=ancestors()
 for p in pathlib.Path('/proc').iterdir():
  if not p.name.isdigit() or int(p.name) in allowed:continue
  try:
   exe=os.readlink(p/'exe').removesuffix(' (deleted)')
   if exe.startswith('/opt/kairo/'):result.append(int(p.name))
  except OSError:pass
 return result
def residuals():
 return [str(p) for p in [prefix,unit,link,drop] if exists(p)]
locks=[]
try:
 if not marker.is_file() or marker.is_symlink() or marker.stat().st_uid!=0 or marker.read_text()!='schema=1\n':raise RuntimeError('缺少可信安装标记；拒绝删除 / Missing trusted managed-user marker; refusing removal')
 safe_directory(marker.parent)
 f=os.open(marker.parent/'online.lock',os.O_CREAT|os.O_RDWR|os.O_NOFOLLOW,0o600);locks.append(f);fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
 if exists(prefix):safe_directory(prefix)
 if exists(link) and not (link.is_symlink() and os.readlink(link)=='/opt/kairo/current/bin/kairo'):raise RuntimeError('Unknown global kairo command; refusing replacement/removal')
 if exists(unit):
  s=unit.lstat()
  if not stat.S_ISREG(s.st_mode) or s.st_uid!=0:raise RuntimeError('Unknown unit ownership/type')
  text=unit.read_text()
  # Fixed managed service identity; never stop an unrelated replacement service.
  required=['User=kairo','Group='+grp.getgrgid(pwd.getpwnam('kairo').pw_gid).gr_name,'Environment=HOME=/home/kairo','ExecStart=/opt/kairo/current/bin/kairo --data-dir /var/lib/kairo daemon','KillMode=control-group']
  if any(line not in text.splitlines() for line in required):raise RuntimeError('kairo.service differs from managed service identity; refusing removal')
 if exists(drop) and drop.is_symlink():raise RuntimeError('Unsafe service drop-in directory')
 load=show('LoadState')
 if load!='not-found':
  fragment=show('FragmentPath')
  if fragment!=str(unit):raise RuntimeError('Service is not the installer-managed unit: '+fragment)
  group=show('ControlGroup')
  if group and group!='/system.slice/kairo.service':raise RuntimeError('Unexpected service cgroup: '+group)
  system('disable','--now','kairo.service')
  # Stop is synchronous and KillMode=control-group includes Codex/helper children.
  if show('MainPID') not in ('','0') or show('ActiveState') not in ('inactive','failed'):raise RuntimeError('Service still running; refusing binary removal')
  if group:
   cg=pathlib.Path('/sys/fs/cgroup')/group.lstrip('/')
   if cg.exists():
    processes=[]
    for p in cg.rglob('cgroup.procs'):processes+=p.read_text().split()
    if processes:
     system('kill','--kill-who=all','--signal=KILL','kairo.service')
     time.sleep(.1)
     if any(p.read_text().strip() for p in cg.rglob('cgroup.procs')):raise RuntimeError('Service children still running')
 remaining=running()
 if remaining:raise RuntimeError('Kairo processes outside stopped unit remain; stop them first. PIDs='+','.join(map(str,remaining)))
 # Refuse to remove binaries while a manual daemon/installation owns their leases.
 for path in [pathlib.Path('/var/lib/kairo/daemon.lock'),prefix/'.install.lock']:
  if exists(path):
   f=os.open(path,os.O_RDWR|os.O_NOFOLLOW);locks.append(f);fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
 def unlink_known(path):
  if path.is_symlink() or path.is_file():path.unlink()
 def remove_empty(path):
  if path.is_symlink():path.unlink()
  elif path.exists():
   try:path.rmdir()
   except OSError:pass
 if exists(prefix):
  for dirname,names in {
   'bin':['kairo','.kairo.new','.kairo.rollback'],
   'manager':['kairo','.kairo.new'],
   'installer':['install.sh','system-uninstall.sh','.install.sh.new','.system-uninstall.sh.new'],
   'trust':['release-auth.sh','signed-update.sh'],
   '.upgrade-snapshot':['binary','current-target','config.json','runtime.json','state.json','cursor.json','deployment.json','accounts.json','status.json','natural-acceptance.json','confirmations.json'],
  }.items():
   directory=prefix/dirname
   if directory.is_symlink():directory.unlink();continue
   if directory.is_dir():
    for name in names:unlink_known(directory/name)
    remove_empty(directory)
  releases=prefix/'releases'
  if releases.is_symlink():releases.unlink()
  elif releases.is_dir():
   for release in releases.iterdir():
    if not re.fullmatch(r'(?:[0-9]+\.[0-9]+\.[0-9]+-[0-9a-f]{16}|\.new-[0-9]+)',release.name):continue
    if release.is_symlink():release.unlink();continue
    directory=release/'bin'
    if directory.is_symlink():directory.unlink()
    elif directory.is_dir():unlink_known(directory/'kairo');remove_empty(directory)
    remove_empty(release)
   remove_empty(releases)
  for name in ['current','.current.new','.install.lock','.upgrade-pending']:unlink_known(prefix/name)
  remove_empty(prefix)
 if exists(unit):unit.unlink()
 if exists(link):link.unlink()
 if drop.exists():
  known=drop/'20-mode.conf'
  if known.is_file() and not known.is_symlink():known.unlink()
  try:drop.rmdir()
  except OSError:pass
 system('daemon-reload')
 system('reset-failed','kairo.service',check=False)
 left=residuals()
 if left:raise RuntimeError('保留未知残留，未完全卸载 / Unknown residuals preserved: '+', '.join(left))
 if purge:
  user=pwd.getpwnam('kairo')
  if user.pw_dir!='/home/kairo':raise RuntimeError('Unexpected user home; refusing purge')
  for path in map(pathlib.Path,retained[:3]):
   if path.is_symlink():raise RuntimeError('Refusing linked data path: '+str(path))
  for path in map(pathlib.Path,retained[:3]):
   if path.exists():shutil.rmtree(path)
  print('已明确清除Kairo状态、请求历史及/home/kairo/.codex / Explicitly purged Kairo state/history and dedicated Codex authentication. Shared CLI/dependencies and user/ownership marker retained.')
 else:
  print('Kairo 已卸载，服务及其子进程已停止 / Kairo removed; service and managed children stopped.' if had_install else 'Kairo 未安装或已卸载 / Kairo is not installed or already removed.')
  print('保留 / Preserved: '+', '.join(retained))
 print('Codex CLI and shared system dependencies were not removed / 未删除Codex CLI或共享系统依赖。')
except Exception as e:
 print('卸载未完成 / Uninstall incomplete: '+str(e),file=sys.stderr)
 print('Remaining installation paths: '+', '.join(residuals()),file=sys.stderr)
 print('User credentials/state are preserved unless explicit purge completed.',file=sys.stderr)
 sys.exit(1)
finally:
 for f in locks:os.close(f)
PY
)
# No filesystem changes occur until the entire function above has been received.
kairo_uninstall_main "$@" </dev/null
