#!/bin/sh
set -eu
[ "$#" -ge 1 ] && [ "$#" -le 2 ] || exit 2
prefix=$(realpath -m -- "$1")
data=${2:-$prefix/var/lib/kairo}
exec "$prefix/bin/kairo" --lang "${KAIRO_LANG:-zh-CN}" --data-dir "$data" uninstall --prefix "$prefix"
