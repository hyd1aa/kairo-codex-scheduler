#!/bin/sh
set -eu
staging=${1:-}
if [ -n "$staging" ]; then
 staging=$(realpath -m -- "$staging")
 case "$staging" in /tmp/*|/opt/Kairo/sandbox/*) ;; *) exit 2;; esac
else
 [ "$(id -u)" -eq 0 ] || exit 1
 systemctl disable --now kairo-onboarding.timer 2>/dev/null || :
 systemctl stop kairo-onboarding.service 2>/dev/null || :
 systemctl disable --now kairo.service
fi
"$staging/opt/kairo/bin/kairo" --lang "${KAIRO_LANG:-zh-CN}" --data-dir "$staging/var/lib/kairo" uninstall --prefix "$staging/opt/kairo"
rm -f "$staging/etc/systemd/system/kairo.service" "$staging/etc/systemd/system/kairo.service.d/20-mode.conf"
link="$staging/usr/local/bin/kairo"
if [ -L "$link" ] && [ "$(readlink "$link")" = "$staging/opt/kairo/current/bin/kairo" ]; then rm "$link"; fi
if [ -z "$staging" ]; then systemctl daemon-reload; fi

# Signed bootstrap trust scripts are public root-owned code, not account data.
trust_dir="$staging/opt/kairo/trust"
if [ -d "$trust_dir" ] && [ ! -L "$trust_dir" ]; then
 rm -f "$trust_dir/release-auth.sh" "$trust_dir/signed-update.sh"
 rmdir "$trust_dir" 2>/dev/null || :
fi

# Public installer helpers contain no user credentials or journals.
if [ -d "$staging/opt/kairo/installer" ] && [ ! -L "$staging/opt/kairo/installer" ]; then
 rm -f "$staging/opt/kairo/installer/install.sh" "$staging/opt/kairo/installer/system-uninstall.sh"
 rmdir "$staging/opt/kairo/installer" 2>/dev/null || :
fi
