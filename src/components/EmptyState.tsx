import { useEffect, useState } from "react";
import { getAppInfo } from "../lib/tauri";
import { useConnectionStore } from "../stores/connectionStore";
import { useWorkspaceStore } from "../stores/workspaceStore";

/** Which remote form is showing, if any. */
type RemoteMode = "wsl" | "ssh" | null;

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
    <div className="flex h-full w-full items-center justify-center overflow-y-auto bg-surface py-10">
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

        <RemoteOpen />

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

/**
 * Opening a workspace that lives on another machine.
 *
 * There is no remote folder browser and there deliberately isn't one yet: the
 * app would need a whole second tree UI to pick a directory it can already be
 * told about in one line. So the location is typed, and — because it is
 * recorded as `wsl://distro/path` — the recents list above reopens it without
 * any of this a second time.
 */
function RemoteOpen() {
  const [mode, setMode] = useState<RemoteMode>(null);
  const distros = useConnectionStore((s) => s.distros);
  const info = useConnectionStore((s) => s.info);
  const goLocal = useConnectionStore((s) => s.goLocal);
  const loadDistros = useConnectionStore((s) => s.loadDistros);

  useEffect(() => {
    void loadDistros();
  }, [loadDistros]);

  return (
    <div className="mt-4 w-full">
      <div className="flex justify-center gap-4 text-xs">
        {/* WSL is offered only where it exists. On Linux the button would be
            a permanent dead end. */}
        {distros.length > 0 && (
          <button
            type="button"
            onClick={() => setMode(mode === "wsl" ? null : "wsl")}
            className="text-text-muted hover:text-text"
          >
            Open in WSL…
          </button>
        )}
        <button
          type="button"
          onClick={() => setMode(mode === "ssh" ? null : "ssh")}
          className="text-text-muted hover:text-text"
        >
          Open over SSH…
        </button>
      </div>

      {mode === "wsl" && <RemoteForm kind="wsl" distros={distros} />}
      {mode === "ssh" && <RemoteForm kind="ssh" distros={distros} />}

      {info.remote && (
        <p className="mt-3 text-center text-xs text-text-muted">
          Connected to {info.label} ·{" "}
          <button
            type="button"
            onClick={() => void goLocal()}
            className="underline hover:text-text"
          >
            use this machine
          </button>
        </p>
      )}
    </div>
  );
}

function RemoteForm({ kind, distros }: { kind: "wsl" | "ssh"; distros: string[] }) {
  const [host, setHost] = useState(kind === "wsl" ? (distros[0] ?? "") : "");
  const [path, setPath] = useState("");
  const open = useWorkspaceStore((s) => s.open);
  const opening = useWorkspaceStore((s) => s.opening);

  // A WSL path has to be given: `wsl.exe` with no directory starts in whatever
  // Windows directory the app happens to be in, which is never what was meant.
  // SSH without one is the login directory, which is.
  const ready = host.trim().length > 0 && (kind === "ssh" || path.trim().length > 0);

  const submit = () => {
    if (!ready) return;
    const suffix = path.trim() ? `/${path.trim().replace(/^\/+/, "")}` : "";
    void open(`${kind}://${host.trim()}${suffix}`);
  };

  return (
    <form
      className="mt-3 flex flex-col gap-2 rounded border border-border bg-bg p-3 text-left"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      {kind === "wsl" ? (
        <label className="text-xs text-text-muted">
          Distro
          <select
            value={host}
            onChange={(event) => setHost(event.target.value)}
            className="mt-1 w-full rounded border border-border bg-surface px-2 py-1 text-xs text-text outline-none focus:border-glow"
          >
            {distros.map((distro) => (
              <option key={distro} value={distro}>
                {distro}
              </option>
            ))}
          </select>
        </label>
      ) : (
        <label className="text-xs text-text-muted">
          Host
          <input
            value={host}
            onChange={(event) => setHost(event.target.value)}
            placeholder="build-box"
            spellCheck={false}
            className="mt-1 w-full rounded border border-border bg-surface px-2 py-1 font-mono text-xs text-text outline-none placeholder:text-text-muted focus:border-glow"
          />
          <span className="mt-1 block text-[11px] text-text-muted">
            Any name your <code>ssh</code> command accepts — <code>~/.ssh/config</code> aliases,
            jump hosts and agent auth all apply.
          </span>
        </label>
      )}

      <label className="text-xs text-text-muted">
        Path
        <input
          value={path}
          onChange={(event) => setPath(event.target.value)}
          placeholder={kind === "wsl" ? "/home/you/project" : "leave blank for your home directory"}
          spellCheck={false}
          className="mt-1 w-full rounded border border-border bg-surface px-2 py-1 font-mono text-xs text-text outline-none placeholder:text-text-muted focus:border-glow"
        />
      </label>

      <button
        type="submit"
        disabled={!ready || opening}
        className="mt-1 rounded border border-accent px-3 py-1 text-xs font-medium text-accent hover:bg-hover disabled:cursor-not-allowed disabled:opacity-40"
      >
        {opening ? "Connecting…" : "Open"}
      </button>
    </form>
  );
}
