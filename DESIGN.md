# DBnest — Dokumen Rancangan Teknis

> Nama "DBnest" adalah nama kerja (codename) dan bisa diganti. Jangan memakai nama atau logo "DBngin", karena itu merek milik TablePlus.

| | |
|---|---|
| Status | Draft v0.1 |
| Stack | Rust (core) + Tauri v2 (desktop) + TypeScript (UI) |
| Target | Linux x86_64 dan aarch64 |
| Lisensi proyek | Ditentukan kemudian (saran: MIT atau Apache-2.0) |

---

## 1. Latar belakang dan tujuan

DBngin (buatan TablePlus) adalah aplikasi gratis untuk macOS dan Windows yang menjalankan server database lokal (PostgreSQL, MySQL, MariaDB, Redis, MongoDB) secara native, tanpa Docker atau VM. Aplikasi ini mendukung banyak versi dan banyak port sekaligus. Versi Linux-nya belum ada.

DBnest adalah padanan DBngin untuk Linux.

### 1.1 Tujuan (goals)

- Membuat server database lokal dengan beberapa klik: pilih engine, pilih versi, isi nama dan port, lalu Start.
- Menjalankan banyak versi dan banyak instance secara bersamaan di port yang berbeda.
- Berjalan native tanpa Docker, VM, maupun hak root.
- Mendukung engine berikut:
  - MVP: PostgreSQL, Redis, MySQL, MariaDB.
  - Tahap berikutnya: MongoDB.
- Autostart instance saat login.
- Tombol "Open Terminal" dengan PATH yang sudah mengarah ke versi yang benar.
- Tombol untuk menyalin connection string.
- Ikon tray untuk start/stop cepat.
- CLI (`dbnest`) dengan kemampuan yang setara dengan GUI.

### 1.2 Di luar cakupan (non-goals)

- Menjadi client atau editor database. Tugas ini diserahkan ke TablePlus, DBeaver, psql, dan sejenisnya.
- Server untuk production, akses jaringan publik, replikasi, atau clustering.
- Dukungan macOS dan Windows (DBngin sudah tersedia di sana).
- Packaging Flatpak atau Snap di fase awal, karena sandbox-nya menyulitkan akses ke systemd dan binary.

### 1.3 Distro minimum

Distro harus berbasis glibc ≥ 2.28. Contohnya:
- Ubuntu 20.04+
- Debian 11+
- Fedora 36+
- RHEL/Rocky 8+
- Arch

Distro berbasis musl (Alpine) tidak didukung.

---

## 2. Arsitektur

```
┌──────────────────────┐   ┌───────────────────┐
│  Tauri app (GUI)     │   │  dbnest (CLI)     │
│  ui/ + src-tauri/    │   │  crates/cli       │
└──────────┬───────────┘   └─────────┬─────────┘
           │   memanggil library yang sama      │
           ▼                                    ▼
┌──────────────────────────────────────────────────┐
│ crates/core  (dbnest-core)                       │
│  config · manifest · downloader · installer      │
│  engines (adapter per engine) · ports · preflight │
│  process backend (systemd --user | direct)       │
└──────────┬───────────────────────────┬───────────┘
           ▼                           ▼
   systemctl --user / proses      ~/.local/share/dbnest
   (mysqld, postgres, redis…)     ~/.config/dbnest
```

Prinsip utamanya adalah **semua logika ada di `dbnest-core`**. GUI dan CLI hanya lapisan tipis di atasnya. Akibatnya, semua fitur bisa dites lewat CLI tanpa membuka GUI.

### 2.1 Struktur repository

```
dbnest/
├── Cargo.toml                 # workspace
├── CLAUDE.md
├── DESIGN.md                  # dokumen ini
├── crates/
│   ├── core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── error.rs
│   │       ├── paths.rs        # XDG paths
│   │       ├── config.rs       # instances.json, settings.json
│   │       ├── model.rs        # Instance, EngineKind, Status, dst.
│   │       ├── manifest.rs     # fetch + parse + cache manifest
│   │       ├── download.rs     # streaming download + sha256
│   │       ├── install.rs      # extract + atomic install binary
│   │       ├── preflight.rs    # cek library (ldd), cek root, cek systemd
│   │       ├── ports.rs
│   │       ├── terminal.rs     # deteksi & buka emulator terminal
│   │       ├── manager.rs      # API tingkat tinggi (facade)
│   │       ├── engines/
│   │       │   ├── mod.rs      # trait EngineAdapter + registry
│   │       │   ├── postgres.rs
│   │       │   ├── mysql.rs
│   │       │   ├── mariadb.rs
│   │       │   ├── redis.rs
│   │       │   └── mongodb.rs
│   │       └── process/
│   │           ├── mod.rs      # trait ProcessBackend
│   │           ├── systemd.rs
│   │           └── direct.rs
│   └── cli/
│       ├── Cargo.toml
│       └── src/main.rs
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/default.json
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── commands.rs         # #[tauri::command]
│       ├── events.rs
│       └── tray.rs
├── ui/                         # React + TypeScript + Vite
│   ├── package.json
│   └── src/
│       ├── App.tsx
│       ├── api.ts              # wrapper invoke() bertipe
│       ├── types.ts            # mirror dari model Rust
│       └── components/
├── manifest/
│   └── manifest.json           # sumber manifest versi (di-host via GitHub Pages/Releases)
└── .github/workflows/
    ├── ci.yml
    ├── release.yml             # bundle AppImage/deb/rpm
    └── build-engines.yml       # compile Redis & PostgreSQL → GitHub Releases
```

### 2.2 Crate yang direkomendasikan

