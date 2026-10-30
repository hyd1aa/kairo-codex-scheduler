#!/usr/bin/env bash
# Buffer the complete function before executing a streamed installer.
kairo_install_main() (
# Default trust: GitHub HTTPS + SHA256 integrity. No signature downgrade/fallback.
set -euo pipefail
export LC_ALL=C
lang=${KAIRO_LANG:-zh-CN}
msg() { if [[ $lang == en-US ]]; then printf '%s\n' "$2"; else printf '%s\n' "$1"; fi; }
fail() { msg "$1" "$2" >&2; exit 1; }
version= source_dir= base= action=install skip_codex=0
while (($#)); do
 case $1 in
  --version|--source-dir|--base-url) (($#>=2)) || exit 2; case $1 in --version) version=$2;; --source-dir) source_dir=$2;; --base-url) base=$2;; esac; shift 2;;
  --update) action=update; shift;;
  --skip-codex) skip_codex=1; shift;;
  --help) echo 'install.sh [--version X.Y.Z] [--update] [--source-dir DIR | --base-url HTTPS_BASE] [--skip-codex]'; exit 0;;
  *) echo "Unknown option: $1" >&2; exit 2;;
 esac
done
[[ $(id -u) == 0 ]] || fail '请使用 sudo bash install.sh' 'Run with sudo bash install.sh'
[[ $(uname -s) == Linux && -r /etc/os-release ]] || fail '仅支持已验证的 Linux 系统' 'A supported Linux system is required'
. /etc/os-release
case "${ID:-}:${VERSION_ID:-}" in
 debian:12|debian:13|ubuntu:22.04|ubuntu:24.04|almalinux:9|almalinux:9.*|rocky:9|rocky:9.*) ;;
 *) fail '此系统版本暂不支持自动安装（Alma/Rocky 10 尚未完成首装验收）' 'Unsupported OS version (Alma/Rocky 10 first installation is not validated)';;
