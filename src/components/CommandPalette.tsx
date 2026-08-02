import { useMemo } from "react";
import QuickPick, { type QuickPickItem } from "./QuickPick";
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
  const hide = usePaletteStore((s) => s.hide);
  const setQuery = usePaletteStore((s) => s.setQuery);

  const results = useMemo(() => fuzzyFilter(files, query, MAX_RESULTS), [files, query]);

  const items: QuickPickItem[] = useMemo(
    () =>
      results.map((result) => ({
        key: result.path,
        node: <Highlighted path={result.path} positions={result.positions} />,
        onChoose: () => {
          hide();
          void useTreeStore.getState().revealPath(result.path);
        },
      })),
    [results, hide],
  );

  if (!open) return null;

  return (
    <QuickPick
      placeholder="Jump to file…"
      query={query}
      items={items}
      loading={loading}
      loadingMessage="Indexing…"
      emptyMessage="No matching files."
      onQueryChange={setQuery}
      onClose={hide}
    />
  );
}

/** Renders the path with matched characters emphasised. */
function Highlighted({ path, positions }: { path: string; positions: number[] }) {
  const marked = new Set(positions);
  return (
    <span className="min-w-0 truncate">
      {Array.from(path).map((char, index) =>
        marked.has(index) ? (
          <span key={index} className="font-medium text-accent">
            {char}
          </span>
        ) : (
          <span key={index}>{char}</span>
        ),
      )}
    </span>
  );
}