| Kebutuhan | Crate |
|---|---|
| Serialisasi | `serde`, `serde_json` |
| Error | `thiserror` (core), `anyhow` (cli) |
| Async runtime | `tokio` (sudah dipakai Tauri) |
| HTTP | `reqwest` (fitur `stream`, `rustls-tls`) |
| Hash | `sha2`, `hex` |
| Ekstrak arsip | `tar`, `flate2`, `xz2` (atau `liblzma`), `zstd` bila perlu |
| Path XDG | `directories` |
| ID | `uuid` (v4) atau `nanoid` |
| Waktu | `time` atau `chrono` |
| Logging | `tracing`, `tracing-subscriber` |
| Sinyal proses | `nix` (fitur `signal`, `process`) |
| CLI | `clap` (derive) |
| File lock | `fs4` atau `fd-lock` |
| Info OS | parse `/etc/os-release` secara manual (sederhana) |
| Tauri plugin | `tauri-plugin-single-instance`, `tauri-plugin-opener`, `tauri-plugin-autostart` (untuk app itu sendiri) |

Opsional untuk fase lanjut: `zbus`, untuk berbicara langsung ke systemd lewat D-Bus alih-alih memanggil `systemctl`.

---

## 3. Lokasi penyimpanan

Semua data disimpan per user, tanpa root, mengikuti XDG Base Directory.

```
$XDG_DATA_HOME/dbnest/            (default ~/.local/share/dbnest)
├── binaries/
│   └── <engine>/<version>/       # isi tarball (bin/, lib/, share/ …)
│       └── .installed            # penanda instalasi selesai + sha256
├── compat-lib/                   # symlink kompatibilitas (lihat §8.2)
├── instances/
│   └── <instance-id>/
│       ├── data/                 # data directory engine
│       ├── run/                  # socket, pid
│       ├── logs/engine.log
│       └── conf/                 # file config hasil generate (redis.conf, dll.)
└── tmp/                          # download & ekstraksi sementara

$XDG_CONFIG_HOME/dbnest/          (default ~/.config/dbnest)
├── instances.json
└── settings.json

$XDG_CACHE_HOME/dbnest/           (default ~/.cache/dbnest)
├── manifest.json                 # cache manifest terakhir
└── downloads/                    # arsip terunduh (boleh dihapus)

~/.config/systemd/user/dbnest-<instance-id>.service
```

Aturan tambahan:
- Izin folder `instances/<id>/` diset `0700`.
- Path socket Unix dibatasi sekitar 108 byte. Jika `run/` terlalu panjang, fallback ke `$XDG_RUNTIME_DIR/dbnest/<id>/`.
- Semua penulisan file JSON bersifat **atomik**: tulis ke `*.tmp`, `fsync`, lalu `rename`.
- Akses ke `instances.json` dilindungi file lock, karena GUI dan CLI bisa berjalan bersamaan.

---

## 4. Model data

### 4.1 Tipe inti (Rust)

```rust
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind { Postgres, Mysql, Mariadb, Redis, Mongodb }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Instance {
    pub id: String,              // "pg-7f3a2c" (prefix engine + 6 hex)
    pub name: String,            // label bebas, unik (case-insensitive)
    pub engine: EngineKind,
    pub version: String,         // "16.4"
    pub port: u16,
    pub autostart: bool,
    pub created_at: String,      // RFC 3339
    #[serde(default)]
    pub extra_args: Vec<String>, // argumen tambahan tingkat lanjut
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum InstanceStatus {
    NotInitialized,
    Stopped,
    Starting,
    Running { pid: Option<u32> },
    Stopping,
    Failed { message: String },
}

#[derive(Serialize, Clone, Debug)]
pub struct ConnectionInfo {
    pub host: String,            // "127.0.0.1"
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,// None = tanpa password
    pub socket: Option<PathBuf>,
    pub url: String,             // "postgresql://postgres@127.0.0.1:5432/postgres"
}
```

### 4.2 `instances.json`

```json
{
  "schema_version": 1,
  "instances": [
    {
      "id": "pg-7f3a2c",
      "name": "Postgres 16",
      "engine": "postgres",
      "version": "16.4",
      "port": 5432,
      "autostart": true,
      "created_at": "2026-09-16T10:00:00Z",
      "extra_args": []
    }
  ]
}
```

### 4.3 `settings.json`

```json
{
  "schema_version": 1,
  "manifest_url": "https://<user>.github.io/dbnest/manifest.json",
  "process_backend": "auto",
  "terminal_command": null,
  "start_minimized_to_tray": false
}
```

Nilai `process_backend` bisa `"auto"`, `"systemd"`, atau `"direct"`.

Setiap migrasi skema ditangani di `config.rs` berdasarkan `schema_version`.

---

## 5. Manifest versi

Manifest adalah daftar engine, versi, dan URL unduhan yang di-host terpisah dari aplikasi. Dengan cara ini, versi baru bisa ditambahkan tanpa merilis ulang aplikasi. Salinan `manifest/manifest.json` juga di-embed ke dalam binary (`include_str!`) sebagai fallback offline.

### 5.1 Format

```json
{
  "schema_version": 1,
  "generated_at": "2026-09-16T00:00:00Z",
  "engines": {
    "postgres": {
      "display_name": "PostgreSQL",
      "default_port": 5432,
      "versions": [
        {
          "version": "16.4",
          "channel": "stable",
          "artifacts": {
            "x86_64": {
              "url": "https://github.com/<org>/dbnest-engines/releases/download/postgres-16.4/postgres-16.4-x86_64-linux-gnu.tar.gz",
              "sha256": "…",
              "format": "tar.gz",
              "strip_components": 1,
              "min_glibc": "2.28"
            },
            "aarch64": { "…": "…" }
          },
          "variants_by_distro": null
        }
      ]
    },
    "mongodb": {
      "display_name": "MongoDB",
      "default_port": 27017,
      "versions": [
        {
          "version": "7.0.14",
          "channel": "stable",
          "artifacts": null,
          "variants_by_distro": [
            { "match": { "id": ["ubuntu"], "version_id_min": "22.04" },
              "x86_64": { "url": "…ubuntu2204…", "sha256": "…", "format": "tar.gz", "strip_components": 1 } },
            { "match": { "id": ["debian"], "version_id_min": "12" },
              "x86_64": { "url": "…debian12…", "sha256": "…", "format": "tar.gz", "strip_components": 1 } }
          ]
        }
      ]
    }
  }
}
```

