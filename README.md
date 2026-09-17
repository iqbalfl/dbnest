# DBnest

Server database lokal untuk Linux — pilih engine, pilih versi, isi port, lalu
Start. Berjalan native tanpa Docker, tanpa VM, dan tanpa root.

DBnest adalah padanan [DBngin](https://dbngin.com/) untuk Linux. Nama dan aset
DBngin/TablePlus tidak dipakai di proyek ini.

> **Status: masih pra-rilis.** Manifest bawaan kini berisi artefak sungguhan
> untuk keempat engine di x86_64, jadi `dbnest start` sudah bisa memasang dan
> menjalankan server. Yang belum: belum ada rilis biner, aarch64 belum ada, dan
> pengujian lintas distro baru mencakup Debian sekeluarga. Lihat
> [Yang belum selesai](#yang-belum-selesai).

## Apa yang sudah ada

| Bagian | Status |
|---|---|
| Engine PostgreSQL, Redis, MySQL, MariaDB | Adapter lengkap (§7.2) |
| MongoDB | Belum — di luar cakupan saat ini |
| CLI `dbnest` | Lengkap sesuai DESIGN.md §15 |
| GUI (Tauri v2 + React) | Daftar server, New Server, Connection/Logs/Settings, tray |
| Backend proses | `systemd --user` dan fallback langsung (`direct`) |
| Preflight | root, library (`ldd`), port, panjang path socket, ruang disk |
| Autostart | Lewat unit systemd yang di-`enable`, atau saat aplikasi dibuka |

## Instalasi

Belum ada rilis biner. Sampai ada, jalankan dari source (lihat
[Pengembangan](#pengembangan)).

Setelah ada rilis, workflow `release.yml` menghasilkan AppImage, `.deb`, `.rpm`,
dan tarball CLI terpisah untuk x86_64. Bundle di-build di Ubuntu 22.04 supaya
glibc-nya cukup tua untuk distro lama.

### Distro yang didukung

Butuh glibc ≥ 2.28: Ubuntu 20.04+, Debian 11+, Fedora 36+, RHEL/Rocky 8+, Arch.
Distro berbasis musl (Alpine) tidak didukung.

## Pemakaian CLI

```bash
dbnest engines                      # daftar engine & versi di manifest
dbnest versions [--installed]       # versi di manifest / yang sudah terpasang
dbnest create postgres 16.4 --name "Proyek A" --port 5433
dbnest start "Proyek A"             # install + init otomatis kalau perlu
dbnest list                         # instance + status + port
dbnest info "Proyek A"              # host/port/user/URL koneksi
dbnest logs "Proyek A" -n 200
dbnest shell "Proyek A"             # $SHELL dengan PATH/env engine ini
eval "$(dbnest env 'Proyek A')"     # env yang sama, di shell saat ini
dbnest stop "Proyek A"
dbnest delete "Proyek A" [--keep-data]
dbnest uninstall postgres 16.4      # hapus versi (ditolak kalau masih dipakai)
dbnest doctor                       # preflight semua instance + info sistem
```

Semua perintah menerima `--json` untuk keperluan skrip. Exit code: `0` sukses,
`1` error umum, `2` argumen salah, `3` preflight gagal.

Instance bisa dirujuk lewat id (`pg-7f3a2c`) maupun namanya.

### Kredensial default

Sama seperti DBngin, server dibuat untuk development: PostgreSQL memakai user
`postgres` tanpa password (`trust`), MySQL/MariaDB memakai `root` tanpa
password, Redis tanpa auth. Semua server hanya mendengarkan di `127.0.0.1` dan
tidak ada opsi untuk mengubahnya. **Jangan pakai untuk production.**

## Manifest engine

DBnest tidak membundel binary engine. Daftar versi dan URL unduhannya datang
dari sebuah manifest JSON (DESIGN.md §5), dengan urutan: `manifest_url` di
settings → cache hasil unduhan terakhir (`$XDG_CACHE_HOME/dbnest/manifest.json`)
→ salinan bawaan di dalam binary.

Manifest bawaan sudah berisi artefak sungguhan untuk keempat engine di
x86_64 — semuanya diisi oleh [`build-engines.yml`](#membangun-binary-engine),
bukan diketik manual:

| Engine | Versi | Sumber artefak | sha256 |
|---|---|---|---|
| Redis | 7.4.0 | Releases repo ini (dibangun sendiri) | diukur saat build |
| PostgreSQL | 16.4 | Releases repo ini (dibangun sendiri) | diukur saat build |
| MariaDB | 11.4.4 | archive.mariadb.org | cocok dengan `sha256sums.txt` resmi |
| MySQL | 8.4.3 | cdn.mysql.com (arsip) | diukur dari unduhan HTTPS — lihat catatan |

**Catatan MySQL:** untuk tarball ini upstream tidak menerbitkan berkas checksum
yang bisa diambil otomatis, jadi sha256 di manifest diukur dari unduhan HTTPS
di runner dan belum dicocokkan dengan nilai resmi. MariaDB dan PostgreSQL
dicocokkan dengan checksum resmi upstream. Selisih ini disengaja dan dicatat,
bukan disamarkan.

Untuk menambah versi atau arsitektur, jalankan `build-engines.yml` lagi; untuk
memperbarui tanpa rilis ulang aplikasi, host manifestnya (GitHub
Pages/Releases) lalu isi **Manifest URL** di Settings.

Setelah manifest di-host, versi baru cukup ditambahkan di sana: aplikasi
mengambilnya saat dibuka atau lewat tombol **Refresh versions**, tanpa perlu
rilis ulang.

Semua artefak diverifikasi sha256-nya sebelum diekstrak, dan ekstraksi menolak
entri arsip berpath absolut atau mengandung `..` — termasuk untuk target hard
link.

## Membangun binary engine

`.github/workflows/build-engines.yml` (jalankan manual lewat **Run workflow**)
mengisi manifest dengan data sungguhan untuk keempat engine. Alurnya mengikuti
DESIGN.md §19.

| Engine | Cara | Hasil di manifest |
|---|---|---|
| Redis, PostgreSQL | dibangun dari source di container AlmaLinux 8 | url Releases repo ini |
| MySQL, MariaDB | tarball resmi upstream diunduh dan diverifikasi | url resmi upstream |

Keduanya bermuara di langkah yang sama: sha256 diukur di runner, lalu sebuah PR
membuka pembaruan `manifest/manifest.json`. Tidak ada nilai yang diketik
manual. Untuk MySQL dan MariaDB, tarball-nya juga dicek bentuknya
(`scripts/check-tarball-layout.sh`) supaya `strip_components: 1` di manifest
benar-benar menghasilkan `bin/` di akar.

AlmaLinux 8 dipakai karena glibc-nya 2.28 — yang tertua di antara distro yang
didukung. `scripts/check-portable.sh` menegakkan janji itu: build gagal kalau
ada binary yang menuntut glibc lebih baru, atau menaut library yang SONAME-nya
berbeda antar distro. Karena itu PostgreSQL dibangun tanpa ICU, readline, dan
OpenSSL (SONAME ketiganya berbeda antara EL8 dan Ubuntu 24.04); konsekuensinya
`psql` tidak punya line editing.

Tiap build juga di-smoke test sebelum dipaketkan: Redis dijalankan lalu
di-`PING`, dan PostgreSQL di-`initdb` serta di-query sebagai user biasa dari
direktori yang berbeda dari prefix build-nya, sekaligus membuktikan pohonnya
relokatabel.

Sumber PostgreSQL diverifikasi terhadap berkas `.sha256` resmi dari
postgresql.org, dan `scripts/verify-upstream-checksum.sh` melakukan hal yang
sama untuk MySQL dan MariaDB bila upstream menerbitkannya. Kalau tidak ada
checksum yang bisa diambil otomatis — seperti pada Redis — sha256-nya diukur
dari unduhan HTTPS di runner dan diberi peringatan di log, bukan dikarang.
Cocokkan sekali dengan halaman unduhan resmi, lalu isikan ke input
`redis_src_sha256` supaya build berikutnya menolak sumber yang berubah.

## Menguji di Debian dan turunannya

Job `integration` di `ci.yml` (juga manual) memasang engine sungguhan dari
manifest lalu menjalankannya di Ubuntu 24.04, Ubuntu 22.04, Debian 12, dan
Debian 11. Ini yang membuktikan binary hasil `build-engines.yml` benar-benar
jalan di distro target, bukan cuma terbangun.

Prasyarat: **Settings → Actions → General → "Allow GitHub Actions to create and
approve pull requests"** harus aktif supaya langkah PR manifest berhasil.

Arsitektur yang dibangun baru x86_64; aarch64 menunggu runner ARM.

## Lokasi data

Semua per user, mengikuti XDG, tanpa root:

```
$XDG_DATA_HOME/dbnest/      binaries/, instances/, compat-lib/, tmp/
$XDG_CONFIG_HOME/dbnest/    instances.json, settings.json
$XDG_CACHE_HOME/dbnest/     manifest.json, downloads/
~/.config/systemd/user/     dbnest-<id>.service (kalau backend systemd)
```

## Backend proses

`auto` (default) memakai `systemd --user` kalau tersedia, selain itu menjalankan
proses langsung dengan `setsid` supaya server tetap hidup setelah aplikasi
ditutup. Pilihannya bisa dipaksa di Settings; `dbnest doctor` menampilkan backend
yang sedang aktif.

Dengan systemd, instance ber-`autostart` dijalankan otomatis saat login lewat
unit yang di-`enable`. Agar server tetap berjalan tanpa login sama sekali:

```bash
loginctl enable-linger $USER
```

## Pengembangan

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all && cargo clippy --workspace -- -D warnings

cd ui && npm install && npm run dev   # frontend saja
cargo tauri dev                        # GUI lengkap, dari root repo

cargo run -p dbnest-cli -- list        # CLI
```

Tes integrasi mengunduh binary sungguhan dan menjalankan server, jadi ditandai
`#[ignore]`:

```bash
DBNEST_IT=1 cargo test -p dbnest-core -- --ignored
```

Sejak manifest berisi artefak sungguhan, tes ini bisa dijalankan — dan job
`integration` di CI menjalankannya di Debian sekeluarga.

### Dependensi sistem untuk membangun GUI

Ubuntu 22.04 dan 24.04:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev patchelf
```

Tauri v2 butuh webkit2gtk **4.1** (paket `-4.0-dev` hanya untuk Tauri v1).
Untuk mem-bundle AppImage, tambahkan `xdg-utils`.

### Struktur

```
crates/core/   dbnest-core — semua logika bisnis
crates/cli/    binary `dbnest`
src-tauri/     aplikasi Tauri v2 (commands/events/tray tipis di atas core)
ui/            React + TypeScript + Vite
manifest/      manifest.json yang di-embed sebagai fallback
```

Rancangan lengkap ada di [DESIGN.md](DESIGN.md).

## Yang belum selesai

- Baru x86_64. aarch64 menunggu runner ARM, dan entri arsitektur itu dihapus
  dari manifest (bukan dibiarkan `TODO`) supaya pengguna aarch64 mendapat
  "versi tidak tersedia" yang jujur, bukan "belum diverifikasi".
- sha256 MySQL 8.4.3 belum dicocokkan dengan checksum resmi upstream (lihat
  [Manifest engine](#manifest-engine)); yang lain sudah.
- MongoDB belum didukung.
- Belum ada rilis biner. Ketiga bundle (AppImage, `.deb`, `.rpm`) sudah terbukti
  bisa dibangun dan `.deb`-nya terpasang bersih lewat `dpkg -i`, tapi
  `release.yml` sendiri belum pernah jalan di sebuah tag.
- Nama paket `.deb`/`.rpm` keluar sebagai `d-bnest` — Tauri menurunkannya dari
  `productName` ("DBnest") dan belum ada opsi untuk menimpanya.
- Lisensi proyek belum ditetapkan (`Cargo.toml` menyebut MIT, tapi belum ada
  file `LICENSE`).
- Tanda tangan manifest (minisign) belum ada.
- Ikon aplikasi masih placeholder polos.
