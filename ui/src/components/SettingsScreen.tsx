import { useEffect, useState } from "react";
import type {
  InstalledVersion,
  ManifestSource,
  ProcessBackendKind,
  SettingsFile,
} from "../types";
import { api } from "../api";
import { errorMessage, formatBytes } from "../format";

interface Props {
  onClose: () => void;
  onError: (message: string) => void;
}

export default function SettingsScreen({ onClose, onError }: Props) {
  const [settings, setSettings] = useState<SettingsFile | null>(null);
  const [versions, setVersions] = useState<InstalledVersion[]>([]);
  const [activeBackend, setActiveBackend] = useState<string | null>(null);
  const [manifestSource, setManifestSource] = useState<ManifestSource | null>(null);
  const [saving, setSaving] = useState(false);
  const [refreshing, setRefreshing] = useState(false);

  const load = () => {
    void api.getSettings().then(setSettings).catch((e) => onError(errorMessage(e)));
    void api
      .listInstalledVersions()
      .then(setVersions)
      .catch((e) => onError(errorMessage(e)));
    void api.activeBackend().then(setActiveBackend).catch(() => undefined);
    void api.manifestSource().then(setManifestSource).catch(() => undefined);
  };

  // Unduh ulang manifest dari manifest_url. Kegagalan sengaja ditampilkan
  // ke pengguna: mereka baru saja menekan tombolnya (§5.3).
  const refreshVersions = async () => {
    setRefreshing(true);
    try {
      await api.refreshManifest();
      load();
    } catch (e) {
      onError(errorMessage(e));
    } finally {
      setRefreshing(false);
    }
  };

  useEffect(load, []);

  const save = async (patch: Partial<SettingsFile>) => {
    if (!settings) return;
    setSaving(true);
    try {
      const updated = await api.updateSettings({ ...settings, ...patch });
      setSettings(updated);
    } catch (e) {
      onError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const uninstall = async (v: InstalledVersion) => {
    try {
      await api.uninstallVersion(v.engine, v.version);
      setVersions((prev) =>
        prev.filter((x) => !(x.engine === v.engine && x.version === v.version)),
      );
    } catch (e) {
      onError(errorMessage(e));
    }
  };

  if (!settings) {
    return (
      <div className="screen">
        <header className="header">
          <button className="btn" onClick={onClose}>
            ← Kembali
          </button>
          <h1>Settings</h1>
        </header>
        <p>Memuat…</p>
      </div>
    );
  }

  return (
    <div className="screen">
      <header className="header">
        <button className="btn" onClick={onClose}>
          ← Kembali
        </button>
        <h1>Settings</h1>
      </header>

      <section className="settings-section">
        <h2>Manifest</h2>
        <label className="field">
          <span>Manifest URL</span>
          <input
            type="text"
            value={settings.manifest_url ?? ""}
            placeholder="(pakai manifest bawaan aplikasi)"
            onChange={(e) => setSettings({ ...settings, manifest_url: e.target.value || null })}
            onBlur={() => void save({ manifest_url: settings.manifest_url })}
          />
        </label>
        <button
          className="btn"
          onClick={() => void refreshVersions()}
          disabled={saving || refreshing}
        >
          {refreshing ? "Mengunduh…" : "Refresh versions"}
        </button>
        {manifestSource && (
          <p className="hint-text">
            {manifestSource === "cache"
              ? "Daftar versi berasal dari manifest yang sudah diunduh."
              : "Daftar versi masih dari manifest bawaan aplikasi — isi Manifest URL lalu Refresh untuk mengambil yang terbaru."}
          </p>
        )}
      </section>

      <section className="settings-section">
        <h2>Process backend</h2>
        <select
          value={settings.process_backend}
          onChange={(e) =>
            void save({ process_backend: e.target.value as ProcessBackendKind })
          }
        >
          <option value="auto">Auto</option>
          <option value="systemd">systemd</option>
          <option value="direct">Direct</option>
        </select>
        {activeBackend && (
          <p className="hint-text">
            Backend aktif saat ini: <code>{activeBackend}</code>. Perubahan di atas berlaku
            setelah aplikasi dijalankan ulang.
          </p>
        )}
        <p className="hint-text">
          Tips: agar server tetap berjalan setelah logout (bukan cuma setelah login), jalankan{" "}
          <code>loginctl enable-linger $USER</code>.
        </p>
      </section>

      <section className="settings-section">
        <h2>Terminal</h2>
        <label className="field">
          <span>Perintah terminal kustom</span>
          <input
            type="text"
            value={settings.terminal_command ?? ""}
            placeholder="(deteksi otomatis)"
            onChange={(e) =>
              setSettings({ ...settings, terminal_command: e.target.value || null })
            }
            onBlur={() => void save({ terminal_command: settings.terminal_command })}
          />
        </label>
      </section>

      <section className="settings-section">
        <h2>Versi terpasang</h2>
        {versions.length === 0 ? (
          <p>Belum ada versi terpasang.</p>
        ) : (
          <ul className="version-list">
            {versions.map((v) => (
              <li key={`${v.engine}-${v.version}`}>
                <span>
                  {v.engine} {v.version}
                </span>
                <span>{formatBytes(v.size_bytes)}</span>
                <button className="btn btn-small btn-danger" onClick={() => void uninstall(v)}>
                  Hapus
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="settings-section">
        <h2>Lokasi data</h2>
        <p className="hint-text">
          <code>$XDG_DATA_HOME/dbnest</code> (default <code>~/.local/share/dbnest</code>).
          Tidak bisa diubah dari UI ini.
        </p>
      </section>
    </div>
  );
}