### 5.2 Sumber artefak per engine

| Engine | Sumber | Catatan |
|---|---|---|
| MySQL | Tarball resmi "Linux – Generic (glibc 2.28)" dari dev.mysql.com | Butuh `libaio` dan `libncurses` dari sistem (§8) |
| MariaDB | Tarball resmi "bintar" dari mariadb.org | Pilih varian `linux-systemd-x86_64` / `aarch64` |
| PostgreSQL | Build portabel pihak ketiga (mis. `theseus-rs/postgresql-binaries`) **atau** build sendiri via `build-engines.yml` | Verifikasi dulu ketersediaan arsitektur dan lisensinya |
| Redis | Build sendiri dari source (`make BUILD_TLS=no`) via CI | Pertimbangkan Valkey sebagai opsi tambahan |
| MongoDB | Tarball resmi per distro dari fastdl.mongodb.org | Dipilih berdasarkan `/etc/os-release` |

Semua URL dan sha256 harus diisi dan diverifikasi saat manifest dibuat. **Jangan mengarang checksum.**

### 5.3 Aturan

- Aplikasi mengambil `manifest_url` dengan timeout 10 detik. Jika gagal, pakai cache. Jika cache juga tidak ada, pakai manifest yang di-embed.
- Artefak dipilih berdasarkan `std::env::consts::ARCH` (`x86_64` / `aarch64`) dan, bila diperlukan, `/etc/os-release`.
- Semua artefak **wajib** diverifikasi sha256-nya sebelum diekstrak.
- Tanda tangan manifest (minisign) menyusul di fase lanjut.

---

## 6. Instalasi binary

Alurnya adalah sebagai berikut:

1. Acquire lock `binaries/<engine>/.lock`.
2. Jika `binaries/<engine>/<version>/.installed` sudah ada dan sha256-nya cocok, instalasi selesai.
3. Download (streaming) ke `cache/downloads/<file>.part`, sambil menghitung sha256 dan mengirim event progress.
4. Bandingkan sha256. Jika tidak cocok, hapus file lalu kembalikan `Error::ChecksumMismatch`.
5. Ekstrak ke `tmp/<random>/`, dengan `strip_components` diterapkan.
6. Tolak entri arsip yang berupa path absolut atau mengandung `..` (mencegah path traversal).
7. Jalankan hook `adapter.post_install(dir)` (mis. menyesuaikan permission).
8. `rename(tmp/<random>, binaries/<engine>/<version>)`.
9. Tulis `.installed` berisi sha256 dan waktu instalasi, lalu release lock.

Event progress yang dikirim ke UI:

```rust
pub enum InstallEvent {
    Downloading { downloaded: u64, total: Option<u64> },
    Verifying,
    Extracting,
    Done,
    Failed { message: String },
}
```

Uninstall versi hanya diizinkan jika tidak ada instance yang memakai versi tersebut.

---

## 7. Engine adapter

### 7.1 Trait

```rust
pub struct InstanceCtx<'a> {
    pub instance: &'a Instance,
    pub bin_dir: PathBuf,      // binaries/<engine>/<version>
    pub data_dir: PathBuf,     // instances/<id>/data
    pub run_dir: PathBuf,      // socket & pid
    pub log_file: PathBuf,     // instances/<id>/logs/engine.log
    pub conf_dir: PathBuf,
    pub lib_path: Vec<PathBuf>,// untuk LD_LIBRARY_PATH
}

pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub working_dir: PathBuf,
    pub stop_signal: StopSignal,    // Term | Int
    pub stop_timeout: Duration,
}

#[async_trait::async_trait]
pub trait EngineAdapter: Send + Sync {
    fn kind(&self) -> EngineKind;
    fn default_port(&self) -> u16;
    /// Nama executable yang ditambahkan ke PATH saat "Open Terminal".
    fn client_bin_dirs(&self, bin_dir: &Path) -> Vec<PathBuf>;
    fn main_binary(&self, bin_dir: &Path) -> PathBuf;

    fn is_initialized(&self, ctx: &InstanceCtx) -> bool;
    async fn init(&self, ctx: &InstanceCtx) -> Result<()>;
    fn launch_spec(&self, ctx: &InstanceCtx) -> Result<LaunchSpec>;
    async fn health_check(&self, ctx: &InstanceCtx) -> Result<bool>;
    fn connection_info(&self, ctx: &InstanceCtx) -> ConnectionInfo;

    /// Dipanggil setelah binary diekstrak.
    fn post_install(&self, _bin_dir: &Path) -> Result<()> { Ok(()) }
}
```

Registry-nya berupa fungsi `fn adapter(kind: EngineKind) -> &'static dyn EngineAdapter`.

Ada dua aturan untuk alur init dan start:
- `init()` dijalankan **sekali**, yaitu saat instance pertama kali di-start (bukan saat dibuat). Dengan begitu, membuat instance tetap cepat.
- `init()` harus idempoten. Jika gagal di tengah jalan, `data_dir` dihapus dan status diset `NotInitialized`.

### 7.2 Spesifikasi per engine

Singkatan yang dipakai di bawah: `B` = bin_dir, `D` = data_dir, `R` = run_dir, `L` = log_file, `C` = conf_dir, `P` = port.

#### PostgreSQL

