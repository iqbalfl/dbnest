// Pembungkus tipis & bertipe di atas `invoke()`, satu fungsi per Tauri
// command dari `src-tauri/src/commands.rs` (DESIGN.md §13.1).

import { invoke } from "@tauri-apps/api/core";
import type {
  EngineKind,
  Instance,
  InstanceView,
  Issue,
  InstalledVersion,
  Manifest,
  SettingsFile,
} from "./types";

export interface CreateInstanceArgs {
  engine: EngineKind;
  version: string;
  name?: string | null;
  port?: number | null;
  autostart: boolean;
}

export interface UpdateInstanceArgs {
  name?: string | null;
  port?: number | null;
  autostart?: boolean | null;
}

export const api = {
  listEngines: () => invoke<Manifest>("list_engines"),

  listInstances: () => invoke<InstanceView[]>("list_instances"),

  createInstance: (payload: CreateInstanceArgs) =>
    invoke<Instance>("create_instance", { payload }),

  updateInstance: (id: string, payload: UpdateInstanceArgs) =>
    invoke<Instance>("update_instance", { id, payload }),

  deleteInstance: (id: string, deleteData: boolean) =>
    invoke<void>("delete_instance", { id, deleteData }),

  startInstance: (id: string) => invoke<void>("start_instance", { id }),

  stopInstance: (id: string) => invoke<void>("stop_instance", { id }),

  restartInstance: (id: string) => invoke<void>("restart_instance", { id }),

  getLogs: (id: string, lines: number) =>
    invoke<string[]>("get_logs", { id, lines }),

  runPreflight: (id: string) => invoke<Issue[]>("run_preflight", { id }),

  openTerminal: (id: string) => invoke<string | null>("open_terminal", { id }),

  openDataFolder: (id: string) => invoke<void>("open_data_folder", { id }),

  suggestPort: (engine: EngineKind) => invoke<number>("suggest_port", { engine }),

  listInstalledVersions: () =>
    invoke<InstalledVersion[]>("list_installed_versions"),

  uninstallVersion: (engine: EngineKind, version: string) =>
    invoke<void>("uninstall_version", { engine, version }),

  getSettings: () => invoke<SettingsFile>("get_settings"),

  updateSettings: (settings: SettingsFile) =>
    invoke<SettingsFile>("update_settings", { settings }),

  quitApp: (stopServers: boolean) =>
    invoke<void>("quit_app", { stopServers }),

  activeBackend: () => invoke<string>("active_backend"),
};
