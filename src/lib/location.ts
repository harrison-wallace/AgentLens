import type { ConnectionTarget } from "./protocol";

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
