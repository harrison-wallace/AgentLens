import { create } from "zustand";

/**
 * Panel widths and collapse state. Persisted in `localStorage` rather than
 * the Tauri store: it's pure window chrome, it has to be readable
 * synchronously on first paint to avoid a layout flash, and it isn't worth a
 * backend round-trip.
 */
const STORAGE_KEY = "agentlens.layout";

const MIN_WIDTH = 180;
const MAX_WIDTH = 640;

export interface LayoutState {
  treeWidth: number;
  feedWidth: number;
  treeCollapsed: boolean;
  feedCollapsed: boolean;
  previewCollapsed: boolean;
  /** Collapsed state of the agent session list above the feed. */
  sessionsCollapsed: boolean;
}

const DEFAULTS: LayoutState = {
  treeWidth: 320,
  feedWidth: 320,
  treeCollapsed: false,
  feedCollapsed: false,
  previewCollapsed: false,
  sessionsCollapsed: false,
};

export function clampWidth(width: number): number {
  if (!Number.isFinite(width)) return DEFAULTS.treeWidth;
  return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(width)));
}

function load(): LayoutState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<LayoutState>;
    return {
      treeWidth: clampWidth(parsed.treeWidth ?? DEFAULTS.treeWidth),
      feedWidth: clampWidth(parsed.feedWidth ?? DEFAULTS.feedWidth),
      treeCollapsed: parsed.treeCollapsed ?? false,
      feedCollapsed: parsed.feedCollapsed ?? false,
      previewCollapsed: parsed.previewCollapsed ?? false,
      sessionsCollapsed: parsed.sessionsCollapsed ?? false,
    };
  } catch {
    // Corrupt or unavailable storage must not stop the app rendering.
    return DEFAULTS;
  }
}

/**
 * Coalesce writes. Dragging a splitter updates the width on every pointermove,
 * and `localStorage.setItem` is synchronous — writing per frame puts disk I/O
 * on the main thread for the whole drag. The final value is what matters.
 */
const PERSIST_DELAY_MS = 300;
let persistTimer: ReturnType<typeof setTimeout> | undefined;

function persist(state: LayoutState): void {
  clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    } catch {
      // Ignore: losing panel sizes is not worth surfacing.
    }
  }, PERSIST_DELAY_MS);
}

/** How many of the three panels a state leaves on screen. */
export function visiblePanels(state: LayoutState): number {
  return (
    Number(!state.treeCollapsed) + Number(!state.previewCollapsed) + Number(!state.feedCollapsed)
  );
}

interface LayoutStore extends LayoutState {
  setTreeWidth: (width: number) => void;
  setFeedWidth: (width: number) => void;
  toggleTree: () => void;
  toggleFeed: () => void;
  togglePreview: () => void;
  toggleSessions: () => void;
}

export const useLayoutStore = create<LayoutStore>((set, get) => {
  const commit = (patch: Partial<LayoutState>) => {
    const next = { ...get(), ...patch };
    // Collapsing the last visible panel would leave an empty window with no
    // hint of how to recover, so the final one stays put.
    if (visiblePanels(next) === 0) return;
    const {
      treeWidth,
      feedWidth,
      treeCollapsed,
      feedCollapsed,
      previewCollapsed,
      sessionsCollapsed,
    } = next;
    persist({
      treeWidth,
      feedWidth,
      treeCollapsed,
      feedCollapsed,
      previewCollapsed,
      sessionsCollapsed,
    });
    set(patch);
  };

  return {
    ...load(),
    setTreeWidth: (width) => commit({ treeWidth: clampWidth(width) }),
    setFeedWidth: (width) => commit({ feedWidth: clampWidth(width) }),
    toggleTree: () => commit({ treeCollapsed: !get().treeCollapsed }),
    toggleFeed: () => commit({ feedCollapsed: !get().feedCollapsed }),
    togglePreview: () => commit({ previewCollapsed: !get().previewCollapsed }),
    toggleSessions: () => commit({ sessionsCollapsed: !get().sessionsCollapsed }),
  };
});
