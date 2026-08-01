import { useEffect, useState } from "react";
import { useSettingsStore } from "../stores/settingsStore";
import { useTreeStore } from "../stores/treeStore";

/** Turn the textarea's free text into the glob list the backend stores. */
function toGlobs(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

export default function SettingsPanel() {
  const open = useSettingsStore((s) => s.open);
  const settings = useSettingsStore((s) => s.settings);
  const saving = useSettingsStore((s) => s.saving);
  const error = useSettingsStore((s) => s.error);
  const setOpen = useSettingsStore((s) => s.setOpen);
  const save = useSettingsStore((s) => s.save);

  const [text, setText] = useState("");

  // Re-seed the textarea each time the panel opens so an abandoned edit
  // doesn't reappear later as if it had been saved.
  useEffect(() => {
    if (open) setText(settings.extraIgnores.join("\n"));
  }, [open, settings.extraIgnores]);

  if (!open) return null;

  const apply = async () => {
    const saved = await save(toGlobs(text));
    if (!saved) return;
    setOpen(false);
    // The globs change what the tree may show, so everything loaded has to
    // be re-read; the backend has already restarted the watcher.
    await useTreeStore.getState().reloadLoaded();
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/50 pt-24"
      onMouseDown={() => setOpen(false)}
    >
      <div
        className="w-[32rem] max-w-[90vw] rounded border border-border bg-surface p-4 shadow-xl"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <h2 className="text-sm font-semibold text-text">Workspace settings</h2>
        <label className="mt-3 block text-xs text-text-muted" htmlFor="extra-ignores">
          Extra ignore globs — gitignore syntax, one per line. Hidden from the tree, the file jump,
          and the activity feed.
        </label>
        <textarea
          id="extra-ignores"
          value={text}
          onChange={(event) => setText(event.target.value)}
          rows={8}
          spellCheck={false}
          placeholder={"dist/\n*.log\ncoverage/"}
          className="mt-2 w-full resize-y rounded border border-border bg-bg p-2 font-mono text-xs text-text outline-none placeholder:text-text-muted focus:border-glow"
        />
        {error && <p className="mt-2 text-xs text-danger">{error}</p>}
        <div className="mt-3 flex justify-end gap-2">
          <button
            type="button"
            onClick={() => setOpen(false)}
            className="rounded px-3 py-1 text-xs text-text-muted hover:bg-hover hover:text-text"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => void apply()}
            disabled={saving}
            className="rounded bg-selected px-3 py-1 text-xs text-text hover:bg-hover disabled:opacity-50"
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}
