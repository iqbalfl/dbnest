import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import { onInstanceProgress, onInstanceStatus, onInstancesChanged } from "./events";
import type { InstanceView, Manifest, ProgressEvent } from "./types";
import InstanceList from "./components/InstanceList";
import NewServerDialog from "./components/NewServerDialog";
import InstanceDetail from "./components/InstanceDetail";
import SettingsScreen from "./components/SettingsScreen";
import { errorMessage } from "./format";

type Screen = { name: "list" } | { name: "detail"; id: string } | { name: "settings" };

export default function App() {
  const [instances, setInstances] = useState<InstanceView[]>([]);
  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [screen, setScreen] = useState<Screen>({ name: "list" });
  const [showNewServer, setShowNewServer] = useState(false);
  const [progress, setProgress] = useState<Record<string, ProgressEvent>>({});
  const [error, setError] = useState<string | null>(null);
  const errorTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const showError = useCallback((message: string) => {
    setError(message);
    if (errorTimer.current) clearTimeout(errorTimer.current);
    errorTimer.current = setTimeout(() => setError(null), 6000);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const [list, m] = await Promise.all([api.listInstances(), api.listEngines()]);
      setInstances(list);
      setManifest(m);
    } catch (e) {
      showError(errorMessage(e));
    }
  }, [showError]);

  useEffect(() => {
    void refresh();
    const subs = [
      onInstancesChanged(() => void refresh()),
      onInstanceStatus(({ id, status }) => {
        setInstances((prev) => prev.map((i) => (i.id === id ? { ...i, status } : i)));
        setProgress((prev) => {
          if (!(id in prev)) return prev;
          const { [id]: _removed, ...rest } = prev;
          return rest;
        });
      }),
      onInstanceProgress(({ id, event }) => {
        setProgress((prev) => ({ ...prev, [id]: event }));
      }),
    ];
    return () => {
      subs.forEach((p) => void p.then((unlisten) => unlisten()));
    };
  }, [refresh]);

  const selected =
    screen.name === "detail" ? instances.find((i) => i.id === screen.id) : undefined;

  return (
    <div className="app">
      {error && (
        <div className="toast toast-error" onClick={() => setError(null)}>
          {error}
        </div>
      )}

      {screen.name === "list" && (
        <>
          <InstanceList
            instances={instances}
            progress={progress}
            onOpenDetail={(id) => setScreen({ name: "detail", id })}
            onNewServer={() => setShowNewServer(true)}
            onError={showError}
          />
          <button
            className="btn settings-fab"
            onClick={() => setScreen({ name: "settings" })}
            aria-label="Settings"
          >
            ⚙
          </button>
        </>
      )}

      {screen.name === "detail" && selected && (
        <InstanceDetail
          view={selected}
          onClose={() => setScreen({ name: "list" })}
          onDeleted={() => {
            setScreen({ name: "list" });
            void refresh();
          }}
          onError={showError}
        />
      )}

      {screen.name === "settings" && (
        <SettingsScreen onClose={() => setScreen({ name: "list" })} onError={showError} />
      )}

      {showNewServer && manifest && (
        <NewServerDialog
          manifest={manifest}
          onClose={() => setShowNewServer(false)}
          onCreated={(id, startNow) => {
            setShowNewServer(false);
            void refresh();
            if (startNow) {
              api.startInstance(id).catch((e) => showError(errorMessage(e)));
            }
            setScreen({ name: "detail", id });
          }}
        />
      )}
    </div>
  );
}
