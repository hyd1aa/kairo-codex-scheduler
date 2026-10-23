#!/bin/sh
# Verified Rust installer. Installs configuration and binary; never starts a service.
set -eu
kairo_lang=${KAIRO_LANG:-zh-CN}
msg() { if [ "$kairo_lang" = en-US ]; then printf '%s\n' "$2"; else printf '%s\n' "$1"; fi; }
[ "$#" -ge 3 ] && [ "$#" -le 4 ] || { msg '用法：install.sh 二进制 SHA256 安装目录 [数据目录]' 'usage: install.sh BINARY SHA256 PREFIX [DATA_DIR]' >&2; exit 2; }
binary=$(realpath -- "$1")
expected=$2
prefix=$(realpath -m -- "$3")
data=${4:-$prefix/var/lib/kairo}
actual=$(sha256sum "$binary" | cut -d ' ' -f 1)
[ "$actual" = "$expected" ] || { msg '校验失败，未执行程序' 'Checksum mismatch; binary was not executed' >&2; exit 1; }
"$binary" --lang "$kairo_lang" --data-dir "$data" install --binary "$binary" --sha256 "$expected" --prefix "$prefix"
msg '安装完成；服务未启动，默认仅观察' 'Installed; service not started; observation is the default'
