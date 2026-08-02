import { useEffect, useRef, useState } from "react";
import { DEFAULT_FEED_MAX_ENTRIES, MAX_FEED_MAX_ENTRIES, MIN_FEED_MAX_ENTRIES } from "../lib/feed";
import {
  DEFAULT_PREVIEW_FONT_SIZE,
  MAX_PREVIEW_FONT_SIZE,
  MIN_PREVIEW_FONT_SIZE,
  useAppearanceStore,
} from "../stores/appearanceStore";
import { useConnectionStore } from "../stores/connectionStore";
import { useSettingsStore } from "../stores/settingsStore";
import type { AgentKind } from "../lib/protocol";

/** Turn a textarea's free text into the list the backend stores. */
function toLines(text: string): string[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

export default function SettingsPanel() {
  const open = useSettingsStore((s) => s.open);
  const settings = useSettingsStore((s) => s.settings);
  const app = useSettingsStore((s) => s.app);
  const saving = useSettingsStore((s) => s.saving);
  const error = useSettingsStore((s) => s.error);
  const setOpen = useSettingsStore((s) => s.setOpen);
  const save = useSettingsStore((s) => s.save);
  const setPinned = useSettingsStore((s) => s.setPinned);
  const toggleShowIgnored = useSettingsStore((s) => s.toggleShowIgnored);
  const toggleShowAgentContext = useSettingsStore((s) => s.toggleShowAgentContext);
  const toggleAutoInstallDaemon = useSettingsStore((s) => s.toggleAutoInstallDaemon);
  const setFeedMaxEntries = useSettingsStore((s) => s.setFeedMaxEntries);

  // Esc closes; there is nothing to lose by closing, since every control here
  // has already applied.
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, setOpen]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/50 pt-20"
      onMouseDown={() => setOpen(false)}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        className="max-h-[80vh] w-[34rem] max-w-[90vw] overflow-y-auto rounded border border-border-strong bg-surface-raised"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-border px-4 py-3">
          <h2 className="text-xs font-medium text-text">Settings</h2>
          <button
            type="button"
            onClick={() => setOpen(false)}
            aria-label="Close settings"
            className="rounded px-2 text-xs text-text-muted hover:bg-hover hover:text-text"
          >
            ✕
          </button>
        </div>

        {/* Sections name what a setting affects, not where it is stored — and
            showing both at once is what teaches the scope model. */}
        <Section title="This workspace">
          <Toggle
            label="Show git-ignored files"
            description="Reveal everything .gitignore hides. The escape hatch, not the everyday setting."
            checked={settings.showIgnored}
            disabled={saving}
            onChange={() => void toggleShowIgnored()}
          />
          <TextRows
            id="pinned-paths"
            label="Pinned paths"
            description="Always visible and grouped at the top of the tree. Pin from a tree row; this is for bulk edits."
            placeholder={"notes/\nTODO.md"}
            value={settings.pinned}
            onCommit={(lines) => void setPinned(lines)}
          />
          <TextRows
            id="extra-ignores"
            label="Extra ignore globs"
            description="Gitignore syntax, one per line. Hidden from the tree, the file jump, and the activity feed."
            placeholder={"dist/\n*.log\ncoverage/"}
            value={settings.extraIgnores}
            onCommit={(lines) => void save(lines)}
          />
        </Section>

        <Section title="All workspaces">
          <Toggle
            label="Show agent context files"
            description="Surface AGENTS.md, CLAUDE.md and friends even when git ignores them."
            checked={app.showAgentContext}
            disabled={saving}
            onChange={() => void toggleShowAgentContext()}
          />
          <UiZoom />
          <PreviewFontSize />
          <FeedMaxEntries
            value={app.feedMaxEntries}
            disabled={saving}
            onCommit={(n) => void setFeedMaxEntries(n)}
          />
          <AgentRoots />
        </Section>

        <Section title="Remote">
          <Toggle
            label="Set up remote machines automatically"
            description="Install the AgentLens observer on a WSL distro or SSH host that hasn't got one, instead of failing with instructions."
            checked={app.autoInstallDaemon}
            disabled={saving}
            onChange={() => void toggleAutoInstallDaemon()}
          />
          <DaemonCommand />
        </Section>

        {error && (
          <p className="border-t border-border px-4 py-2 text-xs text-danger" role="alert">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}

const AGENT_LABEL: Record<AgentKind, string> = {
  claudeCode: "Claude Code",
  opencode: "opencode",
};

/**
 * Where agent sessions are looked for, and the escape hatch for adding more.
 *
 * The read-only list above the field is the point of this row. Detection is a
 * guess — an agent's storage layout is a convention its authors never
 * promised — so when no sessions show up, the only useful question is "what is
 * it actually looking at". Without this, "no agent detected" is a dead end.
 */
function AgentRoots() {
  const roots = useSettingsStore((s) => s.roots);
  const configured = useSettingsStore((s) => s.app.agentRoots);
  const setAgentRoots = useSettingsStore((s) => s.setAgentRoots);

  return (
    <div>
      <p className="text-xs font-medium text-text">Agent session folders</p>
      <p className="text-xs text-text-muted">
        Searched for coding-agent sessions. Detected automatically; add your own if a profile lives
        somewhere unusual.
      </p>

      <ul className="mt-1.5 space-y-0.5 rounded border border-border bg-surface p-2">
        {roots.length === 0 && <li className="text-xs text-text-muted">None found</li>}
        {roots.map((root) => (
          <li key={root.path} className="flex items-baseline gap-2 text-xs">
            <span className="min-w-0 flex-1 truncate text-text-body" title={root.path}>
              {root.path}
            </span>
            {root.agent ? (
              <span className="shrink-0 text-text-muted">{AGENT_LABEL[root.agent]}</span>
            ) : (
              // A path the user typed that nothing recognises is exactly why
              // they'd see no sessions, so it says so instead of vanishing.
              <span className="shrink-0 text-danger" title="No agent recognises this folder">
                not recognised
              </span>
            )}
            {!root.detected && <span className="shrink-0 text-text-muted">added</span>}
          </li>
        ))}
      </ul>

      <TextRows
        id="agent-roots"
        placeholder={"/opt/claude-profiles/work"}
        rows={2}
        value={configured}
        onCommit={(lines) => void setAgentRoots(lines)}
      />
    </div>
  );
}

/**
 * What AgentLens runs on the far side of a WSL or SSH connection.
 *
 * This exists because of one specific, very common failure: `ssh host cmd`
 * runs without a login shell, so `~/.local/bin` is usually not on `PATH` and
 * a perfectly well-installed daemon reports "command not found". An absolute
 * path here is the fix, and only the user knows where they put it.
 */
function DaemonCommand() {
  const command = useSettingsStore((s) => s.app.daemonCommand);
  const setDaemonCommand = useSettingsStore((s) => s.setDaemonCommand);
  const connection = useConnectionStore((s) => s.info);

  return (
    <div>
      <label className="block text-xs font-medium text-text" htmlFor="daemon-command">
        Daemon command
      </label>
      <p className="text-xs text-text-muted">
        Leave this alone unless AgentLens can't work it out. It normally finds or installs the
        observer itself; naming a command here runs exactly that instead. Applies to the next
        connection.
      </p>
      <TextField
        id="daemon-command"
        placeholder="agentlens-daemon"
        value={command}
        onCommit={(value) => void setDaemonCommand(value)}
      />
      {connection.remote && (
        <p className="mt-1.5 text-xs text-text-muted">
          Connected to {connection.label}
          {connection.daemonVersion ? ` · daemon v${connection.daemonVersion}` : ""}
        </p>
      )}
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="border-b border-border px-4 py-3 last:border-b-0">
      <h3 className="section-label pb-2">{title}</h3>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  );
}

/**
 * Whole-window scale. Duplicated here from `Ctrl +/-/0` because a setting
 * nobody can find isn't one — and someone who has zoomed too far to read the
 * footer chip needs a way back that doesn't depend on remembering a shortcut.
 */
function UiZoom() {
  const zoom = useAppearanceStore((s) => s.zoom);
  const zoomIn = useAppearanceStore((s) => s.zoomIn);
  const zoomOut = useAppearanceStore((s) => s.zoomOut);
  const resetZoom = useAppearanceStore((s) => s.resetZoom);

  return (
    <div>
      <span className="block text-xs font-medium text-text">Interface zoom</span>
      <p className="text-xs text-text-muted">
        Scales the whole window — tree, feed, preview and footer alike. Also on{" "}
        <kbd className="text-text">Ctrl</kbd> <kbd className="text-text">+</kbd> /{" "}
        <kbd className="text-text">−</kbd> / <kbd className="text-text">0</kbd>.
      </p>
      <div className="mt-1.5 flex items-center gap-1">
        <ZoomButton label="Zoom out" onClick={zoomOut}>
          −
        </ZoomButton>
        <span className="w-14 text-center text-xs tabular-nums text-text">
          {Math.round(zoom * 100)}%
        </span>
        <ZoomButton label="Zoom in" onClick={zoomIn}>
          +
        </ZoomButton>
        <button
          type="button"
          onClick={resetZoom}
          className="ml-2 h-8 rounded border border-border px-2.5 text-xs text-text-muted hover:bg-hover hover:text-text"
        >
          Reset
        </button>
      </div>
    </div>
  );
}

function ZoomButton({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      className="h-8 w-8 rounded border border-border text-xs text-text hover:bg-hover"
    >
      {children}
    </button>
  );
}

/**
 * Preview text size, independent of the zoom above: the chrome is deliberately
 * dense (DESIGN.md), but the preview is the pane you actually read, and the
 * two wants don't have to agree.
 */
function PreviewFontSize() {
  const size = useAppearanceStore((s) => s.previewFontSize);
  const setPreviewFontSize = useAppearanceStore((s) => s.setPreviewFontSize);
  const [text, setText] = useState(String(size));

  useEffect(() => {
    setText(String(size));
  }, [size]);

  const commit = () => {
    const parsed = Number.parseInt(text, 10);
    if (!Number.isFinite(parsed)) {
      setText(String(size));
      return;
    }
    setPreviewFontSize(parsed);
  };

  return (
    <div>
      <label className="block text-xs font-medium text-text" htmlFor="preview-font-size">
        Preview text size
      </label>
      <p className="text-xs text-text-muted">
        Code, diffs and rendered markdown in the preview pane, in pixels. Default{" "}
        {DEFAULT_PREVIEW_FONT_SIZE}; range {MIN_PREVIEW_FONT_SIZE}–{MAX_PREVIEW_FONT_SIZE}. The
        interface zoom above multiplies it.
      </p>
      <input
        id="preview-font-size"
        type="number"
        min={MIN_PREVIEW_FONT_SIZE}
        max={MAX_PREVIEW_FONT_SIZE}
        step={1}
        value={text}
        onChange={(event) => setText(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            (event.target as HTMLInputElement).blur();
          }
        }}
        className="mt-1.5 h-8 w-28 rounded border border-border bg-surface px-2.5 text-xs text-text outline-none placeholder:text-text-muted focus:border-accent"
      />
    </div>
  );
}

/**
 * How many activity batches to keep. Live window only — not full history —
 * so the unvirtualized feed stays bounded.
 */
function FeedMaxEntries({
  value,
  disabled,
  onCommit,
}: {
  value: number;
  disabled: boolean;
  onCommit: (n: number) => void;
}) {
  const [text, setText] = useState(String(value));

  useEffect(() => {
    setText(String(value));
  }, [value]);

  const commit = () => {
    const parsed = Number.parseInt(text, 10);
    if (!Number.isFinite(parsed)) {
      setText(String(value));
      return;
    }
    if (parsed === value) return;
    onCommit(parsed);
  };

  return (
    <div>
      <label className="block text-xs font-medium text-text" htmlFor="feed-max-entries">
        Activity feed length
      </label>
      <p className="text-xs text-text-muted">
        Max batches kept in the feed (oldest drop off). Default {DEFAULT_FEED_MAX_ENTRIES}; range{" "}
        {MIN_FEED_MAX_ENTRIES}–{MAX_FEED_MAX_ENTRIES}. Does not change the session +/− totals in the
        footer.
      </p>
      <input
        id="feed-max-entries"
        type="number"
        min={MIN_FEED_MAX_ENTRIES}
        max={MAX_FEED_MAX_ENTRIES}
        step={1}
        value={text}
        disabled={disabled}
        onChange={(event) => setText(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            (event.target as HTMLInputElement).blur();
          }
        }}
        className="mt-1.5 h-8 w-28 rounded border border-border bg-surface px-2.5 text-xs text-text outline-none placeholder:text-text-muted focus:border-accent disabled:opacity-40"
      />
    </div>
  );
}

