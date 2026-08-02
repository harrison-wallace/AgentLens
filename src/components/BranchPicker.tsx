import { useMemo, useState } from "react";
import QuickPick, { type QuickPickItem } from "./QuickPick";
import { useGitStore } from "../stores/gitStore";
import { fuzzyFilter } from "../lib/fuzzy";
import type { BranchList } from "../lib/protocol";

/** Branches rendered at once; the field narrows the list faster than scrolling. */
const MAX_RESULTS = 50;

/**
 * Type-to-filter branch checkout, in the same overlay as the file jump.
 *
 * A dropdown made the user scroll for a branch they could already name. The
 * one gesture here — type, Enter — checks out an existing branch or creates
 * the one they just typed, so naming a branch that doesn't exist yet isn't a
 * separate control.
 */
export default function BranchPicker({
  branches,
  onClose,
}: {
  branches: BranchList;
  onClose: () => void;
}) {
  const busy = useGitStore((s) => s.busy);
  const switchBranch = useGitStore((s) => s.switchBranch);
  const createBranch = useGitStore((s) => s.createBranch);
  const stashPush = useGitStore((s) => s.stashPush);
  const stashPop = useGitStore((s) => s.stashPop);
  const [query, setQuery] = useState("");

  const name = query.trim();
  const results = useMemo(
    () => fuzzyFilter(branches.branches, query, MAX_RESULTS),
    [branches.branches, query],
  );

  const items: QuickPickItem[] = useMemo(() => {
    const rows: QuickPickItem[] = results.map((result) => ({
      key: `branch:${result.path}`,
      node: <BranchRow name={result.path} current={result.path === branches.current} />,
      onChoose: () => {
        onClose();
        // Choosing the branch already checked out is a no-op, not an error.
        if (result.path !== branches.current) void switchBranch(result.path);
      },
    }));

    // Only when the name is new: offering to create a branch that exists
    // would be a second, worse way to check it out.
    if (name.length > 0 && !branches.branches.includes(name)) {
      rows.unshift({
        key: "create",
        node: (
          <span className="min-w-0 truncate">
            <span className="text-text-muted">Create branch </span>
            <span className="font-medium text-accent">{name}</span>
          </span>
        ),
        onChoose: () => {
          onClose();
          void createBranch(name);
        },
      });
    }

    return rows;
  }, [results, name, branches, onClose, switchBranch, createBranch]);

  return (
    <QuickPick
      placeholder="Checkout branch, or type a new name…"
      query={query}
      items={items}
      emptyMessage="No matching branches."
      onQueryChange={setQuery}
      onClose={onClose}
      footer={
        // Stash keeps its place beside branch switching: it is still the
        // answer to a switch this list can't complete on a dirty tree.
        <div className="flex gap-1 border-t border-border p-1">
          <StashButton
            label="Stash"
            disabled={busy}
            onClick={() => {
              onClose();
              void stashPush();
            }}
          />
          <StashButton
            label="Pop stash"
            disabled={busy}
            onClick={() => {
              onClose();
              void stashPop();
            }}
          />
        </div>
      }
    />
  );
}

function BranchRow({ name, current }: { name: string; current: boolean }) {
  return (
    <>
      <span className={`w-3 shrink-0 ${current ? "text-accent" : "text-transparent"}`}>✓</span>
      <span className={`min-w-0 truncate ${current ? "text-text" : ""}`}>{name}</span>
    </>
  );
}

function StashButton({
  label,
  disabled,
  onClick,
}: {
  label: string;
  disabled: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="flex-1 rounded px-2 py-1 text-[11px] text-text-muted hover:bg-hover hover:text-text disabled:opacity-40"
    >
      {label}
    </button>
  );
}