| | |
|---|---|
| Init | `B/bin/initdb -D D -U postgres --auth=trust --encoding=UTF8 --locale=C.UTF-8` (fallback `--no-locale` bila locale tidak tersedia) |
| Start | `B/bin/postgres -D D -p P -k R -c listen_addresses=127.0.0.1 -c logging_collector=off` |
| Stop | `SIGINT` (fast shutdown), timeout 30 detik |
| Health | `B/bin/pg_isready -h 127.0.0.1 -p P` exit 0, atau koneksi TCP berhasil |
| Kredensial | user `postgres`, tanpa password (trust) |
| URL | `postgresql://postgres@127.0.0.1:P/postgres` |
| Env | `LD_LIBRARY_PATH=B/lib` |
| Catatan | Postgres menolak berjalan sebagai root. `SIGTERM` = smart shutdown (menunggu klien), jadi gunakan `SIGINT`. |

#### MySQL

| | |
|---|---|
| Init | `B/bin/mysqld --no-defaults --initialize-insecure --basedir=B --datadir=D` |
| Start | `B/bin/mysqld --no-defaults --basedir=B --datadir=D --port=P --bind-address=127.0.0.1 --socket=R/mysql.sock --pid-file=R/mysqld.pid --mysqlx=OFF --log-error=L` |
| Stop | `SIGTERM`, timeout 60 detik |
| Health | koneksi TCP berhasil dan `B/bin/mysqladmin --no-defaults -h 127.0.0.1 -P P -u root ping` exit 0 |
| Kredensial | `root`, tanpa password (sama seperti DBngin) |
| URL | `mysql://root@127.0.0.1:P` |
| Env | `LD_LIBRARY_PATH=<compat-lib>:B/lib/private` |
| Catatan | `--no-defaults` **harus argumen pertama**, agar `/etc/mysql/my.cnf` tidak terbaca. `--mysqlx=OFF` mencegah bentrok port 33060 antar instance. `mysqld` menolak berjalan sebagai root tanpa `--user`. |

#### MariaDB

| | |
|---|---|
| Init | `B/scripts/mariadb-install-db --no-defaults --basedir=B --datadir=D --auth-root-authentication-method=normal --skip-test-db` |
| Start | `B/bin/mariadbd --no-defaults --basedir=B --datadir=D --port=P --bind-address=127.0.0.1 --socket=R/mysql.sock --pid-file=R/mariadbd.pid --log-error=L` |
| Stop | `SIGTERM`, timeout 60 detik |
| Health | `B/bin/mariadb-admin --no-defaults -h 127.0.0.1 -P P -u root ping` |
| Kredensial | `root`, tanpa password |
| URL | `mysql://root@127.0.0.1:P` |
| Catatan | Versi lama memakai nama `mysqld` / `mysql_install_db`. Adapter harus mencari kedua nama tersebut. |

#### Redis

| | |
|---|---|
| Init | Tulis `C/redis.conf` (lihat di bawah) dan buat `D` |
| Start | `B/bin/redis-server C/redis.conf` |
| Stop | `SIGTERM` (Redis menyimpan RDB lalu keluar), timeout 30 detik |
| Health | Kirim `PING\r\n` via TCP, balasan harus `+PONG` |
| Kredensial | tanpa auth |
| URL | `redis://127.0.0.1:P` |

Isi `C/redis.conf`:

```
port P
bind 127.0.0.1
protected-mode yes
dir D
daemonize no
logfile ""
save 3600 1 300 100 60 10000
```

Nilai `logfile ""` membuat log keluar ke stdout, yang kemudian diarahkan ke `L` oleh backend.

#### MongoDB (fase 5)

| | |
|---|---|
| Init | buat `D` |
| Start | `B/bin/mongod --dbpath D --port P --bind_ip 127.0.0.1 --unixSocketPrefix R` |
| Stop | `SIGTERM`, timeout 60 detik |
| Health | koneksi TCP berhasil (tarball server tidak menyertakan `mongosh`) |
| URL | `mongodb://127.0.0.1:P` |
| Catatan | Tarball berbeda per distro, lihat `variants_by_distro` di manifest. |

---

## 8. Preflight dan dependensi sistem

### 8.1 Pemeriksaan sebelum start

`preflight::check(instance)` menghasilkan daftar `Issue { severity, message, fix_hint }`. Pemeriksaannya:

1. **Proses berjalan sebagai root.** Jika ya, tolak dengan pesan yang jelas.
2. **Binary sudah terpasang.** Cek penanda `.installed`.
3. **Library tersedia.** Jalankan `ldd <main_binary>` dengan `LD_LIBRARY_PATH` yang akan dipakai, lalu kumpulkan baris `=> not found`.
4. **Port tersedia** (§10).
5. **Panjang path socket** ≤ 100 byte.
6. **Ruang disk** di `$XDG_DATA_HOME`. Beri peringatan jika < 1 GB.

### 8.2 Pemetaan library yang hilang

Nama library yang hilang dipetakan ke nama paket berdasarkan `ID` / `ID_LIKE` di `/etc/os-release`:

| Library | Debian/Ubuntu | Fedora/RHEL | Arch |
|---|---|---|---|
| `libaio.so.1` | `libaio1` (≤ 23.10) / `libaio1t64` (≥ 24.04) | `libaio` | `libaio` |
| `libncurses.so.6` / `libtinfo.so.6` | `libncurses6` | `ncurses-libs` | `ncurses` |
| `libnuma.so.1` | `libnuma1` | `numactl-libs` | `numactl` |
| `libssl.so.3` | `libssl3` / `libssl3t64` | `openssl-libs` | `openssl` |

UI menampilkan perintah instalasi siap salin, misalnya `sudo apt install libaio1t64`. Aplikasi **tidak pernah** menjalankan sudo sendiri.

Ada satu kasus khusus: Ubuntu 24.04+ hanya menyediakan `libaio.so.1t64`. Jika file itu ada tetapi `libaio.so.1` tidak ada, buat symlink **tanpa sudo** di folder milik aplikasi:

