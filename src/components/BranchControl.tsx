import BranchPicker from "./BranchPicker";
import { useBranchPickerStore } from "../stores/branchPickerStore";
import { useGitStore } from "../stores/gitStore";

/**
 * The branch name in the status bar, and the way into checking out another.
 *
 * The picker it opens is the file jump's widget rather than a dropdown of its
 * own: switching branch and jumping to a file are the same gesture — name the
 * thing you already have in mind — and they should not be two different ones.
 * Open state lives in a store so the command palette can open the same picker.
 */
export default function BranchControl() {
  const branches = useGitStore((s) => s.branches);
  const capabilities = useGitStore((s) => s.capabilities);
  const busy = useGitStore((s) => s.busy);
  const open = useBranchPickerStore((s) => s.open);
  const show = useBranchPickerStore((s) => s.show);
  const hide = useBranchPickerStore((s) => s.hide);

  if (!branches || !capabilities?.canMutate) return null;

  const label = branches.current ?? "detached";

  return (
    <>
      <button
        type="button"
        onClick={() => show()}
        disabled={busy}
        aria-haspopup="dialog"
        aria-expanded={open}
        title="Checkout a branch, create one, or stash"
        className="rounded px-1 text-[11px] text-text-muted hover:bg-hover hover:text-text disabled:opacity-40"
      >
        ⑂ {label} ▾
      </button>
      {open && <BranchPicker branches={branches} onClose={() => hide()} />}
    </>
  );
}
