import { useEffect, useState } from "react";
import { getAppInfo } from "./lib/tauri";

export default function App() {
  const [version, setVersion] = useState("");

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
      <div className="text-center text-text-muted">
        <p className="text-2xl font-semibold tracking-wide text-text">AgentLens</p>
        {version && <p className="mt-1 text-sm">v{version}</p>}
      </div>
    </div>
  );
}
