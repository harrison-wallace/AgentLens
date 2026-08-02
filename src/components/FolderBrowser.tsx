import { useBrowseStore } from "../stores/browseStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import type { ConnectionTarget } from "../lib/protocol";

/**
 * Build the location string for a directory on `target`.
 *
 * The picker deals in absolute paths on the remote, but `openWorkspace` reads
 * a bare path as "this machine" — so handing it one straight from the listing
 * would silently switch back to local and open a directory that probably
 * doesn't exist here.
 */
function locationOf(target: ConnectionTarget, path: string): string {
  const suffix = path.startsWith("/") ? path : `/${path}`;
  switch (target.kind) {
    case "local":
      return path;
    case "wsl":
      return `wsl://${target.distro}${suffix}`;
    case "ssh":
      return `ssh://${target.host}${suffix}`;
  }
}

/**
 * Choosing a directory on a machine you aren't sitting at.
 *
 * There is no OS file dialog for a WSL distro or an SSH host, so this is it:
 * the same listing the backend already knows how to produce, one level at a
 * time, with the current directory itself as the thing being chosen.
 */
export default function FolderBrowser() {
  const listing = useBrowseStore((s) => s.listing);
  const target = useBrowseStore((s) => s.target);
  const loading = useBrowseStore((s) => s.loading);
  const error = useBrowseStore((s) => s.error);
  const go = useBrowseStore((s) => s.go);
  const close = useBrowseStore((s) => s.close);
  const open = useWorkspaceStore((s) => s.open);
  const opening = useWorkspaceStore((s) => s.opening);

  if (!target) return null;

  const choose = () => {
    if (!listing) return;
    close();
    void open(locationOf(target, listing.path));
  };

  // Until a listing lands there is no path to show, so the machine's own name
  // stands in — including when connecting failed, which is when knowing what
  // the app was trying to reach matters most.
  const machine =
    target.kind === "wsl" ? target.distro : target.kind === "ssh" ? target.host : "this machine";
  const heading = listing?.path ?? (loading ? `Connecting to ${machine}…` : machine);

  return (
    <div className="mt-3 flex flex-col gap-2 rounded border border-border bg-bg p-3 text-left">
      <div className="flex items-baseline justify-between gap-2">
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-text" title={heading}>
          {heading}
        </span>
        <button
          type="button"
          onClick={close}
          aria-label="Close the folder browser"
          className="shrink-0 text-xs text-text-muted hover:text-text"
        >
          ✕
        </button>
      </div>

      {/* A fixed-height placeholder rather than a collapsed panel, so a failed
          connect leaves the box (and the reason under it) where the list was
          about to be. */}
      {!listing && (
        <div className="flex h-48 items-center justify-center rounded border border-border bg-surface text-xs text-text-muted">
          {loading ? "Connecting…" : "Not connected."}
        </div>
      )}
      <ul
        className={`h-48 overflow-y-auto rounded border border-border bg-surface ${
          listing ? "" : "hidden"
        }`}
        aria-busy={loading}
      >
        {listing?.parent !== null && listing?.parent !== undefined && (
          <li>
            <button
              type="button"
              onClick={() => void go(listing.parent)}
              className="w-full px-2 py-1 text-left font-mono text-xs text-text-muted hover:bg-hover hover:text-text"
            >
              ../
            </button>
          </li>
        )}
        {listing?.entries.map((entry) => (
          <li key={entry.path}>
            <button
              type="button"
              onClick={() => void go(entry.path)}
              title={entry.path}
              className="flex w-full items-center gap-2 px-2 py-1 text-left font-mono text-xs text-text hover:bg-hover"
            >
              <span className="min-w-0 flex-1 truncate">{entry.name}/</span>
              {/* Which of twenty directories is the project is the whole
                  question a folder picker exists to answer. */}
              {entry.isRepository && (
                <span className="shrink-0 text-[10px] text-accent" title="git repository">
                  git
                </span>
              )}
            </button>
          </li>
        ))}
        {listing && listing.entries.length === 0 && !loading && (
          <li className="px-2 py-1 text-xs text-text-muted">No sub-folders here.</li>
        )}
      </ul>

      {listing?.truncated && (
        <p className="text-[11px] text-text-muted">
          Only the first 1000 folders are listed. Type the path instead if the one you want is
          missing.
        </p>
      )}
      {error && (
        <p className="text-xs text-danger" role="alert">
          {error}
        </p>
      )}

      <button
        type="button"
        onClick={choose}
        disabled={!listing || opening}
        className="rounded border border-accent px-3 py-1 text-xs font-medium text-accent hover:bg-hover disabled:cursor-not-allowed disabled:opacity-40"
      >
        {opening ? "Opening…" : "Open this folder"}
      </button>
    </div>
  );
}
