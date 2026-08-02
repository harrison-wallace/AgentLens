import { useEffect, useMemo, useState } from "react";
import PreviewTabs from "./PreviewTabs";
import { usePreviewStore, type PreviewMode } from "../stores/previewStore";
import {
  collapseContext,
  diffUnavailableReason,
  MAX_DIFF_ROWS,
  summarizeDiff,
  toDiffRows,
  truncateDisplay,
} from "../lib/diff";
import { highlightToHtml } from "../lib/highlight";
import { canRenderRich, formatBytes } from "../lib/preview";
import { renderMarkdown } from "../lib/markdown";
import { openExternally } from "../lib/tauri";
import type { PreviewPayload, SessionDiff } from "../lib/protocol";

/** Unchanged lines kept either side of a change in the diff view. */
const DIFF_CONTEXT_LINES = 3;

export default function Preview() {
  const activePath = usePreviewStore((s) => s.activePath);
  const tabs = usePreviewStore((s) => s.tabs);
  const payload = usePreviewStore((s) => s.payload);
  const diff = usePreviewStore((s) => s.diff);
  const loading = usePreviewStore((s) => s.loading);
  const error = usePreviewStore((s) => s.error);
  const setMode = usePreviewStore((s) => s.setMode);

  const mode: PreviewMode = tabs.find((t) => t.path === activePath)?.mode ?? "current";

  if (!activePath) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-center text-xs text-text-muted">
        Select a file to preview it.
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PreviewTabs />
      <div className="flex h-7 shrink-0 items-center gap-1 border-b border-border px-2">
        <ModeButton active={mode === "current"} onClick={() => void setMode("current")}>
          Current
        </ModeButton>
        <ModeButton active={mode === "diff"} onClick={() => void setMode("diff")}>
          Diff since session
        </ModeButton>
        <span
          className="mx-2 min-w-0 flex-1 truncate text-[11px] text-text-muted"
          title={activePath}
        >
          {activePath}
        </span>
        <OpenExternallyButton path={activePath} />
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {error && <p className="p-4 text-xs text-danger">{error}</p>}
        {!error && loading && <p className="p-4 text-xs text-text-muted">Loading…</p>}
        {!error && !loading && mode === "current" && payload && <CurrentView payload={payload} />}
        {!error && !loading && mode === "diff" && diff && <DiffView diff={diff} />}
      </div>
    </div>
  );
}

/** Handing a file to the OS can fail (no handler, file gone); the failure
 * belongs next to the button rather than as an unhandled rejection. */
function OpenExternallyButton({ path }: { path: string }) {
  const [failed, setFailed] = useState(false);

  const open = async () => {
    setFailed(false);
    try {
      await openExternally(path);
    } catch {
      setFailed(true);
    }
  };

  return (
    <button
      type="button"
      onClick={() => void open()}
      title={failed ? "Could not open this file externally" : undefined}
      className={`shrink-0 rounded px-2 py-0.5 text-[11px] hover:bg-hover hover:text-text ${
        failed ? "text-danger" : "text-text-muted"
      }`}
    >
      {failed ? "Couldn't open" : "Open externally"}
    </button>
  );
}

function ModeButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`shrink-0 rounded px-2 py-0.5 text-[11px] ${
        active ? "bg-selected text-text" : "text-text-muted hover:bg-hover hover:text-text"
      }`}
    >
      {children}
    </button>
  );
}

function CurrentView({ payload }: { payload: PreviewPayload }) {
  switch (payload.kind) {
    case "image":
      return (
        <div className="flex h-full items-center justify-center p-4">
          <img
            src={`data:${payload.mime};base64,${payload.base64}`}
            alt={payload.path}
            className="max-h-full max-w-full object-contain"
          />
        </div>
      );
    case "binary":
      return (
        <p className="p-4 text-xs text-text-muted">
          Binary file ({formatBytes(payload.size)}) — nothing to show.
        </p>
      );
    case "tooLarge":
      return (
        <p className="p-4 text-xs text-text-muted">
          File is too large to preview ({formatBytes(payload.size)}).
        </p>
      );
    case "text":
      return <TextPreview text={payload.text} language={payload.language} />;
  }
}

function TextPreview({ text, language }: { text: string; language: string }) {
  const [html, setHtml] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setHtml(null);

    // Past the size cap the file is shown as plain text: one React text node
    // instead of a span per token.
    if (!canRenderRich(text)) return;

    const render = language === "markdown" ? renderMarkdown(text) : highlightToHtml(text, language);
    void render.then((result) => {
      // The user can select another file while a grammar is still loading;
      // dropping the stale result keeps the wrong content off screen.
      if (!cancelled) setHtml(result);
    });

    return () => {
      cancelled = true;
    };
  }, [text, language]);

  // Rendered markdown and highlighted code both contain anchors. Following
  // one would navigate the whole webview away from the app, so clicks on
  // links are swallowed; the URL stays visible via the title attribute.
  const swallowLinks = (event: React.MouseEvent<HTMLDivElement>) => {
    if ((event.target as HTMLElement).closest("a")) event.preventDefault();
  };

  if (html === null) {
    return <pre className="preview-text p-3 leading-relaxed text-text">{text}</pre>;
  }

  return (
    <div
      onClick={swallowLinks}
      className={language === "markdown" ? "markdown-body p-4" : "shiki-body p-1"}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

function DiffView({ diff }: { diff: SessionDiff }) {
  const rows = useMemo(() => toDiffRows(diff.baseline, diff.current), [diff]);
  const { display, truncated } = useMemo(
    () => truncateDisplay(collapseContext(rows, DIFF_CONTEXT_LINES), MAX_DIFF_ROWS),
    [rows],
  );
  const summary = useMemo(() => summarizeDiff(rows), [rows]);

  const reason = diffUnavailableReason(diff);
  if (reason) return <p className="p-4 text-xs text-text-muted">{reason}</p>;

  if (summary.added === 0 && summary.removed === 0) {
    return <p className="p-4 text-xs text-text-muted">No changes since the session started.</p>;
  }

  return (
    <div className="preview-text leading-relaxed">
      <p className="border-b border-border px-3 py-1 tabular-nums text-text-muted">
        <span className="text-git-added">+ {summary.added}</span>{" "}
        <span className="text-git-deleted">− {summary.removed}</span>
      </p>
      {display.map((entry, index) =>
        entry.type === "gap" ? (
          <p key={`gap:${index}`} className="bg-hover px-3 py-0.5 text-text-muted">
            ⋯ {entry.hidden} unchanged {entry.hidden === 1 ? "line" : "lines"}
          </p>
        ) : (
          <div
            key={`row:${index}`}
            className={`flex gap-2 px-3 ${
              entry.row.kind === "added"
                ? "bg-diff-added"
                : entry.row.kind === "removed"
                  ? "bg-diff-removed"
                  : ""
            }`}
          >
            <span className="w-10 shrink-0 select-none text-right text-text-muted">
              {entry.row.baselineLine ?? ""}
            </span>
            <span className="w-10 shrink-0 select-none text-right text-text-muted">
              {entry.row.currentLine ?? ""}
            </span>
            <span className="w-3 shrink-0 select-none text-text-muted">
              {entry.row.kind === "added" ? "+" : entry.row.kind === "removed" ? "−" : " "}
            </span>
            <span className="whitespace-pre-wrap break-all">{entry.row.text}</span>
          </div>
        ),
      )}
      {truncated > 0 && (
        <p className="border-t border-border px-3 py-2 text-text-muted">
          Diff truncated — {truncated} more {truncated === 1 ? "row" : "rows"} not shown.
        </p>
      )}
    </div>
  );
}
