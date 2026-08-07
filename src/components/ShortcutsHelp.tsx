import { useEffect } from "react";
import { appCommands } from "../lib/commands";
import { useShortcutsStore } from "../stores/shortcutsStore";

export default function ShortcutsHelp() {
  const open = useShortcutsStore((s) => s.open);
  const hide = useShortcutsStore((s) => s.hide);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, hide]);

  if (!open) return null;

  const rows = appCommands().filter((c) => c.keys);

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/50 pt-20"
      onMouseDown={() => hide()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Keyboard shortcuts"
        className="max-h-[80vh] w-[34rem] max-w-[90vw] overflow-y-auto rounded border border-border-strong bg-surface-raised"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <h2 className="text-xs font-medium text-text">Keyboard shortcuts</h2>
          <button
            type="button"
            onClick={() => hide()}
            aria-label="Close shortcuts"
            className="rounded px-2 text-xs text-text-muted hover:bg-hover hover:text-text"
          >
            ✕
          </button>
        </div>

        <table className="w-full text-left text-xs">
          <tbody>
            {rows.map((command) => (
              <tr key={command.id} className="border-b border-border last:border-b-0">
                <td className="px-4 py-2 text-text">{command.title}</td>
                <td className="px-4 py-2 text-right tabular-nums text-text-muted">
                  {command.keys}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
