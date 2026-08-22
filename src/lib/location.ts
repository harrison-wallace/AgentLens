import type { ConnectionState, ConnectionTarget } from "./protocol";

/** True when both targets name the same machine. */
export function sameTarget(a: ConnectionTarget, b: ConnectionTarget): boolean {
  switch (a.kind) {
    case "local":
      return b.kind === "local";
    case "wsl":
      return b.kind === "wsl" && a.distro === b.distro;
    case "ssh":
      return b.kind === "ssh" && a.host === b.host;
  }
}

/**
 * App settings that are per host (extra agent-session folders) must be
 * re-read only once the new backend is in place. Connecting/Installing
 * events name the remote target while the previous backend is still
 * answering, so a refresh then would load the wrong machine's list.
 */
export function shouldRefreshHostSettings(
  prev: { target: ConnectionTarget; state: ConnectionState },
  next: { target: ConnectionTarget; state: ConnectionState },
): boolean {
  if (next.state !== "connected") return false;
  return prev.state !== "connected" || !sameTarget(prev.target, next.target);
}

/**
 * Canonical location string for a path on a given machine — same shape the
 * backend uses for settings and recents (`wsl://…`, `ssh://…`, or a bare
 * local path). Frontend persistence that must not collide across remotes
 * (open tabs, etc.) keys on this rather than `workspace.root` alone.
 */
export function formatLocation(target: ConnectionTarget, root: string): string {
  switch (target.kind) {
    case "local":
      return root;
    case "wsl": {
      const path = root.startsWith("/") ? root : `/${root}`;
      return `wsl://${target.distro}${path}`;
    }
    case "ssh": {
      const path = root.startsWith("/") ? root : `/${root}`;
      return `ssh://${target.host}${path}`;
    }
  }
}