```
$XDG_DATA_HOME/dbnest/compat-lib/libaio.so.1 -> /usr/lib/<triplet>/libaio.so.1t64
```

Lalu tambahkan `compat-lib/` ke `LD_LIBRARY_PATH` MySQL. Triplet ditentukan dengan mencari di `/usr/lib/x86_64-linux-gnu` dan `/usr/lib/aarch64-linux-gnu`.

---

## 9. Process backend

```rust
#[async_trait::async_trait]
pub trait ProcessBackend: Send + Sync {
    async fn start(&self, id: &str, spec: &LaunchSpec, log_file: &Path) -> Result<()>;
    async fn stop(&self, id: &str, spec: &LaunchSpec) -> Result<()>;
    async fn status(&self, id: &str) -> Result<ProcState>; // Running{pid}|Stopped|Failed
    async fn set_autostart(&self, id: &str, spec: &LaunchSpec, log_file: &Path, on: bool) -> Result<()>;
    async fn remove(&self, id: &str) -> Result<()>;
}
```

Backend dipilih dengan aturan `auto`: pakai `SystemdUserBackend` jika perintah `systemctl --user is-system-running` bisa dijalankan dan hasilnya bukan `offline` atau error koneksi. Jika tidak, pakai `DirectBackend`.

### 9.1 SystemdUserBackend (utama)

File unit ditulis ke `~/.config/systemd/user/dbnest-<id>.service`:

```ini
[Unit]
Description=DBnest: {name} ({engine} {version})

[Service]
Type=simple
WorkingDirectory={working_dir}
Environment="LD_LIBRARY_PATH={lib_path}"
ExecStart={program} {args...}
KillSignal={SIGTERM|SIGINT}
TimeoutStopSec={stop_timeout}
Restart=on-failure
RestartSec=3
StandardOutput=append:{log_file}
StandardError=append:{log_file}

[Install]
WantedBy=default.target
```

Setiap argumen di `ExecStart` harus di-quote sesuai aturan systemd. Implementasikan fungsi `systemd_escape_arg()` beserta unit test-nya, karena path bisa mengandung spasi.

Operasi yang dijalankan:

| Operasi | Perintah |
|---|---|
| Tulis/ubah unit | tulis file lalu `systemctl --user daemon-reload` |
| start | `systemctl --user start dbnest-<id>.service` |
| stop | `systemctl --user stop dbnest-<id>.service` |
| status | `systemctl --user show dbnest-<id>.service -p ActiveState,SubState,MainPID,Result` |
| autostart on | `systemctl --user enable dbnest-<id>.service` |
| autostart off | `systemctl --user disable dbnest-<id>.service` |
| remove | stop, disable, hapus file, lalu `daemon-reload` |

Pemetaan status: `active` → Running, `activating` → Starting, `deactivating` → Stopping, `failed` → Failed, selain itu → Stopped.

Catatan:
- Jangan menambahkan `After=default.target`. Karena unit ini di-`WantedBy=default.target`, target tersebut otomatis sudah diurutkan setelah unit, sehingga baris itu akan menimbulkan ordering cycle.
- `StandardOutput=append:` membutuhkan systemd ≥ 240. Untuk versi lebih lama, fallback ke journald dan baca log dengan `journalctl --user -u`.
- Autostart di `default.target` berjalan saat **user login**. Agar server berjalan saat boot tanpa login, pengguna perlu `loginctl enable-linger $USER`. Cukup tampilkan sebagai tips di UI.

### 9.2 DirectBackend (fallback)

- Spawn proses dengan `setsid` (fungsi `pre_exec` + `nix::unistd::setsid`) dan stdout/stderr diarahkan ke `log_file`, supaya server tetap hidup walau aplikasi ditutup.
- PID disimpan di `instances/<id>/run/dbnest.pid`.
- Status: PID dianggap valid jika `/proc/<pid>` ada **dan** `/proc/<pid>/exe` menunjuk ke `spec.program`. Ini mencegah salah membaca PID yang sudah dipakai proses lain.
- Stop: kirim `stop_signal`, tunggu sampai `stop_timeout`, lalu `SIGKILL` sebagai jalan terakhir (dengan peringatan).
- Autostart: saat aplikasi atau tray dibuka (lewat `tauri-plugin-autostart` untuk app itu sendiri), mulai semua instance yang `autostart = true`.

### 9.3 Alur `start(instance)` di manager

1. Preflight. Jika ada issue berkategori error, hentikan.
2. Jika binary belum ada, jalankan install (§6).
3. Jika belum diinisialisasi, jalankan `adapter.init()`.
4. Buat `LaunchSpec`, lalu panggil `backend.start()`.
5. Polling `health_check` setiap 300 ms, maksimal 30 detik (MySQL pertama kali bisa lebih lama, gunakan 60 detik).
6. Jika sukses, status menjadi Running. Jika timeout, status menjadi Failed dan pesannya diisi 20 baris terakhir `log_file`.

---

## 10. Manajemen port

- `is_port_free(p)`: coba `TcpListener::bind(("127.0.0.1", p))`, lalu langsung drop.
- `suggest_port(engine)`: mulai dari `default_port`. Lewati port yang dipakai instance lain di config (walaupun sedang berhenti), dan port yang sedang terpakai. Naikkan satu per satu sampai +100.
- Saat membuat atau mengubah instance: tolak port < 1024 dan port yang dipakai instance lain.
- Saat start: jika port sedang terpakai proses lain, status menjadi Failed dengan pesan yang menyarankan port kosong.

---

## 11. Integrasi terminal dan client

### 11.1 Open Terminal

Terminal dicari dengan urutan berikut:
1. `settings.terminal_command`
2. `$TERMINAL`
3. `x-terminal-emulator`
4. `gnome-terminal`, `ptyxis`, `konsole`, `xfce4-terminal`, `kitty`, `alacritty`, `wezterm`, `xterm`

