import { useState } from "react";
import BranchPicker from "./BranchPicker";
import { useGitStore } from "../stores/gitStore";

/**
 * The branch name in the status bar, and the way into checking out another.
 *
 * The picker it opens is the file jump's widget rather than a dropdown of its
 * own: switching branch and jumping to a file are the same gesture — name the
 * thing you already have in mind — and they should not be two different ones.
 */
export default function BranchControl() {
  const branches = useGitStore((s) => s.branches);
  const capabilities = useGitStore((s) => s.capabilities);
  const busy = useGitStore((s) => s.busy);

  const [open, setOpen] = useState(false);

  if (!branches || !capabilities?.canMutate) return null;

  const label = branches.current ?? "detached";

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        disabled={busy}
        aria-haspopup="dialog"
        aria-expanded={open}
        title="Checkout a branch, create one, or stash"
        className="rounded px-1 text-[11px] text-text-muted hover:bg-hover hover:text-text disabled:opacity-40"
      >
        ⑂ {label} ▾
      </button>
      {open && <BranchPicker branches={branches} onClose={() => setOpen(false)} />}
    </>
  );
}
