import { useEffect, useState } from "react";
import type { InstanceView, Issue } from "../types";
import { api } from "../api";
import { errorMessage, statusLabel } from "../format";
import DeleteConfirmDialog from "./DeleteConfirmDialog";

interface Props {
  view: InstanceView;
  onClose: () => void;
  onDeleted: () => void;
  onError: (message: string) => void;
}

type Tab = "connection" | "logs" | "settings";

export default function InstanceDetail({ view, onClose, onDeleted, onError }: Props) {
  const [tab, setTab] = useState<Tab>("connection");
  const [issues, setIssues] = useState<Issue[]>([]);
  const [logs, setLogs] = useState<string[]>([]);
  const [showDelete, setShowDelete] = useState(false);
  const [name, setName] = useState(view.name);
  const [port, setPort] = useState(view.port);
  const [autostart, setAutostart] = useState(view.autostart);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setName(view.name);
    setPort(view.port);
    setAutostart(view.autostart);
  }, [view.id, view.name, view.port, view.autostart]);

  useEffect(() => {
    // Refetch tiap status berubah, bukan cuma sekali saat mount: kalau
    // tidak, preflight yang berjalan tepat saat start() lagi mengikat
    // port-nya sendiri bisa nyasar melaporkan "port sudah dipakai" padahal
    // itu port instance ini sendiri, dan banner itu nempel selamanya.
    void api
      .runPreflight(view.id)
      .then(setIssues)
      .catch((e) => onError(errorMessage(e)));
  }, [view.id, view.status.state, onError]);

  useEffect(() => {
    if (tab !== "logs") return;
    let cancelled = false;
    const tick = () => {
      void api
        .getLogs(view.id, 200)
        .then((l) => {
          if (!cancelled) setLogs(l);
        })
        .catch((e) => onError(errorMessage(e)));
    };
    tick();
    const interval = setInterval(tick, 2000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [tab, view.id, onError]);

  const copy = (text: string) => void navigator.clipboard.writeText(text);

  const saveSettings = async () => {
    setSaving(true);
    try {
      await api.updateInstance(view.id, {
        name: name !== view.name ? name : null,
        port: port !== view.port ? port : null,
        autostart: autostart !== view.autostart ? autostart : null,
      });
    } catch (e) {
      onError(errorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const errorIssues = issues.filter((i) => i.severity === "error");

  return (
    <div className="screen">
      <header className="header">
        <button className="btn" onClick={onClose}>
          ← Kembali
        </button>
        <h1>{view.name}</h1>
        <button className="btn btn-danger" onClick={() => setShowDelete(true)}>
          Delete
        </button>
      </header>

      <p className="detail-subtitle">
        {view.engine} {view.version} · :{view.port} · {statusLabel(view.status)}
      </p>

      {errorIssues.length > 0 && (
        <div className="banner banner-error">
          {errorIssues.map((issue, idx) => (
            <div key={idx}>
              <strong>{issue.message}</strong>
              {issue.fix_hint && (
                <div className="fix-hint">
                  <code>{issue.fix_hint}</code>
                  <button className="btn btn-small" onClick={() => copy(issue.fix_hint!)}>
                    Copy
                  </button>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      <nav className="tabs">
        <button className={tab === "connection" ? "active" : ""} onClick={() => setTab("connection")}>
          Connection
        </button>
        <button className={tab === "logs" ? "active" : ""} onClick={() => setTab("logs")}>
          Logs
        </button>
        <button className={tab === "settings" ? "active" : ""} onClick={() => setTab("settings")}>
          Settings
        </button>
      </nav>

      {tab === "connection" && (
        <div className="tab-panel connection-panel">
          <Row label="Host" value={view.connection.host} onCopy={copy} />
          <Row label="Port" value={String(view.connection.port)} onCopy={copy} />
          {view.connection.username && (
            <Row label="Username" value={view.connection.username} onCopy={copy} />
          )}
          <Row
            label="Password"
            value={view.connection.password ?? "(tanpa password — hanya untuk development)"}
            onCopy={view.connection.password ? copy : undefined}
          />
          <Row label="URL" value={view.connection.url} onCopy={copy} />
        </div>
      )}

      {tab === "logs" && (
        <div className="tab-panel">
          <pre className="log-view">{logs.join("\n") || "(log kosong)"}</pre>
        </div>
      )}

      {tab === "settings" && (
        <div className="tab-panel settings-panel">
          <label className="field">
            <span>Nama</span>
            <input type="text" value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <label className="field">
            <span>Port</span>
            <input
              type="number"
              value={port}
              onChange={(e) => setPort(Number(e.target.value))}
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
          <button className="btn btn-primary" onClick={() => void saveSettings()} disabled={saving}>
            Simpan
          </button>
        </div>
      )}

      {showDelete && (
        <DeleteConfirmDialog
          instanceName={view.name}
          onCancel={() => setShowDelete(false)}
          onConfirm={async (keepData) => {
            try {
              await api.deleteInstance(view.id, !keepData);
              onDeleted();
            } catch (e) {
              onError(errorMessage(e));
            }
          }}
        />
      )}
    </div>
  );
}

function Row({
  label,
  value,
  onCopy,
}: {
  label: string;
  value: string;
  onCopy?: (text: string) => void;
}) {
  return (
    <div className="connection-row">
      <span className="connection-label">{label}</span>
      <code className="connection-value">{value}</code>
      {onCopy && (
        <button className="btn btn-small" onClick={() => onCopy(value)}>
          Copy
        </button>
      )}
    </div>
  );
}
