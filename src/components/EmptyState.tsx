import { useEffect, useState } from "react";
import { getAppInfo } from "../lib/tauri";
import { useWorkspaceStore } from "../stores/workspaceStore";

export default function EmptyState() {
  const [version, setVersion] = useState("");
  const recent = useWorkspaceStore((s) => s.recent);
  const error = useWorkspaceStore((s) => s.error);
  const openViaDialog = useWorkspaceStore((s) => s.openViaDialog);
  const open = useWorkspaceStore((s) => s.open);

  useEffect(() => {
    getAppInfo()
      .then((info) => setVersion(info.version))
      .catch(() => {
        // No backend available (e.g. running in a plain browser) — leave
        // the version blank rather than crashing the empty window.
      });
  }, []);

  return (
    <div className="flex h-full w-full items-center justify-center bg-surface">
      <div className="flex w-80 flex-col items-center text-center">
        <p className="text-2xl font-semibold tracking-wide text-text">AgentLens</p>
        {version && <p className="mt-1 text-sm text-text-muted">v{version}</p>}

        <button
          type="button"
          onClick={() => void openViaDialog()}
          className="mt-6 rounded border border-accent px-4 py-2 text-sm font-medium text-accent hover:bg-hover"
        >
          Open folder
        </button>

        {error && <p className="mt-3 text-xs text-danger">{error}</p>}

        {recent.length > 0 && (
          <div className="mt-8 w-full text-left">
            <p className="mb-2 text-xs tracking-wide text-text-muted uppercase">Recent</p>
            <ul className="flex flex-col gap-1">
              {recent.map((path) => (
                <li key={path}>
                  <button
                    type="button"
                    onClick={() => void open(path)}
                    title={path}
                    className="w-full truncate rounded px-2 py-1 text-left text-sm text-text-muted hover:bg-hover hover:text-text"
                  >
                    {path}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