Terminal dijalankan dengan environment berikut:
- `PATH=<client_bin_dirs>:$PATH`
- `LD_LIBRARY_PATH` sesuai engine
- Variabel koneksi per engine:
  - Postgres: `PGHOST=127.0.0.1`, `PGPORT`, `PGUSER=postgres`
  - MySQL/MariaDB: `MYSQL_HOST=127.0.0.1`, `MYSQL_TCP_PORT`
  - Redis: tidak ada variabel env untuk port, jadi tampilkan petunjuk `redis-cli -p P` di awal sesi

Kalau dijalankan dari terminal ini, `psql` atau `mysql -u root` langsung terhubung ke instance yang benar.

Argumen untuk menjalankan shell berbeda di tiap emulator (`--`, `-e`, `-x`). Simpan tabel argumen per emulator di `terminal.rs`.

### 11.2 Aksi lain

- **Copy connection URL**: menyalin `ConnectionInfo.url`.
- **Open in client**: menjalankan `xdg-open <url>` lewat `tauri-plugin-opener`. Berguna jika pengguna punya handler URL, misalnya TablePlus atau DBeaver.
- **Open data folder**: menjalankan `xdg-open instances/<id>/`.

---

## 12. API manager (facade `dbnest-core`)

```rust
impl Manager {
    pub async fn new() -> Result<Self>;
    pub async fn list_engines(&self) -> Result<Vec<EngineCatalog>>;   // dari manifest
    pub async fn list_instances(&self) -> Result<Vec<InstanceView>>;  // Instance + Status + ConnectionInfo
    pub async fn create_instance(&self, req: CreateInstance) -> Result<Instance>;
    pub async fn update_instance(&self, id: &str, patch: UpdateInstance) -> Result<Instance>; // nama, port, autostart
    pub async fn delete_instance(&self, id: &str, delete_data: bool) -> Result<()>;
    pub async fn start(&self, id: &str, progress: impl Fn(ProgressEvent)) -> Result<()>;
    pub async fn stop(&self, id: &str) -> Result<()>;
    pub async fn restart(&self, id: &str) -> Result<()>;
    pub async fn status(&self, id: &str) -> Result<InstanceStatus>;
    pub async fn tail_logs(&self, id: &str, lines: usize) -> Result<Vec<String>>;
    pub async fn preflight(&self, id: &str) -> Result<Vec<Issue>>;
    pub async fn open_terminal(&self, id: &str) -> Result<()>;
    pub async fn installed_versions(&self) -> Result<Vec<InstalledVersion>>;
    pub async fn uninstall_version(&self, engine: EngineKind, version: &str) -> Result<()>;
    pub async fn suggest_port(&self, engine: EngineKind) -> Result<u16>;
    pub async fn autostart_all(&self) -> Result<()>;  // dipakai DirectBackend
}
```

Ada dua perilaku penting:
- **Ubah port**: harus stop, update config, tulis ulang unit systemd, lalu start lagi jika sebelumnya sedang berjalan.
- **Hapus instance**: stop, remove unit, lalu hapus `instances/<id>` jika `delete_data=true`. UI wajib meminta konfirmasi dengan mengetik nama instance.

---

## 13. Lapisan Tauri

### 13.1 Commands

Setiap command adalah pembungkus tipis untuk method `Manager`. `Manager` disimpan di `tauri::State<Arc<Manager>>`.

| Command | Keterangan |
|---|---|
| `list_engines` | katalog engine dan versi |
| `list_instances` | |
| `create_instance` | |
| `update_instance` | |
| `delete_instance` | |
| `start_instance` | berjalan async, progress dikirim lewat event |
| `stop_instance` | |
| `restart_instance` | |
| `get_logs` | |
| `run_preflight` | |
| `open_terminal` | |
| `copy_connection_url` | lakukan di frontend dengan clipboard API, atau dengan plugin clipboard |
| `open_data_folder` | |
| `suggest_port` | |
| `list_installed_versions` | |
| `uninstall_version` | |
| `get_settings` / `update_settings` | |

Error dikirim ke frontend sebagai `{ code: string, message: string, hint?: string }`.

### 13.2 Events (backend → frontend)

| Event | Payload |
|---|---|
| `instance://status` | `{ id, status }` |
| `instance://progress` | `{ id, event: ProgressEvent }` |
| `instances://changed` | `null`, artinya frontend perlu refetch |

Status diperbarui lewat **polling ringan di backend** setiap 2 detik untuk semua instance. Event hanya dikirim jika status berubah. Ini menangkap perubahan dari luar, misalnya server di-stop lewat CLI atau crash.

### 13.3 Tray

Menu tray berisi:
- Satu item per instance (● hijau / ○ abu) → submenu Start/Stop, Copy URL, Open Terminal
- Pemisah
- Show Window
- Quit

Menutup jendela hanya menyembunyikannya ke tray. **Quit tidak menghentikan server** jika backend-nya systemd. Pada DirectBackend, tampilkan dialog "Hentikan semua server?".

Catatan: GNOME membutuhkan ekstensi AppIndicator agar ikon tray tampil. Jika tray tidak tersedia, aplikasi berjalan sebagai jendela biasa.

### 13.4 Keamanan Tauri

- `capabilities/default.json` hanya mengizinkan command milik aplikasi, event, dan `opener` terbatas pada skema `http(s)`, `postgresql`, `mysql`, `redis`, `mongodb`, dan `file` (khusus folder data).
- Tidak ada akses shell umum dari frontend. Semua eksekusi proses terjadi di Rust.
- CSP ketat, tanpa remote content.
- Pasang `tauri-plugin-single-instance` agar hanya satu jendela aplikasi yang aktif.

---

## 14. Antarmuka pengguna

### 14.1 Layar utama

