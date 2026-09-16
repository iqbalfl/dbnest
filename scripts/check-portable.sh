#!/usr/bin/env bash
# Memeriksa apakah sebuah pohon instalasi engine aman dipakai lintas distro.
#
# Dua hal yang diperiksa untuk setiap ELF di dalamnya:
#
#   1. Dependensi dinamis (DT_NEEDED) hanya boleh library yang SONAME-nya sama
#      di semua distro target, atau library yang ikut dipaketkan di pohon ini.
#      libreadline, libicu, dan libssl sengaja tidak masuk daftar: SONAME-nya
#      berbeda antara EL8 dan Ubuntu 24.04, jadi binary yang menautnya akan
#      gagal jalan di sisi pengguna.
#   2. Simbol glibc yang diminta tidak boleh lebih baru dari $MAX_GLIBC. Ini
#      yang membuat build di container glibc 2.28 tetap jalan di distro lama.
#
# Pemakaian: scripts/check-portable.sh <dir> [...]
set -euo pipefail

max_glibc="${MAX_GLIBC:-2.28}"

if [ "$#" -eq 0 ]; then
  echo "pemakaian: $0 <dir> [...]" >&2
  exit 2
fi

# SONAME yang stabil di semua distro target.
base_allowed="
ld-linux-x86-64.so.2
ld-linux-aarch64.so.1
libc.so.6
libcrypt.so.1
libdl.so.2
libgcc_s.so.1
libm.so.6
libpthread.so.0
librt.so.1
libstdc++.so.6
libutil.so.1
libz.so.1
linux-vdso.so.1
"

fail=0
checked=0

for root in "$@"; do
  if [ ! -d "$root" ]; then
    echo "::error::$root bukan direktori"
    fail=1
    continue
  fi

  # Library yang ikut dipaketkan boleh saling menaut. Symlink ikut didata:
  # DT_NEEDED menyebut SONAME (libpq.so.5) sedangkan berkas sungguhannya
  # bernama lengkap (libpq.so.5.16), dan yang menjembatani keduanya symlink.
  own=$(find "$root" \( -type f -o -type l \) -name '*.so*' -printf '%f\n' \
    2>/dev/null | sort -u || true)
  allowed="$base_allowed
$own"

  while IFS= read -r -d '' bin; do
    case "$(file -b "$bin" 2>/dev/null || true)" in
      *ELF*) ;;
      *) continue ;;
    esac
    checked=$((checked + 1))

    while read -r lib; do
      [ -n "$lib" ] || continue
      if ! printf '%s\n' "$allowed" | grep -qxF "$lib"; then
        echo "::error::$bin menaut $lib, yang versinya berbeda antar distro"
        fail=1
      fi
    done < <(objdump -p "$bin" 2>/dev/null | awk '$1 == "NEEDED" { print $2 }')

    newest=$(objdump -T "$bin" 2>/dev/null \
      | grep -o 'GLIBC_[0-9][0-9.]*' \
      | sed 's/^GLIBC_//; s/\.$//' \
      | sort -V | tail -n1 || true)
    if [ -n "$newest" ] \
      && [ "$(printf '%s\n%s\n' "$max_glibc" "$newest" | sort -V | tail -n1)" != "$max_glibc" ]; then
      echo "::error::$bin butuh glibc $newest, lebih baru dari $max_glibc"
      fail=1
    fi
  done < <(find "$root" -type f -print0)
done

if [ "$checked" -eq 0 ]; then
  echo "::error::tidak ada ELF yang diperiksa — argumen salah?"
  exit 1
fi

echo "$checked ELF diperiksa, batas glibc $max_glibc."
exit "$fail"