/** Label, one-line description, control right-aligned. Applies immediately. */
function Toggle({
  label,
  description,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  disabled: boolean;
  onChange: () => void;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-3">
      <span className="min-w-0 flex-1">
        <span className="block text-xs font-medium text-text">{label}</span>
        <span className="block text-xs text-text-muted">{description}</span>
      </span>
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={onChange}
        className="mt-0.5 h-4 w-4 shrink-0 accent-accent"
      />
    </label>
  );
}

/** A single-line value, committed on blur for the same reason as `TextRows`. */
function TextField({
  id,
  placeholder,
  value,
  onCommit,
}: {
  id: string;
  placeholder: string;
  value: string;
  onCommit: (value: string) => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  const [text, setText] = useState(value);

  useEffect(() => {
    if (ref.current !== document.activeElement) setText(value);
  }, [value]);

  return (
    <input
      id={id}
      ref={ref}
      value={text}
      onChange={(event) => setText(event.target.value)}
      onBlur={() => {
        if (text.trim() !== value) onCommit(text);
      }}
      spellCheck={false}
      placeholder={placeholder}
      className="mt-1.5 h-8 w-full rounded border border-border bg-surface px-2.5 text-xs text-text outline-none placeholder:text-text-muted focus:border-accent"
    />
  );
}

/**
 * A one-per-line list. Text fields apply on blur rather than instantly: a
 * half-typed glob would otherwise take effect on every keystroke.
 */
function TextRows({
  id,
  label,
  description,
  placeholder,
  value,
  rows = 4,
  onCommit,
}: {
  id: string;
  label?: string;
  description?: string;
  placeholder: string;
  rows?: number;
  value: string[];
  onCommit: (lines: string[]) => void;
}) {
  // Keyed on the joined text, not the array: the backend hands back a fresh
  // array on every save, so an identity dep would re-sync after writes that
  // never touched this field.
  const stored = value.join("\n");
  const ref = useRef<HTMLTextAreaElement>(null);
  const [text, setText] = useState(stored);

  // Follow the stored value when it changes underneath — a pin toggled from a
  // tree row, or a fresh workspace — but never mid-edit: the field commits
  // itself on blur, and overwriting it here would discard live keystrokes.
  useEffect(() => {
    if (ref.current !== document.activeElement) setText(stored);
  }, [stored]);

  return (
    <div>
      {label && (
        <label className="block text-xs font-medium text-text" htmlFor={id}>
          {label}
        </label>
      )}
      {description && <p className="text-xs text-text-muted">{description}</p>}
      <textarea
        id={id}
        ref={ref}
        value={text}
        onChange={(event) => setText(event.target.value)}
        onBlur={() => {
          const lines = toLines(text);
          if (lines.join("\n") !== stored) onCommit(lines);
        }}
        rows={rows}
        spellCheck={false}
        placeholder={placeholder}
        className="mt-1.5 w-full resize-y rounded border border-border bg-surface p-2.5 text-xs text-text outline-none placeholder:text-text-muted focus:border-accent"
      />
    </div>
  );
}