- Header: judul, tombol **+ New Server**, dan ikon Settings.
- Daftar instance berupa kartu atau baris. Tiap item berisi:
  - Ikon engine, nama, `engine versi`, dan `:port`
  - Indikator status
  - Tombol Start/Stop
  - Menu ⋯ berisi Copy URL, Open Terminal, Logs, Edit, Open Folder, dan Delete
- Empty state: ajakan membuat server pertama.

### 14.2 Dialog New Server

- Pilihan engine (kartu).
- Dropdown versi. Versi yang sudah terpasang diberi label "installed".
- Kolom nama (default: `"<Engine> <major>"`, dibuat unik).
- Kolom port (default dari `suggest_port`, divalidasi secara live).
- Checkbox "Start automatically".
- Tombol **Create** (opsi: "Create & Start").

### 14.3 Detail / Logs

- Tab **Connection**: host, port, user, password, dan URL, masing-masing dengan tombol salin.
- Tab **Logs**: tail 200 baris, auto-refresh saat terbuka, tombol "Open log file".
- Tab **Settings**: nama, port, autostart, dan extra args (bagian lanjutan).
- Banner preflight jika ada masalah, lengkap dengan perintah perbaikan siap salin.

### 14.4 Settings

- Manifest URL dan tombol "Refresh versions".
- Process backend: Auto / systemd / Direct.
- Perintah terminal kustom.
- Daftar versi terpasang, ukuran di disk, dan tombol hapus.
- Lokasi data (hanya ditampilkan, tidak bisa diubah di v1).
- Tips `loginctl enable-linger`.

### 14.5 Teknis frontend

- Stack: React + TypeScript + Vite. Styling bebas, tapi sebaiknya ringan (CSS modules atau Tailwind).
- `ui/src/types.ts` harus sinkron dengan model Rust. Untuk v1 cukup ditulis manual. Opsi lanjut: generate dengan `specta` / `tauri-specta`.
- Mengikuti tema terang/gelap sistem.

---

## 15. CLI `dbnest`

```
dbnest engines                         # daftar engine & versi
dbnest list                            # instance + status + port
dbnest create <engine> <version> [--name N] [--port P] [--autostart]
dbnest start <id|name>
dbnest stop <id|name>
dbnest restart <id|name>
dbnest status <id|name>
dbnest logs <id|name> [-n 100] [-f]
dbnest info <id|name>                  # connection info
dbnest shell <id|name>                 # buka $SHELL dengan PATH/env engine (tanpa emulator)
dbnest env <id|name>                   # cetak export PATH=… (untuk eval)
dbnest delete <id|name> [--keep-data]
dbnest versions [--installed]
dbnest uninstall <engine> <version>
dbnest doctor                          # preflight semua + info sistem
```

Semua perintah mendukung `--json` untuk keperluan skrip. Exit code: `0` sukses, `1` error umum, `2` argumen salah, `3` preflight gagal.

---

## 16. Keamanan

- Semua server hanya mendengarkan di `127.0.0.1`. Opsi untuk mengubahnya tidak disediakan di v1.
- UI memberi peringatan jelas bahwa kredensial default tanpa password hanya untuk development.
- Folder instance diberi izin `0700`.
- Aplikasi dan server tidak pernah berjalan sebagai root. Aplikasi juga tidak pernah memanggil `sudo`.
- Semua unduhan memakai HTTPS dan diverifikasi sha256. Ekstraksi arsip dilindungi dari path traversal.
- Argumen proses selalu dikirim sebagai array (`Command::args`), tidak pernah lewat shell string.
- Nama instance hanya dipakai sebagai label. Path dan nama unit systemd selalu memakai `id` yang dihasilkan aplikasi (karakter `[a-z0-9-]`).

---

## 17. Error handling dan logging

```rust
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("instance tidak ditemukan: {0}")] InstanceNotFound(String),
    #[error("port {0} sudah dipakai")] PortInUse(u16),
    #[error("versi {engine:?} {version} tidak tersedia untuk arsitektur ini")] VersionUnavailable { engine: EngineKind, version: String },
    #[error("checksum tidak cocok")] ChecksumMismatch,
    #[error("library hilang: {0:?}")] MissingLibraries(Vec<String>),
    #[error("tidak boleh dijalankan sebagai root")] RunningAsRoot,
    #[error("server gagal siap dalam {0:?}")] StartTimeout(Duration),
    #[error("systemd: {0}")] Systemd(String),
    #[error(transparent)] Io(#[from] std::io::Error),
    #[error(transparent)] Http(#[from] reqwest::Error),
    #[error(transparent)] Json(#[from] serde_json::Error),
}
```

Setiap varian punya `code()` (string stabil, mis. `"port_in_use"`) dan `hint()` opsional untuk UI.

Log aplikasi memakai `tracing`, dengan output ke stderr dan ke `$XDG_STATE_HOME/dbnest/app.log` (rotasi sederhana, maksimal 5 MB). Log engine berada di `instances/<id>/logs/engine.log`.

---

## 18. Pengujian

| Lapisan | Cakupan |
|---|---|
| Unit (core) | parse manifest, pemilihan artefak (arch/distro), `systemd_escape_arg`, render unit file, render `redis.conf`, `suggest_port`, pemetaan library ke paket, migrasi config, proteksi path traversal |
| Integrasi (core) | Ditandai `#[ignore]` dan dijalankan dengan `cargo test -- --ignored` atau env `DBNEST_IT=1`. Isinya: install, create, start, health, stop, delete untuk Redis dan PostgreSQL (lalu MySQL dan MariaDB), memakai `XDG_*` sementara (`tempfile`) dan `DirectBackend`. |
| Systemd | Diuji manual atau di VM. Opsional: container dengan systemd (mis. image `jrei/systemd-ubuntu`). |
| CLI | `assert_cmd` untuk perintah dasar dan output `--json`. |
| Frontend | Minimal `vitest` untuk utilitas. Uji manual per rilis memakai checklist. |

