import { useMemo } from "react";
import QuickPick, { type QuickPickItem } from "./QuickPick";
import { appCommands } from "../lib/commands";
import { usePaletteStore } from "../stores/paletteStore";
import { usePreviewStore } from "../stores/previewStore";
import { useTreeStore } from "../stores/treeStore";
import { fuzzyFilter } from "../lib/fuzzy";

/** Results rendered at once; the list is a jump target, not a browser. */
const MAX_RESULTS = 50;

export default function CommandPalette() {
  const open = usePaletteStore((s) => s.open);
  const kind = usePaletteStore((s) => s.kind);
  const files = usePaletteStore((s) => s.files);
  const loading = usePaletteStore((s) => s.loading);
  const query = usePaletteStore((s) => s.query);
  const hide = usePaletteStore((s) => s.hide);
  const setQuery = usePaletteStore((s) => s.setQuery);

  const fileResults = useMemo(
    () => (kind === "files" ? fuzzyFilter(files, query, MAX_RESULTS) : []),
    [kind, files, query],
  );

  const commandResults = useMemo(() => {
    if (kind !== "commands") return [];
    const enabled = appCommands().filter((c) => c.enabled?.() ?? true);
    // fuzzyFilter scores paths; titles are short enough that the same matcher works.
    const titles = enabled.map((c) => c.title);
    const matches = fuzzyFilter(titles, query, titles.length || 1);
    return matches.map((match) => {
      const command = enabled.find((c) => c.title === match.path)!;
      return { command, positions: match.positions };
    });
  }, [kind, query]);

  const items: QuickPickItem[] = useMemo(() => {
    if (kind === "commands") {
      return commandResults.map(({ command, positions }) => ({
        key: command.id,
        node: (
          <span className="flex min-w-0 flex-1 items-center justify-between gap-3">
            <Highlighted text={command.title} positions={positions} />
            {command.keys && (
              <span className="shrink-0 text-[11px] text-text-muted">{command.keys}</span>
            )}
          </span>
        ),
        onChoose: () => {
          hide();
          void command.run();
        },
      }));
    }

    return fileResults.map((result) => ({
      key: result.path,
      node: <Highlighted text={result.path} positions={result.positions} />,
      onChoose: () => {
        hide();
        void useTreeStore.getState().revealPath(result.path);
        // Jump targets are intentional: keep a permanent tab, like VS Code.
        void usePreviewStore.getState().openPermanent(result.path);
      },
    }));
  }, [kind, fileResults, commandResults, hide]);

  if (!open) return null;

  return (
    <QuickPick
      placeholder={kind === "commands" ? "Run a command…" : "Jump to file…"}
      query={query}
      items={items}
      loading={kind === "files" && loading}
      loadingMessage="Indexing…"
      emptyMessage={kind === "commands" ? "No matching commands." : "No matching files."}
      onQueryChange={setQuery}
      onClose={hide}
    />
  );
}

/** Renders text with matched characters emphasised. */
function Highlighted({ text, positions }: { text: string; positions: number[] }) {
  const marked = new Set(positions);
  return (
    <span className="min-w-0 truncate">
      {Array.from(text).map((char, index) =>
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
