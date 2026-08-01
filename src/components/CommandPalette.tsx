import { useEffect, useMemo, useRef } from "react";
import { usePaletteStore } from "../stores/paletteStore";
import { useTreeStore } from "../stores/treeStore";
import { fuzzyFilter } from "../lib/fuzzy";

/** Results rendered at once; the list is a jump target, not a browser. */
const MAX_RESULTS = 50;

export default function CommandPalette() {
  const open = usePaletteStore((s) => s.open);
  const files = usePaletteStore((s) => s.files);
  const loading = usePaletteStore((s) => s.loading);
  const query = usePaletteStore((s) => s.query);
  const cursor = usePaletteStore((s) => s.cursor);
  const hide = usePaletteStore((s) => s.hide);
  const setQuery = usePaletteStore((s) => s.setQuery);
  const moveCursor = usePaletteStore((s) => s.moveCursor);

  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const results = useMemo(() => fuzzyFilter(files, query, MAX_RESULTS), [files, query]);

  useEffect(() => {
    if (open) inputRef.current?.focus();
  }, [open]);

  // Keep the highlighted row in view when arrowing past the visible window.
  useEffect(() => {
    listRef.current?.children[cursor]?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  if (!open) return null;

  const choose = (path: string) => {
    hide();
    void useTreeStore.getState().revealPath(path);
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      hide();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      moveCursor(1, results.length);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      moveCursor(-1, results.length);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const chosen = results[cursor];
      if (chosen) choose(chosen.path);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/50 pt-24"
      onMouseDown={hide}
    >
      <div
        className="w-[36rem] max-w-[90vw] overflow-hidden rounded border border-border bg-surface shadow-xl"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Jump to file…"
          spellCheck={false}
          className="w-full bg-transparent px-3 py-2 text-sm text-text outline-none placeholder:text-text-muted"
        />
        <div className="max-h-80 overflow-y-auto border-t border-border">
          {loading && <p className="px-3 py-2 text-xs text-text-muted">Indexing…</p>}
          {!loading && results.length === 0 && (
            <p className="px-3 py-2 text-xs text-text-muted">No matching files.</p>
          )}
          <ul ref={listRef}>
            {results.map((result, index) => (
              <li key={result.path}>
                <button
                  type="button"
                  onClick={() => choose(result.path)}
                  onMouseMove={() => {
                    // Guarded: mousemove fires continuously within one row.
                    if (index !== cursor) moveCursor(index - cursor, results.length);
                  }}
                  className={`flex w-full items-baseline gap-2 px-3 py-1 text-left text-sm ${
                    index === cursor ? "bg-selected text-text" : "text-text hover:bg-hover"
                  }`}
                >
                  <Highlighted path={result.path} positions={result.positions} />
                </button>
              </li>
            ))}
          </ul>
        </div>
      </div>
    </div>
  );
}

/** Renders the path with matched characters emphasised. */
function Highlighted({ path, positions }: { path: string; positions: number[] }) {
  const marked = new Set(positions);
  return (
    <span className="min-w-0 truncate">
      {Array.from(path).map((char, index) =>
        marked.has(index) ? (
          <span key={index} className="font-semibold text-glow">
            {char}
          </span>
        ) : (
          <span key={index}>{char}</span>
        ),
      )}
    </span>
  );
}
