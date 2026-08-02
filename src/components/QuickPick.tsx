import { useEffect, useRef, useState } from "react";
import { registerQuickPick } from "../lib/quickPick";

/** One selectable row. `node` is the row's content, already formatted. */
export interface QuickPickItem {
  key: string;
  node: React.ReactNode;
  onChoose: () => void;
}

/**
 * The filter-and-pick overlay: a query field, a keyboard-driven list, and an
 * optional footer for actions that aren't list items.
 *
 * Extracted from the file jump so branch checkout can be the same gesture
 * rather than a lookalike. The cursor lives here rather than in a store —
 * it's a property of the open widget, and two pickers must not share one.
 */
export default function QuickPick({
  placeholder,
  query,
  items,
  loading,
  loadingMessage = "Loading…",
  emptyMessage,
  onQueryChange,
  onClose,
  footer,
}: {
  placeholder: string;
  query: string;
  items: QuickPickItem[];
  loading?: boolean;
  loadingMessage?: string;
  emptyMessage: string;
  onQueryChange: (query: string) => void;
  onClose: () => void;
  footer?: React.ReactNode;
}) {
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    return registerQuickPick();
  }, []);

  // Typing invalidates the previous selection, so the cursor goes back to the
  // best match rather than to whatever happens to sit at that index now.
  useEffect(() => {
    setCursor(0);
  }, [query]);

  // Items can shrink without the query changing (branch list refresh, create
  // row disappearing). Keep the highlight on a real row rather than past the end.
  useEffect(() => {
    setCursor((at) => (items.length === 0 ? 0 : Math.min(at, items.length - 1)));
  }, [items.length]);

  // Keep the highlighted row in view when arrowing past the visible window.
  useEffect(() => {
    listRef.current?.children[cursor]?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  const move = (delta: number) => {
    if (items.length === 0) return;
    setCursor((at) => (at + delta + items.length) % items.length);
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      move(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      move(-1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      items[cursor]?.onChoose();
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/50 pt-24"
      onMouseDown={onClose}
    >
      <div
        className="w-[36rem] max-w-[90vw] overflow-hidden rounded border border-border-strong bg-surface-raised"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(event) => onQueryChange(event.target.value)}
          onKeyDown={onKeyDown}
          placeholder={placeholder}
          aria-label={placeholder}
          spellCheck={false}
          className="w-full bg-transparent px-3 py-2 text-xs text-text outline-none placeholder:text-text-muted"
        />
        <div className="max-h-80 overflow-y-auto border-t border-border">
          {loading && <p className="px-3 py-2 text-[11px] text-text-muted">{loadingMessage}</p>}
          {!loading && items.length === 0 && (
            <p className="px-3 py-2 text-[11px] text-text-muted">{emptyMessage}</p>
          )}
          <ul ref={listRef}>
            {items.map((item, index) => (
              <li key={item.key}>
                <button
                  type="button"
                  onClick={item.onChoose}
                  onMouseMove={() => {
                    // Guarded: mousemove fires continuously within one row.
                    if (index !== cursor) setCursor(index);
                  }}
                  className={`flex w-full items-baseline gap-2 px-3 py-1 text-left text-xs ${
                    index === cursor ? "bg-selected text-text" : "text-text-body hover:bg-hover"
                  }`}
                >
                  {item.node}
                </button>
              </li>
            ))}
          </ul>
        </div>
        {footer}
      </div>
    </div>
  );
}
