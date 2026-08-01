import { useState } from "react";
import { useGitStore } from "../stores/gitStore";

/**
 * Branch switching and stashing, in the status bar next to the branch name.
 *
 * Stash lives here rather than in the file lists because it's the answer to
 * the error you get from the control beside it: switching with a dirty tree
 * fails, and stash is what unblocks it.
 */
export default function BranchControl() {
  const branches = useGitStore((s) => s.branches);
  const capabilities = useGitStore((s) => s.capabilities);
  const busy = useGitStore((s) => s.busy);
  const switchBranch = useGitStore((s) => s.switchBranch);
  const createBranch = useGitStore((s) => s.createBranch);
  const stashPush = useGitStore((s) => s.stashPush);
  const stashPop = useGitStore((s) => s.stashPop);

  const [open, setOpen] = useState(false);
  const [newName, setNewName] = useState("");

  if (!branches || !capabilities?.canMutate) return null;

  const label = branches.current ?? "detached";

  const create = async () => {
    if (newName.trim().length === 0) return;
    if (await createBranch(newName.trim())) {
      setNewName("");
      setOpen(false);
    }
  };

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setOpen((was) => !was)}
        disabled={busy}
        aria-expanded={open}
        aria-haspopup="menu"
        title="Switch branch, create a branch, or stash"
        className="rounded px-1 text-xs text-text-muted hover:bg-hover hover:text-text disabled:opacity-40"
      >
        ⑂ {label} ▾
      </button>

      {open && (
        <>
          {/* Click-away, so the menu doesn't need a document listener. */}
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} aria-hidden />
          <div
            role="menu"
            className="absolute bottom-full left-0 z-50 mb-1 w-64 rounded border border-border bg-surface p-1 shadow-xl"
          >
            <ul className="max-h-48 overflow-y-auto">
              {branches.branches.map((name) => (
                <li key={name}>
                  <button
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      setOpen(false);
                      void switchBranch(name);
                    }}
                    disabled={name === branches.current}
                    className="w-full truncate rounded px-2 py-1 text-left text-xs text-text hover:bg-hover disabled:text-text-muted disabled:hover:bg-transparent"
                  >
                    {name === branches.current ? `✓ ${name}` : name}
                  </button>
                </li>
              ))}
            </ul>

            <div className="mt-1 border-t border-border pt-1">
              <input
                value={newName}
                onChange={(event) => setNewName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void create();
                  }
                }}
                placeholder="New branch…"
                aria-label="New branch name"
                className="w-full rounded border border-border bg-bg px-2 py-1 text-xs text-text outline-none placeholder:text-text-muted focus:border-glow"
              />
            </div>

            <div className="mt-1 flex gap-1 border-t border-border pt-1">
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setOpen(false);
                  void stashPush();
                }}
                className="flex-1 rounded px-2 py-1 text-xs text-text-muted hover:bg-hover hover:text-text"
              >
                Stash
              </button>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setOpen(false);
                  void stashPop();
                }}
                className="flex-1 rounded px-2 py-1 text-xs text-text-muted hover:bg-hover hover:text-text"
              >
                Pop stash
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
