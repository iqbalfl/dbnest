use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};

use crate::model::Instance;

/// Coba bind ke `127.0.0.1:port`. Listener langsung di-drop; hasil `true`
/// berarti port kosong saat pemeriksaan ini (bisa berubah race-condition-wise,
/// tapi cukup untuk preflight/UI).
pub fn is_port_free(port: u16) -> bool {
    TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).is_ok()
}

/// Cari port kosong mulai dari `default_port`, melewati port yang dipakai
/// instance lain di config (walaupun instance itu sedang berhenti) dan port
/// yang sedang dipakai proses lain. Naik satu per satu sampai +100.
pub fn suggest_port(default_port: u16, existing_instances: &[Instance]) -> Option<u16> {
    let taken: std::collections::HashSet<u16> = existing_instances.iter().map(|i| i.port).collect();
    for offset in 0..=100u32 {
        let candidate = default_port as u32 + offset;
        if candidate > u16::MAX as u32 {
            break;
        }
        let candidate = candidate as u16;
        if taken.contains(&candidate) {
            continue;
        }
        if is_port_free(candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EngineKind;

    fn instance_with_port(port: u16) -> Instance {
        Instance {
            id: "rds-000001".into(),
            name: "test".into(),
            engine: EngineKind::Redis,
            version: "7.4.0".into(),
            port,
            autostart: false,
            created_at: "2026-09-16T10:00:00Z".into(),
            extra_args: vec![],
        }
    }

    #[test]
    fn is_port_free_detects_bound_port() {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(!is_port_free(port));
        drop(listener);
        assert!(is_port_free(port));
    }

    #[test]
    fn suggest_port_skips_taken_ports_in_config() {
        let existing = vec![instance_with_port(6379), instance_with_port(6380)];
        let suggested = suggest_port(6379, &existing).unwrap();
        assert_eq!(suggested, 6381);
    }

    #[test]
    fn suggest_port_returns_default_when_free() {
        let suggested = suggest_port(6379, &[]).unwrap();
        assert_eq!(suggested, 6379);
    }
}
