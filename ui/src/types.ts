// Mirror manual dari `crates/core/src/model.rs`, `config.rs`, dan
// `manifest.rs`. Harus disinkronkan tangan setiap tipe Rust berubah
// (CLAUDE.md: "Tipe di ui/src/types.ts harus sinkron dengan
// crates/core/src/model.rs").

export type EngineKind = "postgres" | "mysql" | "mariadb" | "redis" | "mongodb";

export interface Instance {
  id: string;
  name: string;
  engine: EngineKind;
  version: string;
  port: number;
  autostart: boolean;
  created_at: string;
  extra_args: string[];
}

export type InstanceStatus =
  | { state: "not_initialized" }
  | { state: "stopped" }
  | { state: "starting" }
  | { state: "running"; pid: number | null }
  | { state: "stopping" }
  | { state: "failed"; message: string };

export interface ConnectionInfo {
  host: string;
  port: number;
  username: string | null;
  password: string | null;
  socket: string | null;
  url: string;
}

// `#[serde(flatten)]` di Instance + field status/connection terpisah.
export type InstanceView = Instance & {
  status: InstanceStatus;
  connection: ConnectionInfo;
};

export type InstallEvent =
  | { Downloading: { downloaded: number; total: number | null } }
  | "Verifying"
  | "Extracting"
  | "Done"
  | { Failed: { message: string } };

export type ProgressEvent =
  | "Preflight"
  | { Install: InstallEvent }
  | "Initializing"
  | "Starting"
  | "HealthCheck"
  | "Ready"
  | { Failed: { message: string } };

export type IssueSeverity = "error" | "warning" | "info";

export interface Issue {
  severity: IssueSeverity;
  message: string;
  fix_hint: string | null;
}

export interface InstalledVersion {
  engine: EngineKind;
  version: string;
  size_bytes: number;
}

export type ProcessBackendKind = "auto" | "systemd" | "direct";

export interface SettingsFile {
  schema_version: number;
  manifest_url: string | null;
  process_backend: ProcessBackendKind;
  terminal_command: string | null;
  start_minimized_to_tray: boolean;
}

export interface Artifact {
  url: string;
  sha256: string;
  format: string;
  strip_components: number;
  min_glibc: string | null;
}

export interface VersionEntry {
  version: string;
  channel: string;
  verified: boolean;
  artifacts: Record<string, Artifact> | null;
  variants_by_distro: unknown | null;
}

export interface EngineCatalog {
  display_name: string;
  default_port: number;
  versions: VersionEntry[];
}

export interface Manifest {
  schema_version: number;
  generated_at: string;
  engines: Record<string, EngineCatalog>;
}

// { code, message, hint? } — bentuk error command Tauri (§13.1).
export interface CommandError {
  code: string;
  message: string;
  hint?: string;
}

export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value
  );
}
