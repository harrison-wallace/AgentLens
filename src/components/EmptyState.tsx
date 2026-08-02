import { useEffect, useState } from "react";
import FolderBrowser from "./FolderBrowser";
import { getAppInfo } from "../lib/tauri";
import { useBrowseStore } from "../stores/browseStore";
import { useConnectionStore } from "../stores/connectionStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import type { ConnectionTarget } from "../lib/protocol";

/** Which remote form is showing, if any. */
type RemoteMode = "wsl" | "ssh" | null;

export default function EmptyState() {
  const [version, setVersion] = useState("");
  const recent = useWorkspaceStore((s) => s.recent);
  const error = useWorkspaceStore((s) => s.error);
  const openViaDialog = useWorkspaceStore((s) => s.openViaDialog);
  const open = useWorkspaceStore((s) => s.open);
  const connection = useConnectionStore((s) => s.info);
  const startBrowse = useBrowseStore((s) => s.start);

  useEffect(() => {
    getAppInfo()
      .then((info) => setVersion(info.version))
      .catch(() => {
        // No backend available (e.g. running in a plain browser) — leave
        // the version blank rather than crashing the empty window.
      });
  }, []);

  // While connected elsewhere, the OS file dialog would be picking folders on
  // the wrong machine — and opening one would silently drop the connection,
  // since a bare path means "here".
  const openFolder = () =>
    connection.remote ? void startBrowse(connection.target) : void openViaDialog();

  return (
    <div className="flex h-full w-full items-center justify-center overflow-y-auto bg-surface py-10">
      <div className="flex w-80 flex-col items-center text-center">
        <p className="text-[22px] font-semibold tracking-wide text-text">AgentLens</p>
        {version && <p className="mt-1 text-[11px] text-text-muted">v{version}</p>}

        <button
          type="button"
          onClick={openFolder}
          className="mt-6 h-8 rounded border border-accent px-4 text-xs font-medium text-accent hover:bg-hover"
        >
          {connection.remote ? `Open folder on ${connection.label}` : "[+] Open folder"}
        </button>

        {error && <p className="mt-3 text-[11px] text-danger">{error}</p>}

        <RemoteOpen />

        {recent.length > 0 && (
          <div className="mt-8 w-full text-left">
            <p className="section-label mb-2">Recent</p>
            <ul className="flex flex-col gap-0.5">
              {recent.map((path) => (
                <li key={path}>
                  <button
                    type="button"
                    onClick={() => void open(path)}
                    title={path}
                    className="w-full truncate px-2 py-1 text-left text-xs text-text-muted hover:bg-hover hover:text-text"
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
 * Name the machine and either type the path or browse for it — there is no OS
 * file dialog for a WSL distro or an SSH host, so browsing is the backend
 * listing its own directories a level at a time. Whichever route, the result
 * is recorded as `wsl://distro/path`, so the recents list above reopens it
 * without any of this a second time.
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

      <FolderBrowser />

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
  // Setting up a machine that has never run AgentLens takes long enough that
  // "Connecting…" would look stuck, so it gets said out loud.
  const installing = useConnectionStore((s) => s.info.state === "installing");

  const startBrowse = useBrowseStore((s) => s.start);

  const named = host.trim().length > 0;
  // A WSL path has to be given to open *directly*: `wsl.exe` with no directory
  // starts in whatever Windows directory the app happens to be in, which is
  // never what was meant. SSH without one is the login directory, which is.
  // Browsing sidesteps the question entirely by starting at the home
  // directory and showing what is actually there.
  const canOpen = named && (kind === "ssh" || path.trim().length > 0);

  const target = (): ConnectionTarget =>
    kind === "wsl" ? { kind: "wsl", distro: host.trim() } : { kind: "ssh", host: host.trim() };

  const submit = () => {
    if (!canOpen) return;
    const suffix = path.trim() ? `/${path.trim().replace(/^\/+/, "")}` : "";
    void open(`${kind}://${host.trim()}${suffix}`);
  };

  return (
    <form
      className="mt-3 flex flex-col gap-2 rounded border border-border bg-surface-raised p-3 text-left"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      {kind === "wsl" ? (
        <label className="text-[11px] text-text-muted">
          Distro
          <select
            value={host}
            onChange={(event) => setHost(event.target.value)}
            className="mt-1 h-8 w-full rounded border border-border bg-surface px-2 text-xs text-text outline-none focus:border-accent"
          >
            {distros.map((distro) => (
              <option key={distro} value={distro}>
                {distro}
              </option>
            ))}
          </select>
        </label>
      ) : (
        <label className="text-[11px] text-text-muted">
          Host
          <input
            value={host}
            onChange={(event) => setHost(event.target.value)}
            placeholder="build-box"
            spellCheck={false}
            className="mt-1 h-8 w-full rounded border border-border bg-surface px-2 text-xs text-text outline-none placeholder:text-text-muted focus:border-accent"
          />
          <span className="mt-1 block text-[11px] text-text-ash">
            Any name your <code>ssh</code> command accepts — <code>~/.ssh/config</code> aliases,
            jump hosts and agent auth all apply.
          </span>
        </label>
      )}

      <label className="text-[11px] text-text-muted">
        Path <span className="text-text-ash">(optional — or browse for it)</span>
        <input
          value={path}
          onChange={(event) => setPath(event.target.value)}
          placeholder={kind === "wsl" ? "/home/you/project" : "leave blank for your home directory"}
          spellCheck={false}
          className="mt-1 h-8 w-full rounded border border-border bg-surface px-2 text-xs text-text outline-none placeholder:text-text-muted focus:border-accent"
        />
      </label>

      {/* Browsing needs only a machine to connect to, so it is offered as
          soon as one is named — knowing the path is the thing it exists to
          make unnecessary. */}
      <button
        type="button"
        onClick={() => void startBrowse(target())}
        disabled={!named || opening}
        className="h-7 rounded border border-border-strong px-3 text-[11px] font-medium text-text-muted hover:bg-hover hover:text-text disabled:cursor-not-allowed disabled:opacity-40"
      >
        Browse…
      </button>

      <button
        type="submit"
        disabled={!canOpen || opening}
        className="mt-1 h-7 rounded border border-accent px-3 text-[11px] font-medium text-accent hover:bg-hover disabled:cursor-not-allowed disabled:opacity-40"
      >
        {installing ? "Setting up…" : opening ? "Connecting…" : "Open"}
      </button>
    </form>
  );
}