Matriks CI:
- `ubuntu-22.04` dan `ubuntu-24.04` untuk build dan unit test.
- Tes integrasi Redis dan Postgres di `ubuntu-24.04`.
- Container Fedora untuk smoke test CLI.

Semua perubahan wajib lolos `cargo fmt --check` dan `cargo clippy -- -D warnings`.

---

## 19. Build dan distribusi

- `release.yml` menjalankan `tauri build` untuk bundle **AppImage**, **.deb**, dan **.rpm**, untuk x86_64 (aarch64 menyusul lewat runner ARM). Hasilnya diunggah ke GitHub Releases.
- Binary CLI `dbnest` disertakan di paket .deb/.rpm (`/usr/bin/dbnest`) dan juga sebagai tarball terpisah.
- `build-engines.yml` di repo terpisah `dbnest-engines` melakukan compile Redis dan PostgreSQL di container glibc 2.28 (mis. `almalinux:8`) agar kompatibel dengan distro lama. Hasilnya diunggah ke Releases, sha256-nya dihitung, lalu PR update `manifest.json` dibuat.
- Update aplikasi di v1 cukup berupa notifikasi versi baru (cek GitHub Releases API). Auto-update Tauri bisa ditambahkan belakangan.

---

## 20. Roadmap dan kriteria selesai

### Milestone 1 — Fondasi core + CLI (Redis, PostgreSQL)

Pekerjaan:
- Workspace, `paths`, `config` (atomic write + lock), `model`, `error`.
- Manifest (versi embed saja dulu), download dengan sha256, install atomik.
- Adapter Redis dan PostgreSQL.
- `DirectBackend`.
- Port utils.
- CLI: `engines`, `list`, `create`, `start`, `stop`, `status`, `logs`, `info`, `delete`.

Selesai jika:
- `dbnest create redis 7.x && dbnest start …` membuat `redis-cli -p P ping` membalas `PONG`.
- Dua instance PostgreSQL berbeda versi bisa berjalan bersamaan di port berbeda, dan `psql` bisa terhubung ke keduanya.
- Server tetap hidup setelah CLI keluar, dan `dbnest status` membacanya dengan benar.
- Unit test lulus, dan tes integrasi Redis serta Postgres lulus di CI.

### Milestone 2 — MySQL dan MariaDB + preflight

Pekerjaan:
- Adapter MySQL dan MariaDB.
- `preflight` (root, ldd, port, socket path, disk).
- Symlink compat `libaio`.
- `dbnest doctor`.

Selesai jika:
- MySQL 8.x dan MariaDB 11.x berjalan di Ubuntu 24.04 tanpa sudo (selain instalasi paket yang disarankan doctor).
- `mysql -u root -h 127.0.0.1 -P P` bisa terhubung.
- Dua instance MySQL berjalan bersamaan tanpa bentrok port 33060.

### Milestone 3 — GUI Tauri

Pekerjaan:
- Scaffold Tauri v2 dan React.
- Commands dan events.
- Layar utama, dialog New Server, detail (Connection/Logs/Settings), dan Settings.
- Open Terminal, Copy URL, Open Folder.

Selesai jika:
- Semua alur Milestone 1–2 bisa dilakukan lewat GUI.
- Status di GUI ikut berubah saat server di-stop lewat CLI.

### Milestone 4 — systemd, autostart, tray

Pekerjaan:
- `SystemdUserBackend` dan pemilihan `auto`.
- Autostart.
- Tray dengan menu per instance.
- Perilaku close-to-tray.

Selesai jika:
- Setelah logout dan login lagi, instance dengan `autostart` berjalan tanpa membuka aplikasi.
- `systemctl --user status dbnest-<id>` menunjukkan server yang benar.
- Log tetap masuk ke `engine.log`.

### Milestone 5 — MongoDB, manifest remote, rilis

Pekerjaan:
- Adapter MongoDB dengan pemilihan varian distro.
- Manifest remote dan cache.
- Uninstall versi.
- `build-engines.yml` dan `release.yml`.
- README dan panduan pengguna.

Selesai jika:
- AppImage dan .deb terpasang dan berjalan di Ubuntu 22.04/24.04 dan Fedora terbaru.
- Versi baru bisa muncul di aplikasi hanya dengan memperbarui manifest.

---

## 21. Risiko dan mitigasi

| Risiko | Mitigasi |
|---|---|
| Binary PostgreSQL/Redis tidak punya sumber resmi untuk Linux | Build sendiri di CI dengan glibc 2.28, lalu host di GitHub Releases |
| Perbedaan library antar distro | Preflight berbasis `ldd`, pemetaan paket, dan symlink compat |
| Tarball MongoDB spesifik distro | `variants_by_distro` dengan pesan jelas bila distro tidak didukung |
| Tray tidak tampil di GNOME | Aplikasi tetap berfungsi tanpa tray, dengan tips ekstensi AppIndicator |
| Sistem tanpa systemd (Alpine, Void, container) | `DirectBackend` |
| Lisensi (GPL MySQL, SSPL MongoDB) | Tidak membundel binary, hanya mengunduh dari sumber resmi saat runtime. Tampilkan lisensi engine di UI. |
| Merek dagang | Tidak memakai nama atau aset DBngin/TablePlus |

---

## 22. Keputusan terbuka

1. Nama final proyek dan ID aplikasi (mis. `io.github.<user>.dbnest`).
2. Sumber binary PostgreSQL: pihak ketiga atau build sendiri sejak awal.
3. Library UI (komponen dan styling).
4. Apakah password default opsional (ditentukan saat create) masuk ke v1.
5. Dukungan Valkey dan versi Redis ≥ 7.4 (perubahan lisensi Redis perlu dicek).
6. Lokasi hosting manifest (GitHub Pages atau Releases).
