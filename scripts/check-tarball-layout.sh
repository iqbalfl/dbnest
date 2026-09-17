#!/usr/bin/env bash
# Memastikan sebuah tarball engine berbentuk seperti yang diharapkan manifest:
# satu direktori teratas (supaya `strip_components: 1` menghasilkan bin/ di akar)
# dan berkas-berkas wajib benar-benar ada setelah pemangkasan itu.
#
# Pemakaian: check-tarball-layout.sh <tarball> <path-wajib> [...]
#   mis. check-tarball-layout.sh mysql.tar.xz bin/mysqld bin/mysql
set -euo pipefail

tarball="${1:?pemakaian: $0 <tarball> <path-wajib> [...]}"
shift
if [ "$#" -eq 0 ]; then
  echo "::error::tidak ada path wajib yang diberikan" >&2
  exit 2
fi

listing="$(tar -tf "$tarball")"

# Semua entri harus berada di bawah satu direktori teratas yang sama.
roots="$(printf '%s\n' "$listing" | awk -F/ 'NF > 0 && $1 != "" { print $1 }' | sort -u)"
count="$(printf '%s\n' "$roots" | grep -c . || true)"
if [ "$count" -ne 1 ]; then
  echo "::error::tarball punya $count direktori teratas, strip_components: 1 tidak cocok:"
  printf '%s\n' "$roots" | head -20
  exit 1
fi
root="$roots"
echo "direktori teratas: $root"

fail=0
for want in "$@"; do
  if printf '%s\n' "$listing" | grep -qxF "$root/$want"; then
    echo "  ok  $want"
  else
    echo "::error::$want tidak ada di tarball (dicari sebagai $root/$want)"
    fail=1
  fi
done

exit "$fail"
