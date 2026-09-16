import { useState } from "react";
import type { InstanceView, ProgressEvent } from "../types";
import { statusLabel, isRunning, progressLabel, errorMessage } from "../format";
import { api } from "../api";

interface Props {
  instances: InstanceView[];
  progress: Record<string, ProgressEvent>;
  onOpenDetail: (id: string) => void;
  onNewServer: () => void;
  onError: (message: string) => void;
}

const ENGINE_ICON: Record<string, string> = {
  postgres: "🐘",
  mysql: "🐬",
  mariadb: "🦭",
  redis: "🟥",
  mongodb: "🍃",
};

export default function InstanceList({
  instances,
  progress,
  onOpenDetail,
  onNewServer,
  onError,
}: Props) {
  const [busy, setBusy] = useState<Record<string, boolean>>({});
  const [menuOpenFor, setMenuOpenFor] = useState<string | null>(null);

  const withBusy = async (id: string, fn: () => Promise<void>) => {
    setBusy((b) => ({ ...b, [id]: true }));
    try {
      await fn();
    } catch (e) {
      onError(errorMessage(e));
    } finally {
      setBusy((b) => ({ ...b, [id]: false }));
    }
  };

  const toggleStart = (view: InstanceView) => {
    const running = isRunning(view.status);
    void withBusy(view.id, () =>
      running ? api.stopInstance(view.id) : api.startInstance(view.id),
    );
  };

  return (
    <div className="screen">
      <header className="header">
        <h1>DBnest</h1>
        <button className="btn btn-primary" onClick={onNewServer}>
          + New Server
        </button>
      </header>

      {instances.length === 0 ? (
        <div className="empty-state">
          <p>Belum ada server. Buat server pertama Anda.</p>
          <button className="btn btn-primary" onClick={onNewServer}>
            + New Server
          </button>
        </div>
      ) : (
        <ul className="instance-list">
          {instances.map((view) => {
            const running = isRunning(view.status);
            const isBusy = busy[view.id];
            const activeProgress = progress[view.id];
            return (
              <li key={view.id} className="instance-card">
                <button
                  className="instance-card-main"
                  onClick={() => onOpenDetail(view.id)}
                >
                  <span className="engine-icon">{ENGINE_ICON[view.engine] ?? "🗄️"}</span>
                  <span className="instance-info">
                    <span className="instance-name">{view.name}</span>
                    <span className="instance-meta">
                      {view.engine} {view.version} · :{view.port}
                    </span>
                  </span>
                  <span className={`status-dot status-${view.status.state}`} />
                  <span className="status-text">
                    {activeProgress ? progressLabel(activeProgress) : statusLabel(view.status)}
                  </span>
                </button>

                <button
                  className="btn"
                  disabled={isBusy}
                  onClick={() => toggleStart(view)}
                >
                  {running ? "Stop" : "Start"}
                </button>

                <div className="menu-wrapper">
                  <button
                    className="btn btn-icon"
                    onClick={() =>
                      setMenuOpenFor(menuOpenFor === view.id ? null : view.id)
                    }
                    aria-label="Menu"
                  >
                    ⋯
                  </button>
                  {menuOpenFor === view.id && (
                    <div className="menu" onMouseLeave={() => setMenuOpenFor(null)}>
                      <button
                        onClick={() => {
                          navigator.clipboard.writeText(view.connection.url);
                          setMenuOpenFor(null);
                        }}
                      >
                        Copy URL
                      </button>
                      <button
                        onClick={() => {
                          void api.openTerminal(view.id).catch((e) => onError(errorMessage(e)));
                          setMenuOpenFor(null);
                        }}
                      >
                        Open Terminal
                      </button>
                      <button
                        onClick={() => {
                          void api
                            .openDataFolder(view.id)
                            .catch((e) => onError(errorMessage(e)));
                          setMenuOpenFor(null);
                        }}
                      >
                        Open Folder
                      </button>
                      <button
                        onClick={() => {
                          onOpenDetail(view.id);
                          setMenuOpenFor(null);
                        }}
                      >
                        Detail / Logs
                      </button>
                    </div>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
