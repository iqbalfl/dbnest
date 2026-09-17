//! Tes integrasi menyeluruh: install → create → start → health → stop →
//! delete, untuk Redis dan PostgreSQL (DESIGN.md §18).
//!
//! Ditandai `#[ignore]` karena mengunduh binary asli dan menjalankan server
//! sungguhan. Jalankan dengan:
//!   DBNEST_IT=1 cargo test -p dbnest-core -- --ignored
//!
//! Prasyarat: entri versi terkait di `manifest/manifest.json` harus
//! `"verified": true` dengan URL/sha256 asli. Sejak `build-engines.yml`
//! dijalankan, manifest bawaan sudah memenuhi itu untuk keempat engine di
//! x86_64; kalau suatu entri kembali kosong, tes gagal dengan pesan yang
//! menyebutkan entri mana.
//!
//! Tes ini juga harus dijalankan sebagai user biasa: preflight menolak root
//! (DESIGN §8), jadi di container CI yang berjalan sebagai root binary tesnya
//! dijalankan lewat user tanpa hak istimewa.

use dbnest_core::config::{ConfigStore, ProcessBackendKind};
use dbnest_core::manager::{CreateInstanceRequest, Manager};
use dbnest_core::model::{EngineKind, InstanceStatus};
use dbnest_core::paths::Paths;

fn require_it_flag() {
    if std::env::var("DBNEST_IT").ok().as_deref() != Some("1") {
        panic!(
            "jalankan dengan DBNEST_IT=1 (atau `cargo test -- --ignored`) untuk tes integrasi ini"
        );
    }
}

/// Membuat Manager yang seluruh pathnya terisolasi di `root`, dengan backend
/// dipaksa ke `direct`.
///
/// Backend systemd tidak bisa dipakai di tes seperti ini: unit ditulis ke
/// `config_dir.parent()/systemd/user` (lihat `Paths::config_dir_for_systemd`),
/// yang di bawah root sementara menjadi direktori yang tidak pernah dibaca
/// systemd — jadi `systemctl --user start` menjawab "Unit not found". Yang
/// diuji di sini adalah daur hidup engine, bukan integrasi systemd, jadi
/// backendnya dipilih eksplisit ketimbang bergantung pada pemilihan `auto`
/// yang hasilnya berbeda antar mesin.
fn isolated_manager(root: &std::path::Path) -> Manager {
    let paths = Paths::under_root(root);
    paths.ensure_base_dirs().unwrap();

    // Settings harus ditulis SEBELUM Manager dibuat. `Manager::with_paths`
    // memanggil `select_backend()` sekali saat konstruksi, jadi mengubah
    // settings lewat `update_settings()` setelahnya tidak mengganti backend
    // yang sudah dipilih instance itu — percobaan pertama perbaikan ini gagal
    // tepat karena itu.
    let config = ConfigStore::new(paths.clone());
    let mut settings = config.load_settings().unwrap();
    settings.process_backend = ProcessBackendKind::Direct;
    config.save_settings(&settings).unwrap();

    let manager = Manager::with_paths(paths).unwrap();
    assert!(
        manager.is_direct_backend(),
        "tes ini butuh backend direct, dapat: {}",
        manager.backend_label()
    );
    manager
}

async fn full_lifecycle(engine: EngineKind, version: &str) {
    let tmp = tempfile::tempdir().unwrap();
    let manager = isolated_manager(tmp.path());

    let manifest = manager.manifest().unwrap();
    let verified = manifest
        .version_entry(engine, version)
        .map(|v| v.verified)
        .unwrap_or(false);
    if !verified {
        panic!(
            "versi {engine} {version} belum diverifikasi di manifest/manifest.json \
             (URL/sha256 masih placeholder \"TODO\"); lengkapi manifest sebelum \
             menjalankan tes integrasi ini"
        );
    }

    let instance = manager
        .create_instance(CreateInstanceRequest {
            engine,
            version: version.to_string(),
            name: None,
            port: None,
            autostart: false,
        })
        .await
        .unwrap();

    manager.start(&instance.id, |_event| {}).await.unwrap();

    let status = manager.status(&instance.id).await.unwrap();
    assert!(
        matches!(status, InstanceStatus::Running { .. }),
        "instance harus Running setelah start, dapat: {status:?}"
    );

    manager.stop(&instance.id).await.unwrap();
    let status = manager.status(&instance.id).await.unwrap();
    assert_eq!(status, InstanceStatus::Stopped);

    manager.delete_instance(&instance.id, true).await.unwrap();
    assert!(manager.find_instance(&instance.id).is_err());
}

