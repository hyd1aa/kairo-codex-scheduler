#!/bin/sh
# Provision only: no service start/enable, no OAuth, no production activation.
set -eu
kairo_lang=${KAIRO_LANG:-zh-CN}
msg() { if [ "$kairo_lang" = en-US ]; then printf '%s\n' "$2"; else printf '%s\n' "$1"; fi; }
[ "$#" -ge 3 ] && [ "$#" -le 4 ] || { msg '用法：system-install.sh 二进制 SHA256 现有用户 [隔离根目录]' 'usage: system-install.sh BINARY SHA256 EXISTING_USER [STAGING_ROOT]' >&2; exit 2; }
binary=$1
checksum=$2
account_user=$3
staging=${4:-}
printf '%s' "$account_user" | grep -Eq '^[a-z_][a-z0-9_-]*[$]?$' || exit 2
entry=$(getent passwd "$account_user") || exit 1
account_home=$(printf '%s' "$entry" | cut -d: -f6)
account_group=$(id -gn "$account_user")
printf '%s' "$account_home" | grep -Eq '^/[A-Za-z0-9_./-]+$' || exit 2
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
case "$staging" in
 '') prefix=/opt/kairo;;
 *) staging=$(realpath -m -- "$staging"); case "$staging" in /tmp/*|/opt/Kairo/sandbox/*) ;; *) exit 2;; esac; prefix="$staging/opt/kairo";;
esac
# Check every predictable collision before the Rust installer changes any state.
if [ -z "$staging" ] && [ ! -e /etc/localtime ] && [ ! -f /etc/timezone ]; then
 msg '系统时区未配置；请先安装 tzdata 并设置时区' 'System timezone is missing; install tzdata and configure the timezone first' >&2
 exit 1
fi
printf '%s' "$checksum" | grep -Eq '^[0-9a-f]{64}$' || exit 2
[ "$(sha256sum "$binary" | cut -d ' ' -f 1)" = "$checksum" ] || { msg '校验失败，未安装' 'Checksum mismatch; nothing installed' >&2; exit 1; }
link="$staging/usr/local/bin/kairo"
if [ -e "$link" ] || [ -L "$link" ]; then
 [ -L "$link" ] && [ "$(readlink "$link")" = "$prefix/current/bin/kairo" ] || { msg '已有未知来源的 kairo 命令，拒绝覆盖' 'Existing kairo command has unknown ownership; refusing replacement' >&2; exit 1; }
fi
unit="$staging/etc/systemd/system/kairo.service"
expected_unit=$(mktemp /tmp/kairo-unit.XXXXXXXX)
new_unit=0
new_dropin=0
new_link=0
cleanup() {
 result=$?
 if [ "$result" -ne 0 ]; then
  if [ "$new_link" -eq 1 ]; then rm -f "$link"; fi
  if [ "$new_dropin" -eq 1 ]; then rm -f "$staging/etc/systemd/system/kairo.service.d/20-mode.conf"; fi
  if [ "$new_unit" -eq 1 ]; then rm -f "$unit"; fi
  if [ -z "$staging" ] && { [ "$new_unit" -eq 1 ] || [ "$new_dropin" -eq 1 ]; }; then
   systemctl daemon-reload >/dev/null 2>&1 || :
  fi
 fi
 rm -f "$expected_unit"
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM
sed -e "s|@USER@|$account_user|g" -e "s|@GROUP@|$account_group|g" -e "s|@HOME@|$account_home|g" -e "s|@PREFIX@|$prefix|g" "$script_dir/kairo.service" > "$expected_unit"
if [ -e "$unit" ] || [ -L "$unit" ]; then
 [ -f "$unit" ] && [ ! -L "$unit" ] && [ "$(sha256sum "$expected_unit" | cut -d ' ' -f 1)" = "$(sha256sum "$unit" | cut -d ' ' -f 1)" ] || { msg '已有未知或修改过的 kairo.service，拒绝覆盖' 'Existing kairo.service differs; refusing replacement' >&2; exit 1; }
fi
if [ -n "$staging" ]; then
 mkdir -p "$staging/opt/kairo/releases"
 "$script_dir/install.sh" "$binary" "$checksum" "$staging/opt/kairo" "$staging/var/lib/kairo"
 prefix="$staging/opt/kairo"
else
 [ "$(id -u)" -eq 0 ] || { msg '系统安装需要 root 权限' 'System installation requires root' >&2; exit 1; }
 [ -d /run/systemd/system ] || { msg '系统安装需要 systemd' 'System installation requires systemd' >&2; exit 1; }
 systemctl daemon-reload || { msg 'systemd 无法重新加载；未开始安装' 'systemd cannot reload; installation has not started' >&2; exit 1; }
 had_runtime=0
 if [ -f /var/lib/kairo/runtime.json ]; then had_runtime=1; fi
 mkdir -p /opt/kairo/releases
 "$script_dir/install.sh" "$binary" "$checksum" /opt/kairo /var/lib/kairo
 prefix=/opt/kairo
fi
mkdir -p "$prefix/manager"
cp "$prefix/current/bin/kairo" "$prefix/manager/.kairo.new"
chmod 755 "$prefix/manager/.kairo.new"
mv -f "$prefix/manager/.kairo.new" "$prefix/manager/kairo"
chmod 755 "$prefix" "$prefix/bin" "$prefix/releases" "$prefix/manager"
mkdir -p "$staging/etc/systemd/system" "$staging/var/lib/kairo-account-guards"
if [ ! -e "$unit" ] && [ ! -L "$unit" ]; then new_unit=1; fi
cp "$expected_unit" "$unit"
chmod 644 "$staging/etc/systemd/system/kairo.service"
mkdir -p "$staging/etc/systemd/system/kairo.service.d"
if [ "$new_unit" -eq 1 ] && [ ! -f "$staging/etc/systemd/system/kairo.service.d/20-mode.conf" ]; then
 new_dropin=1
 printf '[Service]\nExecStart=\nExecStart=%s/current/bin/kairo --data-dir /var/lib/kairo daemon --observe\n' "$prefix" > "$staging/etc/systemd/system/kairo.service.d/20-mode.conf"
fi
mkdir -p "$staging/usr/local/bin"
if [ ! -L "$link" ]; then new_link=1; ln -s "$prefix/current/bin/kairo" "$link"; fi
if [ -z "$staging" ]; then
 # Runtime points to the existing user's OAuth; credentials are never copied.
 chmod 755 /opt/kairo /opt/kairo/bin
 codex_binary=/usr/local/bin/codex
 for candidate in "$account_home/.local/bin/codex" /usr/local/bin/codex /usr/bin/codex /opt/codex-window-scheduler/bin/codex; do if [ -x "$candidate" ]; then codex_binary=$candidate; break; fi; done
 if [ "$had_runtime" -eq 0 ]; then
  /opt/kairo/bin/kairo --data-dir /var/lib/kairo runtime --home "$account_home" --codex-home "$account_home/.codex" --codex "$codex_binary" >/dev/null
 fi
 chown -R "$account_user:$account_group" /var/lib/kairo /var/lib/kairo-account-guards
 chmod 700 /var/lib/kairo /var/lib/kairo-account-guards
 systemctl daemon-reload
fi
msg '服务模板已安装；未启用、未启动、未接管生产' 'Service provisioned; not enabled, not started, no production cutover'