esac
case $(uname -m) in x86_64) target=x86_64-unknown-linux-gnu;; aarch64|arm64) target=aarch64-unknown-linux-gnu;; *) fail '不支持的 CPU 架构' 'Unsupported CPU architecture';; esac
[[ -d /run/systemd/system ]] && command -v systemctl >/dev/null || fail '需要运行中的 systemd' 'A running systemd is required'
[[ -z $source_dir || -z $base ]] || exit 2
[[ -z $version || $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 2
[[ -z $base || $base =~ ^https://[A-Za-z0-9.-]+(:[0-9]+)?(/[A-Za-z0-9._/-]*)?$ ]] || fail '下载地址必须为 HTTPS' 'Download base must use HTTPS'
if [[ -n $source_dir ]]; then [[ -n $version && -d $source_dir ]] || exit 2; fi
missing=0
for tool in curl sha256sum python3 tar gzip useradd getent flock timeout; do command -v "$tool" >/dev/null || missing=1; done
[[ -e /etc/localtime || -f /etc/timezone ]] || missing=1
[[ -s /etc/ssl/certs/ca-certificates.crt || -s /etc/pki/tls/certs/ca-bundle.crt ]] || missing=1
if ((missing)); then
 if command -v apt-get >/dev/null; then
  timeout 180 apt-get update -qq
  DEBIAN_FRONTEND=noninteractive timeout 180 apt-get install -y -qq --no-install-recommends ca-certificates curl coreutils python3 tar gzip passwd tzdata util-linux
 elif command -v dnf >/dev/null || command -v microdnf >/dev/null; then
  pm=dnf; command -v dnf >/dev/null || pm=microdnf
  packages=(ca-certificates curl coreutils python3 tar gzip shadow-utils tzdata util-linux)
  if [[ $pm == microdnf ]] && command -v rpm >/dev/null && rpm -q coreutils-single >/dev/null 2>&1; then
   packages=(ca-certificates curl python3 tar gzip shadow-utils tzdata util-linux)
  fi
  timeout 180 "$pm" -y install "${packages[@]}"
 else fail '缺少可用包管理器' 'No supported package manager'; fi
fi
msg 'Kairo：安装授权在安全条件满足后自动预热；可随时暂停。升级继承已有授权，不自动取消暂停。' 'Kairo: installation authorizes automatic warmup once safe; pause anytime. Upgrades preserve existing intent and pauses.'
stage=$(mktemp -d /tmp/kairo-online.XXXXXXXX)
chmod 700 "$stage"
was_active=0; changed=0; old_hash=; completed=0; fresh_install=0; new_auto_intent=0
cleanup() {
 result=$?
 trap - EXIT
 if ((result!=0 && changed)) && [[ -x /opt/kairo/manager/kairo ]]; then
  /opt/kairo/manager/kairo --data-dir /var/lib/kairo update recover --prefix /opt/kairo || :
  current_hash=$(sha256sum /opt/kairo/current/bin/kairo 2>/dev/null | cut -d ' ' -f1) || current_hash=
  if [[ -n $old_hash && $current_hash != "$old_hash" ]]; then
   /opt/kairo/manager/kairo --data-dir /var/lib/kairo update rollback --prefix /opt/kairo || :
  fi
 fi
 if ((was_active && !completed)); then
  current_hash=$(sha256sum /opt/kairo/current/bin/kairo 2>/dev/null | cut -d ' ' -f1) || current_hash=
  [[ -n $old_hash && $current_hash == "$old_hash" ]] && systemctl start kairo.service || :
 fi
 rm -rf -- "$stage"
 exit "$result"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
fetch() {
 curl --disable --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --tlsv1.2 --connect-timeout 15 --max-time 120 --max-filesize "${3:-104857600}" --output "$2" "$1"
}
if [[ -z $version ]]; then
 fetch https://api.github.com/repos/hyd1aa/kairo-codex-scheduler/releases/latest "$stage/latest.json"
 version=$(python3 - "$stage/latest.json" <<'PY'
import json,re,sys
v=json.load(open(sys.argv[1]));tag=v['tag_name']
assert re.fullmatch(r'v[0-9]+\.[0-9]+\.[0-9]+',tag) and not v.get('draft') and not v.get('prerelease')
print(tag[1:])
PY
 )
fi
msg "选择稳定正式版：$version（$target）" "Selected stable release: $version ($target)"
base=${base:-https://github.com/hyd1aa/kairo-codex-scheduler/releases/download/v$version}
asset="kairo-$version-$target.tar.gz"
for name in SHA256SUMS "$asset"; do
 if [[ -n $source_dir ]]; then cp -- "$source_dir/$name" "$stage/$name"; else fetch "$base/$name" "$stage/$name"; fi
done
python3 - "$stage" "$asset" "$version" "$target" <<'PY'
import hashlib,pathlib,re,sys,tarfile,struct
stage=pathlib.Path(sys.argv[1]);asset,version,target=sys.argv[2:];entries={}
for line in (stage/'SHA256SUMS').read_text('ascii').splitlines():
 m=re.fullmatch(r'([0-9a-f]{64})  ([A-Za-z0-9_.-]+)',line)
 if not m or m[2] in entries:raise SystemExit('Invalid/duplicate SHA256 manifest entry')
 entries[m[2]]=m[1]
if entries.get(asset)!=hashlib.sha256((stage/asset).read_bytes()).hexdigest():raise SystemExit('Release SHA256 mismatch; nothing installed')
root=f'kairo-{version}';seen=set();total=0
with tarfile.open(stage/asset) as archive:
 members=archive.getmembers()
 for m in members:
  p=pathlib.PurePosixPath(m.name)
  if p.is_absolute() or '..' in p.parts or not p.parts or p.parts[0]!=root or m.name in seen or not (m.isdir() or m.isfile()):raise SystemExit('Unsafe archive entry')
  seen.add(m.name);total+=m.size
  if total>300*1024*1024:raise SystemExit('Archive too large')
 # Only private staging, regular files/directories, no symlinks or special files.
 for m in members:
  p=stage/m.name
  if m.isdir():p.mkdir(parents=True,exist_ok=True)
  else:
   p.parent.mkdir(parents=True,exist_ok=True)
   with p.open('xb') as f:f.write(archive.extractfile(m).read())
   p.chmod(0o755 if m.mode & 0o111 else 0o644)
for required in ['install.sh','packaging/system-first-install.sh','packaging/system-install.sh','packaging/install.sh','packaging/system-uninstall.sh','packaging/kairo.service']:
 assert (stage/root/required).is_file(), 'Incomplete installation package: '+required
binary=stage/root/'bin'/f'kairo-{target}'
b=binary.read_bytes();assert b[:6]==b'\x7fELF\x02\x01' and struct.unpack('<H',b[18:20])[0]==(62 if target.startswith('x86_64') else 183), 'ELF architecture mismatch'
rows=(stage/root/'SHA256SUMS').read_text('ascii').splitlines()
assert rows==[hashlib.sha256(binary.read_bytes()).hexdigest()+'  bin/kairo-'+target], 'Invalid bundled binary checksum'
PY
root="$stage/kairo-$version"
binary="$root/bin/kairo-$target"
[[ $(timeout 10 "$binary" --version) == "kairo $version" ]] || fail '程序版本或架构不匹配' 'Binary version or architecture mismatch'
[[ ! -L /var/lib/kairo-installer ]] || exit 1
mkdir -p /var/lib/kairo-installer
[[ $(stat -c %u /var/lib/kairo-installer) == 0 && $((8#$(stat -c %a /var/lib/kairo-installer)&022)) == 0 ]] || exit 1
chmod 700 /var/lib/kairo-installer
[[ ! -L /var/lib/kairo-installer/online.lock ]] || exit 1
exec 9>/var/lib/kairo-installer/online.lock
flock -n 9 || fail '另一个安装或更新正在运行' 'Another installation or update is running'
# Install the official native CLI only if absent; never authenticate or overwrite an existing CLI.
if (( !skip_codex )) && [[ ! -x /usr/local/bin/codex && ! -x /usr/bin/codex && ! -x /home/kairo/.local/bin/codex ]]; then
 codex_version=0.164.0
 case $target in
  x86_64*) codex_arch=x86_64; codex_sha=90dd0b03b28afcb7fa733a4f73b413db2916a47347efd43fb669cc1c774ee8e4;;
  aarch64*) codex_arch=aarch64; codex_sha=29a507a2a3fd8f6cd94b96b762d281d299d15d7902dee6132d314b78cf9108eb;;
 esac
 codex_name="codex-$codex_arch-unknown-linux-musl"
 fetch "https://github.com/openai/codex/releases/download/rust-v$codex_version/$codex_name.tar.gz" "$stage/codex.tar.gz" 268435456
 printf '%s  %s\n' "$codex_sha" "$stage/codex.tar.gz" | sha256sum -c -
 python3 - "$stage" "$codex_name" <<'PYCODEX'
import pathlib,sys,tarfile
p=pathlib.Path(sys.argv[1]);name=sys.argv[2]
with tarfile.open(p/'codex.tar.gz') as t:
 members=t.getmembers();files=[m for m in members if m.isfile()]
 assert len(files)==1 and files[0].name==name and all(m.isfile() for m in members), 'Unexpected official Codex archive layout'
 assert files[0].size<300*1024*1024
 (p/'codex').write_bytes(t.extractfile(files[0]).read());(p/'codex').chmod(0o755)
PYCODEX
 timeout 10 "$stage/codex" --version
 mkdir -p /usr/local/bin
 [[ ! -e /usr/local/bin/codex && ! -L /usr/local/bin/codex ]] || exit 1
 install -m 755 "$stage/codex" /usr/local/bin/.codex-kairo.new
 mv -f /usr/local/bin/.codex-kairo.new /usr/local/bin/codex
fi
digest=$(sha256sum "$binary" | cut -d ' ' -f1)
if [[ -e /opt/kairo/current/bin/kairo ]]; then
 [[ -f /var/lib/kairo-installer/managed-user && -x /opt/kairo/manager/kairo ]] || fail '现有安装不属于此安装器' 'Existing installation is not managed by this installer'
 installed_version=$(/opt/kairo/current/bin/kairo --version)
 python3 - "$installed_version" "$version" <<'PYVERSION'
import re,sys
m=re.fullmatch(r'kairo ([0-9]+)\.([0-9]+)\.([0-9]+)',sys.argv[1]);assert m, 'Invalid installed version'
assert tuple(map(int,sys.argv[2].split('.')))>=tuple(map(int,m.groups())), 'Downgrade refused; use explicit rollback'
PYVERSION
 old_hash=$(sha256sum /opt/kairo/current/bin/kairo | cut -d ' ' -f1)
 if [[ $old_hash != "$digest" ]]; then
  if systemctl is-active --quiet kairo.service; then was_active=1; systemctl stop kairo.service; fi
  changed=1
  /opt/kairo/manager/kairo --data-dir /var/lib/kairo update apply "$binary" --sha256 "$digest" --prefix /opt/kairo
 fi
else
 [[ $action != update ]] || fail 'Kairo 尚未安装' 'Kairo is not installed'
 fresh_install=1
 sh "$root/packaging/system-first-install.sh" "$binary" "$digest"
fi
[[ $(sha256sum /opt/kairo/current/bin/kairo | cut -d ' ' -f1) == "$digest" ]] || fail '安装后哈希不一致' 'Installed SHA256 mismatch'
[[ ! -L /opt/kairo/installer ]] || exit 1
mkdir -p /opt/kairo/installer
chmod 755 /opt/kairo/installer
# Helpers come from this already trusted HTTPS bootstrap, not the old immutable archive.
{ printf '#!/usr/bin/env bash\n'; declare -f kairo_install_main; printf '\nkairo_install_main "$@" </dev/null\n'; } > "$stage/installed-install.sh"
# BEGIN_EMBEDDED_UNINSTALLER
cat > "$stage/system-uninstall.sh" <<'KAIRO_UNINSTALL_EOF'
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
 for name in ['kairo-onboarding.timer','kairo-onboarding.service']:
  task=unit.parent/name
  if exists(task):
   safe_directory(task.parent)
   m=task.lstat()
   if not stat.S_ISREG(m.st_mode) or m.st_uid!=0 or m.st_mode&0o022:raise RuntimeError('Unsafe initialization unit')
   body=task.read_text()
   expected='Unit=kairo-onboarding.service' if name.endswith('.timer') else 'ExecStart=/opt/kairo/current/bin/kairo --data-dir /var/lib/kairo onboard'
   if expected not in body.splitlines():raise RuntimeError('Unknown initialization unit; preserved')
   system('disable','--now',name) if name.endswith('.timer') else system('stop',name)
   task.unlink()
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
KAIRO_UNINSTALL_EOF
# END_EMBEDDED_UNINSTALLER
for name in install.sh system-uninstall.sh; do
 source="$stage/installed-install.sh"; [[ $name == install.sh ]] || source="$stage/system-uninstall.sh"
 install -m 755 "$source" "/opt/kairo/installer/.$name.new"
 mv -f "/opt/kairo/installer/.$name.new" "/opt/kairo/installer/$name"
done
if ((was_active)); then systemctl start kairo.service; fi
# Installing authorizes automatic warmup once all safety gates pass. Existing
# installations retain their intent, pause, deployment and observation override.
if /opt/kairo/current/bin/kairo onboard --help >/dev/null 2>&1; then
 for unit in kairo-onboarding.service kairo-onboarding.timer; do
  target="/etc/systemd/system/$unit"
  source="$root/packaging/$unit"
  [[ -f $source && ! -L $target ]] || fail '初始化定时任务文件不安全' 'Unsafe initialization timer file'
  if [[ -e $target ]]; then
   cmp -s "$source" "$target" || fail '保留已有自定义初始化任务，请人工检查' 'Preserving custom initialization task; manual review required'
  else
   install -o root -g root -m 644 "$source" "$target"
  fi
 done
 systemctl daemon-reload
 if (( fresh_install )); then
  msg '安装即授权：安全条件满足后自动预热；可随时在菜单04暂停。' 'Installation authorizes automatic warmup once safe; pause any time in menu04.'
  # Import only the invoking administrator's private Codex login, never a browser
  # session. Strict owner/mode/no-follow checks; no token in argv/stdout/logs.
  python3 - <<'PYOAUTH'
import os,pwd,json,stat
try:
 target=pwd.getpwnam('kairo')
 user=pwd.getpwuid(int(os.environ.get('SUDO_UID','0')))
 def directory(path,owner):
  fd=os.open(path,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
  m=os.fstat(fd)
  if m.st_uid!=owner or m.st_mode & 0o022:os.close(fd);raise ValueError('unsafe directory')
  return fd
 home=directory('/home/kairo',target.pw_uid)
 try:os.mkdir('.codex',0o700,dir_fd=home)
 except FileExistsError:pass
 dest=os.open('.codex',os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=home)
 m=os.fstat(dest)
 if m.st_uid==0:os.fchown(dest,target.pw_uid,target.pw_gid)
 elif m.st_uid!=target.pw_uid:raise ValueError('wrong target owner')
 if m.st_mode & 0o077:raise ValueError('target must be private')
 try:os.stat('auth.json',dir_fd=dest,follow_symlinks=False)
 except FileNotFoundError:
  source_home=directory(user.pw_dir,user.pw_uid)
  source=os.open('.codex',os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=source_home)
  sm=os.fstat(source)
  if sm.st_uid!=user.pw_uid or sm.st_mode & 0o022:raise ValueError('source must be private')
  f=os.open('auth.json',os.O_RDONLY|os.O_NOFOLLOW,dir_fd=source)
  m=os.fstat(f)
  if not stat.S_ISREG(m.st_mode) or m.st_uid!=user.pw_uid or m.st_nlink!=1 or m.st_mode & 0o077:raise ValueError('unsafe source')
  data=os.read(f,1048577)
  if len(data)>1048576:raise ValueError('oversize')
  v=json.loads(data)
  assert not v.get('OPENAI_API_KEY') and v['tokens']['account_id'] and v['tokens']['access_token']
  out=os.open('auth.json',os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=dest)
  os.fchown(out,target.pw_uid,target.pw_gid)
  with os.fdopen(out,'wb') as w:w.write(data);w.flush();os.fsync(w.fileno())
  os.fsync(dest)
  print('已安全复用管理员 Codex CLI 登录 / Reused administrator Codex CLI login securely')
except Exception:
 print('未导入管理员凭据；可在账号菜单完成官方登录 / No administrator credential imported; use the account menu for official login')
PYOAUTH
  new_auto_intent=0
  if [[ ! -e /var/lib/kairo/onboarding.json && ! -L /var/lib/kairo/onboarding.json && ! -e /var/lib/kairo/deployment.json && ! -L /var/lib/kairo/deployment.json ]]; then
   /opt/kairo/current/bin/kairo onboard --request
   new_auto_intent=1
  fi
  systemctl enable --now kairo.service
 fi
 systemctl enable --now kairo-onboarding.timer
 if (( fresh_install && new_auto_intent )) && [[ -r /dev/tty ]] && [[ -w /dev/tty ]]; then
  if ! /usr/sbin/runuser -u kairo -- test -r /home/kairo/.codex/auth.json; then
   msg '请完成官方 Codex OAuth 登录。' 'Complete official Codex OAuth login.'
   /usr/sbin/runuser -u kairo -- /opt/kairo/current/bin/kairo auth login </dev/tty >/dev/tty 2>&1 || :
  fi
  msg '其他机器使用同账号无法自动检测。已停止其他调度器，且恢复等待期间不手动请求？[yes/N]' 'Other machines cannot be checked. Other schedulers stopped, and no manual requests during recovery? [yes/N]'
  answer=; read -r answer </dev/tty || :
  if [[ $answer == yes ]]; then /opt/kairo/current/bin/kairo onboard --request --confirm-exclusive-account || msg '请先完成登录，再在菜单04确认。' 'Complete login, then confirm once in menu04.'; fi
 fi
 systemctl start --no-block kairo-onboarding.service
fi

completed=1
msg 'Kairo 已就绪。输入 sudo kairo：03 登录，04 启动/暂停。安全恢复自动复查；升级保留已有运行及暂停状态。' 'Kairo is ready. Run sudo kairo: 03 login, 04 start/pause. Safe recovery rechecks automatically; upgrades preserve running/paused state.'
msg '无交互安装不会等待认证；需要登录或一次账号独占确认时，进入 sudo kairo 菜单。' 'Noninteractive installation never waits for authentication. Use sudo kairo for login or a one-time exclusive-account confirmation.' 

)
# Child commands cannot consume shell source or interactive answers from the pipe.
kairo_install_main "$@" </dev/null
