#!/bin/sh
# Provision a fresh dedicated Kairo account from an independently verified binary.
# The service remains disabled and in observation mode until human authorization.
set -eu
lang=${KAIRO_LANG:-zh-CN}
msg() { if [ "$lang" = en-US ]; then printf '%s\n' "$2"; else printf '%s\n' "$1"; fi; }
[ "$#" -eq 2 ] || { msg '用法：system-first-install.sh 已验证二进制 可信SHA256' 'usage: system-first-install.sh VERIFIED_BINARY TRUSTED_SHA256' >&2; exit 2; }
binary=$1
checksum=$2
[ "$(id -u)" -eq 0 ] || { msg '系统安装需要 root 权限' 'System installation requires root' >&2; exit 1; }
[ "$(uname -s)" = Linux ] || { msg '仅支持 Linux' 'Linux is required' >&2; exit 1; }
case "$(uname -m)" in
 x86_64) elf_machine=620;;
 aarch64|arm64) elf_machine=1830;;
 *) msg '不支持的处理器架构' 'Unsupported CPU architecture' >&2; exit 1;;
esac
[ -r /etc/os-release ] || { msg '无法识别 Linux 发行版' 'Cannot identify Linux distribution' >&2; exit 1; }
# /etc/os-release is supplied by the operating system, not the downloaded package.
. /etc/os-release
major=${VERSION_ID%%.*}
case "${ID:-}:$major" in
 debian:12|debian:13|ubuntu:22|ubuntu:24|almalinux:9|almalinux:10|rocky:9|rocky:10) ;;
 *) msg '不支持的 Linux 发行版或版本' 'Unsupported Linux distribution or version' >&2; exit 1;;
esac
case "${ID:-}:${VERSION_ID:-}" in
 ubuntu:22.04|ubuntu:24.04) ;;
 ubuntu:*) msg '不支持的 Ubuntu 版本' 'Unsupported Ubuntu version' >&2; exit 1;;
esac
[ -d /run/systemd/system ] && command -v systemctl >/dev/null 2>&1 || { msg '需要正在运行的 systemd' 'A running systemd is required' >&2; exit 1; }
[ -e /etc/localtime ] || [ -f /etc/timezone ] || { msg '系统时区未配置' 'System timezone is not configured' >&2; exit 1; }
printf '%s' "$checksum" | grep -Eq '^[0-9a-f]{64}$' || { msg 'SHA256 格式无效' 'Invalid SHA256 format' >&2; exit 2; }
[ -f "$binary" ] && [ "$(sha256sum "$binary" | cut -d ' ' -f1)" = "$checksum" ] || { msg '二进制 SHA256 校验失败' 'Binary SHA256 mismatch' >&2; exit 1; }
elf_magic=$(od -An -tx1 -N4 "$binary" | tr -d ' \n')
elf_class_endian=$(od -An -tu1 -j4 -N2 "$binary" | tr -d ' \n')
actual_machine=$(od -An -tu1 -j18 -N2 "$binary" | tr -d ' \n')
if [ "$elf_magic" != 7f454c46 ] || [ "$elf_class_endian" != 21 ] || [ "$actual_machine" != "$elf_machine" ]; then
 msg '二进制架构与系统不匹配' 'Binary architecture does not match this system' >&2
 exit 1
fi
if ! binary_version=$(timeout 10 "$binary" --version 2>/dev/null) || ! printf '%s\n' "$binary_version" | grep -Eq '^kairo [0-9]+\.[0-9]+\.[0-9]+$'; then
 msg '二进制无法在此系统运行或版本无效' 'Binary cannot run on this system or has an invalid version' >&2
 exit 1
fi
systemctl daemon-reload || { msg 'systemd 无法重新加载；未创建用户' 'systemd cannot reload; no user was created' >&2; exit 1; }
marker_dir=/var/lib/kairo-installer
marker=$marker_dir/managed-user
if getent passwd kairo >/dev/null; then
 [ -f "$marker" ] && [ ! -L "$marker" ] && [ "$(stat -c %u "$marker")" = 0 ] || { msg '已有非本安装器管理的 kairo 用户' 'Existing kairo user is not managed by this installer' >&2; exit 1; }
 [ "$(getent passwd kairo | cut -d: -f6)" = /home/kairo ] || { msg 'kairo 用户目录与预期不符' 'Kairo user home differs from the expected path' >&2; exit 1; }
else
 [ ! -e /opt/kairo ] && [ ! -L /usr/local/bin/kairo ] && [ ! -e /etc/systemd/system/kairo.service ] || { msg '已有 Kairo 安装残留；拒绝创建用户' 'Existing Kairo installation detected; refusing user creation' >&2; exit 1; }
 command -v useradd >/dev/null 2>&1 || { msg '缺少 useradd' 'useradd is required' >&2; exit 1; }
 mkdir -p "$marker_dir"
 chmod 700 "$marker_dir"
 # Persist ownership intent before useradd: SIGKILL after account creation must
 # remain distinguishable from a foreign pre-existing user on the next run.
 printf 'schema=1\n' > "$marker"
 chown root:root "$marker"
 chmod 600 "$marker"
 sync -f "$marker"
 useradd --system --create-home --home-dir /home/kairo --shell /bin/false kairo
fi
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
"$script_dir/system-install.sh" "$binary" "$checksum" kairo
msg 'Kairo 已安装；服务保持停用及只读观察。请由管理员为 kairo 用户完成人工 OAuth 授权，再检查状态。' 'Kairo is installed; its service remains disabled and observation-only. Complete human OAuth authorization for the kairo user, then check status.'
msg '检查：kairo status；授权：sudo -u kairo kairo auth login' 'Check: kairo status; authorize: sudo -u kairo kairo auth login'
