import { useState } from "react";
import { useGitStore } from "../stores/gitStore";
import { useTreeStore } from "../stores/treeStore";
import type { GitFileStatus, GitStatusKind } from "../lib/protocol";

const BADGE_CLASS: Record<GitStatusKind, string> = {
  added: "text-git-added",
  modified: "text-git-modified",
  deleted: "text-git-deleted",
  renamed: "text-git-renamed",
  untracked: "text-git-untracked",
  conflicted: "text-git-conflicted",
};

const BADGE: Record<GitStatusKind, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  untracked: "?",
  conflicted: "!",
};

/**
 * Staged and unstaged work, with the actions needed to get a commit out
 * without leaving the app.
 *
 * Deliberately not a git client: no log graph, no remote operations. The
 * scope line is "the git you need mid-agent-session" — see the phase plan's
 * non-goals — and `git push` is zero clicks away in the terminal already.
 */
export default function GitPanel() {
  const status = useGitStore((s) => s.status);
  const capabilities = useGitStore((s) => s.capabilities);
  const busy = useGitStore((s) => s.busy);
  const error = useGitStore((s) => s.error);
  const dismissError = useGitStore((s) => s.dismissError);

  const [collapsed, setCollapsed] = useState(false);

  if (!status?.isRepository) return null;

  // `null` is "not read yet". Treating it as unavailable would flash a notice
  // saying git is missing every time a workspace opens.
  const known = capabilities !== null;
  const canMutate = capabilities?.canMutate ?? false;

  const staged = status.files.filter((file) => file.staged);
  const unstaged = status.files.filter((file) => !file.staged);

  return (
    <div className="flex max-h-[55%] shrink-0 flex-col border-t border-border">
      <button
        type="button"
        onClick={() => setCollapsed((was) => !was)}
        aria-expanded={!collapsed}
        className="flex shrink-0 items-center gap-1.5 px-2 py-1.5 text-left hover:bg-hover"
      >
        <span
          className={`inline-block w-3 text-text-muted transition-transform ${
            collapsed ? "" : "rotate-90"
          }`}
        >
          ▸
        </span>
        <span className="section-label">Source control</span>
        <span className="ml-auto shrink-0 text-[11px] tabular-nums text-text-muted">
          {staged.length > 0 && <span className="text-git-added">S {staged.length}</span>}
          {staged.length > 0 && unstaged.length > 0 && "  "}
          {unstaged.length > 0 && <span>M {unstaged.length}</span>}
        </span>
      </button>

      {!collapsed && (
        <div className="flex min-h-0 flex-1 flex-col">
          {known && !canMutate && (
            // Read-only rather than buttons that fail on click.
            <p className="px-2 pb-2 text-xs text-text-muted">
              {capabilities?.reason ?? "Git actions unavailable."}
            </p>
          )}

          <div className="min-h-0 flex-1 overflow-y-auto">
            <FileGroup
              label="Staged"
              files={staged}
              action="unstage"
              disabled={!canMutate || busy}
            />
            <FileGroup
              label="Changes"
              files={unstaged}
              action="stage"
              disabled={!canMutate || busy}
            />
            {status.files.length === 0 && (
              <p className="px-2 py-1 text-xs text-text-muted">Nothing to commit.</p>
            )}
          </div>

          {canMutate && <CommitBox stagedCount={staged.length} busy={busy} />}

          {error && (
            // A toast, not a dialog: git's own words, dismissible, and it
            // never blocks the next attempt.
            <div className="flex items-start gap-2 border-t border-border bg-danger/10 px-2 py-1.5">
              <pre className="min-w-0 flex-1 whitespace-pre-wrap break-words font-mono text-xs text-danger">
                {error}
              </pre>
              <button
                type="button"
                onClick={dismissError}
                aria-label="Dismiss error"
                className="shrink-0 text-xs text-text-muted hover:text-text"
              >
                ✕
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function FileGroup({
  label,
  files,
  action,
  disabled,
}: {
  label: string;
  files: GitFileStatus[];
  action: "stage" | "unstage";
  disabled: boolean;
}) {
  const stage = useGitStore((s) => s.stage);
  const unstage = useGitStore((s) => s.unstage);
  const stageAll = useGitStore((s) => s.stageAll);
  const unstageAll = useGitStore((s) => s.unstageAll);
  const selected = useTreeStore((s) => s.selected);

  if (files.length === 0) return null;

  const apply = (paths: string[]) => (action === "stage" ? stage(paths) : unstage(paths));
  const applyAll = () => (action === "stage" ? stageAll() : unstageAll());
  const verb = action === "stage" ? "Stage" : "Unstage";

  return (
    <section>
      <div className="flex items-center gap-2 px-2 py-1">
        <h3 className="section-label">{label}</h3>
        <button
          type="button"
          onClick={() => void applyAll()}
          disabled={disabled}
          className="ml-auto text-[11px] text-text-muted hover:text-accent disabled:opacity-40"
        >
          {verb} all
        </button>
      </div>
      <ul>
        {files.map((file) => (
          <li key={`${action}:${file.path}`}>
            <div
              className={`group flex items-center gap-1.5 px-2 py-0.5 text-xs ${
                selected === file.path ? "bg-selected" : "hover:bg-hover"
              }`}
            >
              <span
                className={`w-3 shrink-0 text-center text-[11px] tabular-nums ${BADGE_CLASS[file.status]}`}
              >
                {BADGE[file.status]}
              </span>
              <button
                type="button"
                onClick={() => void useTreeStore.getState().revealPath(file.path)}
                title={file.path}
                className="min-w-0 flex-1 truncate text-left text-text-body"
              >
                {file.path}
              </button>
              <button
                type="button"
                onClick={() => void apply([file.path])}
                disabled={disabled}
                title={`${verb} ${file.path}`}
                aria-label={`${verb} ${file.path}`}
                className="shrink-0 px-1 text-[11px] text-text-muted opacity-0 hover:text-accent group-hover:opacity-100 disabled:opacity-0"
              >
                {action === "stage" ? "+" : "−"}
              </button>
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}

function CommitBox({ stagedCount, busy }: { stagedCount: number; busy: boolean }) {
  const commit = useGitStore((s) => s.commit);
  const [message, setMessage] = useState("");
  const [amend, setAmend] = useState(false);

  // Amending rewrites the previous commit, so it's the one case where an
  // empty staging area is still a valid thing to do.
  const canCommit = message.trim().length > 0 && (stagedCount > 0 || amend) && !busy;

  const submit = async () => {
    if (!canCommit) return;
    if (await commit(message, amend)) {
      setMessage("");
      setAmend(false);
    }
  };

  return (
    <div className="shrink-0 border-t border-border bg-surface-raised p-2">
      <textarea
        value={message}
        onChange={(event) => setMessage(event.target.value)}
        onKeyDown={(event) => {
          // Ctrl/Cmd+Enter commits — Enter alone must still insert a newline,
          // since commit bodies are multi-line.
          if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
            event.preventDefault();
            void submit();
          }
        }}
        rows={2}
        placeholder={amend ? "Reword the previous commit…" : "Commit message…"}
        aria-label="Commit message"
        className="w-full resize-y rounded border border-border bg-surface p-1.5 text-xs text-text outline-none placeholder:text-text-muted focus:border-accent"
      />
      <div className="mt-1.5 flex items-center gap-2">
        <label className="flex cursor-pointer items-center gap-1 text-[11px] text-text-muted">
          <input
            type="checkbox"
            checked={amend}
            onChange={(event) => setAmend(event.target.checked)}
            className="h-3 w-3 accent-accent"
          />
          Amend
        </label>
        <button
          type="button"
          onClick={() => void submit()}
          disabled={!canCommit}
          title="Commit staged changes (Ctrl+Enter)"
          className="ml-auto h-7 rounded border border-accent px-3 text-[11px] font-medium text-accent hover:bg-hover disabled:opacity-40"
        >
          {busy
            ? "Working…"
            : amend
              ? "Amend"
              : `Commit${stagedCount > 0 ? ` ${stagedCount}` : ""}`}
        </button>
      </div>
    </div>
  );
}
