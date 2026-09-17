#!/usr/bin/env bash
# Mencari URL unduhan yang benar-benar ada dari sederet kandidat.
#
# Perlu karena tata letak unduhan upstream berubah-ubah: MySQL, misalnya,
# memindahkan rilis lama dari CDN "current" ke direktori arsip, dan sufiks
# glibc pada nama berkasnya berbeda antar seri. Menebak satu URL lalu berharap
# benar itu rapuh — di sini tiap kandidat dicoba dengan HEAD, yang pertama
# menjawab 200 yang dipakai, dan kalau semua gagal skrip berhenti sambil
# menyebutkan apa saja yang sudah dicoba.
#
# URL yang menang dicetak ke stdout; semua catatan lain ke stderr supaya
# pemanggil bisa `URL="$(resolve-upstream-url.sh ...)"`.
set -euo pipefail

if [ "$#" -eq 0 ]; then
  echo "pemakaian: $0 <url> [...]" >&2
  exit 2
fi

for url in "$@"; do
  code="$(curl -sSL -o /dev/null -w '%{http_code}' --head --max-time 60 "$url" 2>/dev/null || echo 000)"
  if [ "$code" = "200" ]; then
    echo "ditemukan (HTTP 200): $url" >&2
    printf '%s\n' "$url"
    exit 0
  fi
  echo "  HTTP $code  $url" >&2
done

echo "::error::tidak ada kandidat URL yang tersedia; yang dicoba:" >&2
printf '  %s\n' "$@" >&2
exit 1
