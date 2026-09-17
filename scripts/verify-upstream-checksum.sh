#!/usr/bin/env bash
# Mencocokkan sebuah berkas dengan checksum yang diterbitkan upstream.
#
# Tiap URL kandidat dicoba berurutan; yang pertama berhasil diambil dipakai.
# Formatnya boleh berupa satu baris sha256 telanjang, atau berkas gaya
# `sha256sums.txt` berisi banyak baris "<sha256>  <nama berkas>" — baris yang
# cocok dengan nama berkas kita yang dipakai.
#
# Kalau tidak ada kandidat yang tersedia, ini BUKAN error: skrip hanya
# memperingatkan dan mencetak sha256 yang terukur, supaya operator bisa
# mencocokkannya sendiri dengan halaman unduhan resmi. Yang dilarang adalah
# mengarang nilai, bukan mengakui bahwa upstream tidak menerbitkannya.
#
# Pemakaian: verify-upstream-checksum.sh <berkas> <url-checksum> [...]
set -euo pipefail

file="${1:?pemakaian: $0 <berkas> <url-checksum> [...]}"
shift

name="$(basename "$file")"
actual="$(sha256sum "$file" | cut -d' ' -f1)"
echo "sha256 terukur: $actual  ($name)"

for url in "$@"; do
  echo "mencoba checksum upstream: $url"
  if ! body="$(curl -fsSL --retry 2 --max-time 60 "$url" 2>/dev/null)"; then
    echo "  tidak tersedia"
    continue
  fi

  expected="$(printf '%s\n' "$body" | awk -v n="$name" '
    NF == 1 && length($1) == 64 { print $1; exit }
    NF >= 2 {
      f = $NF
      sub(/^\*/, "", f)
      sub(/.*\//, "", f)
      if (f == n) { print $1; exit }
    }')"

  if [ -z "$expected" ]; then
    echo "  terambil, tapi tidak ada baris untuk $name"
    continue
  fi

  if [ "$expected" = "$actual" ]; then
    echo "  cocok dengan checksum resmi upstream."
    exit 0
  fi
  echo "::error::checksum upstream $expected tidak cocok dengan berkas terunduh $actual"
  exit 1
done

echo "::warning::Tidak ada checksum upstream yang bisa diambil otomatis untuk" \
     "$name. sha256 yang dipakai manifest diukur dari unduhan HTTPS di runner" \
     "ini: $actual — cocokkan sekali dengan halaman unduhan resmi."
