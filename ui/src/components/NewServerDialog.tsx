import { useEffect, useMemo, useState } from "react";
import type { EngineKind, InstalledVersion, Manifest } from "../types";
import { api, CreateInstanceArgs } from "../api";
import { errorMessage } from "../format";

interface Props {
  manifest: Manifest;
  onClose: () => void;
  onCreated: (id: string, startNow: boolean) => void;
}

const ENGINE_ORDER: EngineKind[] = ["postgres", "redis", "mysql", "mariadb", "mongodb"];

export default function NewServerDialog({ manifest, onClose, onCreated }: Props) {
  const availableEngines = ENGINE_ORDER.filter((e) => manifest.engines[e]);
  const [engine, setEngine] = useState<EngineKind>(availableEngines[0] ?? "postgres");
  const [version, setVersion] = useState<string>("");
  const [name, setName] = useState("");
  const [port, setPort] = useState<number | "">("");
  const [autostart, setAutostart] = useState(false);
  const [installed, setInstalled] = useState<InstalledVersion[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const catalog = manifest.engines[engine];
  const versions = useMemo(() => catalog?.versions ?? [], [catalog]);

  useEffect(() => {
    void api.listInstalledVersions().then(setInstalled).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (versions.length > 0 && !versions.some((v) => v.version === version)) {
      setVersion(versions[0].version);
    }
  }, [versions, version]);

  useEffect(() => {
    void api
      .suggestPort(engine)
      .then((p) => setPort(p))
      .catch(() => undefined);
  }, [engine]);

  const isInstalled = (v: string) =>
    installed.some((i) => i.engine === engine && i.version === v);

  const submit = async (startNow: boolean) => {
    setSubmitting(true);
    setError(null);
    try {
      const payload: CreateInstanceArgs = {
        engine,
        version,
        name: name.trim() ? name.trim() : null,
        port: port === "" ? null : port,
        autostart,
      };
      const instance = await api.createInstance(payload);
      onCreated(instance.id, startNow);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>New Server</h2>

        <div className="engine-picker">
          {availableEngines.map((kind) => (
            <button
              key={kind}
              className={`engine-card ${engine === kind ? "selected" : ""}`}
              onClick={() => setEngine(kind)}
            >
              {manifest.engines[kind]?.display_name ?? kind}
            </button>
          ))}
        </div>

        <label className="field">
          <span>Versi</span>
          <select value={version} onChange={(e) => setVersion(e.target.value)}>
            {versions.map((v) => (
              <option key={v.version} value={v.version}>
                {v.version}
                {isInstalled(v.version) ? " (installed)" : ""}
                {!v.verified ? " — belum tersedia" : ""}
              </option>
            ))}
          </select>
        </label>

        <label className="field">
          <span>Nama</span>
          <input
            type="text"
            placeholder={`${catalog?.display_name ?? engine} (otomatis)`}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </label>

        <label className="field">
          <span>Port</span>
          <input
            type="number"
            value={port}
            onChange={(e) => setPort(e.target.value ? Number(e.target.value) : "")}
          />
        </label>

        <label className="field field-checkbox">
          <input
            type="checkbox"
            checked={autostart}
            onChange={(e) => setAutostart(e.target.checked)}
          />
          <span>Start automatically</span>
        </label>

        {error && <p className="error-text">{error}</p>}

        <div className="modal-actions">
          <button className="btn" onClick={onClose} disabled={submitting}>
            Batal
          </button>
          <button
            className="btn"
            onClick={() => void submit(false)}
            disabled={submitting || !version}
          >
            Create
          </button>
          <button
            className="btn btn-primary"
            onClick={() => void submit(true)}
            disabled={submitting || !version}
          >
            Create &amp; Start
          </button>
        </div>
      </div>
    </div>
  );
}
