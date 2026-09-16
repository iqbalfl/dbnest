# CLAUDE.md

## Proyek

DBnest adalah padanan DBngin untuk Linux. Aplikasi ini menjalankan server database lokal (PostgreSQL, MySQL, MariaDB, Redis, MongoDB) secara native, dengan banyak versi dan port, tanpa Docker dan tanpa root.

Rancangan lengkap ada di `DESIGN.md`. **Baca DESIGN.md sebelum mengerjakan fitur apa pun**, dan ikuti milestone di §20 secara berurutan.

## Stack

- Rust workspace:
  - `crates/core` (library, semua logika)
  - `crates/cli` (binary `dbnest`)
  - `src-tauri` (app Tauri v2)
- Frontend: `ui/` (React + TypeScript + Vite)

## Perintah

```bash
cargo build --workspace
cargo test --workspace
DBNEST_IT=1 cargo test -p dbnest-core -- --ignored   # tes integrasi (unduh binary asli)
cargo fmt --all && cargo clippy --workspace -- -D warnings
cargo run -p dbnest-cli -- list
cd ui && npm install && npm run dev
cargo tauri dev                                     # dari root repo
```

## Aturan kerja

- Semua logika bisnis ada di `crates/core`. Tauri commands dan CLI hanya pembungkus tipis.
- Jangan pernah menjalankan proses lewat shell string. Selalu gunakan `Command::new(..).args([...])`.
- Jangan memanggil `sudo` dan jangan menulis di luar folder XDG milik aplikasi dan `~/.config/systemd/user/`.
- Semua server hanya bind ke `127.0.0.1`.
- Penulisan JSON harus atomik (tmp → fsync → rename) dan dilindungi file lock.
- Jangan mengarang URL unduhan atau sha256 di `manifest/manifest.json`. Jika belum diverifikasi, beri tanda `TODO` dan beri tahu pengguna.
- Tes yang mengunduh binary atau menjalankan server wajib diberi `#[ignore]` dan memakai direktori XDG sementara (`tempfile`), jangan memakai folder home asli.
- Sebelum menyelesaikan tugas, pastikan `cargo fmt`, `clippy -D warnings`, dan `cargo test` lulus.
- Tipe di `ui/src/types.ts` harus sinkron dengan `crates/core/src/model.rs`.

## Jebakan yang sudah diketahui

- MySQL/MariaDB: `--no-defaults` harus argumen **pertama**. Tambahkan `--mysqlx=OFF` untuk MySQL.
- PostgreSQL: gunakan `-k <run_dir>` untuk socket dan stop dengan `SIGINT`.
- MySQL di Ubuntu 24.04 butuh symlink `libaio.so.1` → `libaio.so.1t64` di `compat-lib/` (lihat DESIGN §8.2).
- Path socket Unix maksimal sekitar 108 byte.
- Unit systemd: quote argumen `ExecStart` dengan benar, lalu jalankan `daemon-reload` setelah menulis unit.