#[tokio::test]
#[ignore]
async fn redis_full_lifecycle() {
    require_it_flag();
    full_lifecycle(EngineKind::Redis, "7.4.0").await;
}

#[tokio::test]
#[ignore]
async fn postgres_full_lifecycle() {
    require_it_flag();
    full_lifecycle(EngineKind::Postgres, "16.4").await;
}

#[tokio::test]
#[ignore]
async fn mysql_full_lifecycle() {
    require_it_flag();
    full_lifecycle(EngineKind::Mysql, "8.4.3").await;
}

#[tokio::test]
#[ignore]
async fn mariadb_full_lifecycle() {
    require_it_flag();
    full_lifecycle(EngineKind::Mariadb, "11.4.4").await;
}

/// Dua instance MySQL berjalan bersamaan tanpa bentrok port 33060, karena
/// adapter selalu menambahkan `--mysqlx=OFF` (kriteria selesai Milestone 2).
#[tokio::test]
#[ignore]
async fn two_mysql_instances_run_concurrently() {
    require_it_flag();
    let tmp = tempfile::tempdir().unwrap();
    let manager = isolated_manager(tmp.path());

    let manifest = manager.manifest().unwrap();
    let verified = manifest
        .version_entry(EngineKind::Mysql, "8.4.3")
        .map(|v| v.verified)
        .unwrap_or(false);
    if !verified {
        panic!("versi MySQL 8.4.3 belum diverifikasi di manifest/manifest.json");
    }

    let a = manager
        .create_instance(CreateInstanceRequest {
            engine: EngineKind::Mysql,
            version: "8.4.3".to_string(),
            name: Some("my-a".to_string()),
            port: Some(13306),
            autostart: false,
        })
        .await
        .unwrap();
    let b = manager
        .create_instance(CreateInstanceRequest {
            engine: EngineKind::Mysql,
            version: "8.4.3".to_string(),
            name: Some("my-b".to_string()),
            port: Some(13307),
            autostart: false,
        })
        .await
        .unwrap();

    manager.start(&a.id, |_| {}).await.unwrap();
    manager.start(&b.id, |_| {}).await.unwrap();

    assert!(matches!(
        manager.status(&a.id).await.unwrap(),
        InstanceStatus::Running { .. }
    ));
    assert!(matches!(
        manager.status(&b.id).await.unwrap(),
        InstanceStatus::Running { .. }
    ));

    manager.stop(&a.id).await.unwrap();
    manager.stop(&b.id).await.unwrap();
    manager.delete_instance(&a.id, true).await.unwrap();
    manager.delete_instance(&b.id, true).await.unwrap();
}

/// Dua instance PostgreSQL versi sama di port berbeda harus bisa berjalan
/// bersamaan (kriteria selesai Milestone 1).
#[tokio::test]
#[ignore]
async fn two_postgres_instances_run_concurrently() {
    require_it_flag();
    let tmp = tempfile::tempdir().unwrap();
    let manager = isolated_manager(tmp.path());

    let manifest = manager.manifest().unwrap();
    let verified = manifest
        .version_entry(EngineKind::Postgres, "16.4")
        .map(|v| v.verified)
        .unwrap_or(false);
    if !verified {
        panic!("versi Postgres 16.4 belum diverifikasi di manifest/manifest.json");
    }

    let a = manager
        .create_instance(CreateInstanceRequest {
            engine: EngineKind::Postgres,
            version: "16.4".to_string(),
            name: Some("pg-a".to_string()),
            port: Some(15432),
            autostart: false,
        })
        .await
        .unwrap();
    let b = manager
        .create_instance(CreateInstanceRequest {
            engine: EngineKind::Postgres,
            version: "16.4".to_string(),
            name: Some("pg-b".to_string()),
            port: Some(15433),
            autostart: false,
        })
        .await
        .unwrap();

    manager.start(&a.id, |_| {}).await.unwrap();
    manager.start(&b.id, |_| {}).await.unwrap();

    assert!(matches!(
        manager.status(&a.id).await.unwrap(),
        InstanceStatus::Running { .. }
    ));
    assert!(matches!(
        manager.status(&b.id).await.unwrap(),
        InstanceStatus::Running { .. }
    ));

    manager.stop(&a.id).await.unwrap();
    manager.stop(&b.id).await.unwrap();
    manager.delete_instance(&a.id, true).await.unwrap();
    manager.delete_instance(&b.id, true).await.unwrap();
}
