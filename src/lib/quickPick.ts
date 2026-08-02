/**
 * How many quick-pick overlays are on screen, so a global shortcut can
 * decline to open one on top of another.
 *
 * A module counter rather than a store flag: it is a fact about what is
 * mounted, every picker maintains it simply by existing, and a picker added
 * later gets the behaviour without anyone having to remember to wire it up.
 */
let mounted = 0;

/** Count an open picker. The returned function is its unmount cleanup. */
export function registerQuickPick(): () => void {
  mounted += 1;
  return () => {
    mounted -= 1;
  };
}

export function quickPickOpen(): boolean {
  return mounted > 0;
}
