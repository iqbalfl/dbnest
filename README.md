# DBnest

Server database lokal untuk Linux — pilih engine, pilih versi, isi port, lalu
Start. Berjalan native tanpa Docker, tanpa VM, dan tanpa root.

DBnest adalah padanan [DBngin](https://dbngin.com/) untuk Linux. Nama dan aset
DBngin/TablePlus tidak dipakai di proyek ini.

> **Status: belum siap dipakai sehari-hari.** Aplikasi, CLI, dan GUI-nya sudah
> jalan, tapi manifest bawaan belum berisi URL unduhan engine yang terverifikasi,
> jadi instalasi engine otomatis belum bisa berjalan di luar kotak. Lihat
> [Manifest engine](#manifest-engine) di bawah.

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

**Manifest bawaan saat ini belum berisi artefak yang terverifikasi.** Semua
entri ditandai `"verified": false` dengan `url` dan `sha256` berisi `"TODO"`,
karena URL dan checksum tidak boleh dikarang. Akibatnya `dbnest start` akan
menolak memasang engine dengan pesan "belum diverifikasi".

Supaya instalasi otomatis berjalan, seseorang perlu:

1. Menyiapkan tarball engine (lihat §5.2 DESIGN.md untuk sumber tiap engine).
   MySQL dan MariaDB punya tarball resmi yang tinggal dipakai. PostgreSQL dan
   Redis tidak, jadi keduanya dibangun lewat workflow
   [`build-engines.yml`](#membangun-binary-engine) di bawah.
2. Menghitung sha256-nya, mengisi `manifest/manifest.json`, dan mengubah
   `"verified"` jadi `true`. `build-engines.yml` melakukan ini lewat PR.
3. Meng-host manifest itu (GitHub Pages/Releases) lalu mengisi **Manifest URL**
   di Settings — atau cukup memperbarui manifest bawaan lalu build ulang.

Setelah manifest di-host, versi baru cukup ditambahkan di sana: aplikasi
mengambilnya saat dibuka atau lewat tombol **Refresh versions**, tanpa perlu
rilis ulang.

Semua artefak diverifikasi sha256-nya sebelum diekstrak, dan ekstraksi menolak
entri arsip berpath absolut atau mengandung `..` — termasuk untuk target hard
link.

## Membangun binary engine

`.github/workflows/build-engines.yml` (jalankan manual lewat **Run workflow**)
membangun Redis dan PostgreSQL di container AlmaLinux 8, mengunggah tarball-nya
ke Releases repo ini, lalu membuka PR yang mengisi `manifest/manifest.json`
dengan url dan sha256 hasil build. Alurnya mengikuti DESIGN.md §19.

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
postgresql.org. Upstream Redis tidak menerbitkan berkas checksum yang bisa
diambil otomatis, jadi sha256 tarball sumbernya hanya dicatat di log — cocokkan
sekali dengan halaman unduhan Redis, lalu isikan ke input `redis_src_sha256`
supaya build berikutnya menolak sumber yang berubah.

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

Tes ini baru bisa lulus setelah manifest berisi artefak terverifikasi.

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

- Manifest belum berisi artefak terverifikasi (lihat di atas) — ini yang
  menghalangi instalasi engine otomatis dan tes integrasi. `build-engines.yml`
  sudah ada untuk mengisinya, tapi belum pernah dijalankan.
- `build-engines.yml` baru membangun Redis dan PostgreSQL untuk x86_64. MySQL
  dan MariaDB memakai tarball resmi, jadi url dan sha256-nya masih perlu diisi
  dengan tangan.
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
