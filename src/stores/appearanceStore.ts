import { create } from "zustand";

/**
 * UI zoom and preview text size. Persisted in `localStorage` alongside the
 * panel layout rather than in the Tauri store: it is window chrome, it has to
 * be readable synchronously on first paint, and it describes *this* display —
 * a size that suits a 4K laptop panel shouldn't follow the user to a monitor.
 */
const STORAGE_KEY = "agentlens.appearance";

/**
 * Discrete zoom stops. A multiplier per keypress drifts to values like 1.331
 * and never comes back to exactly 100%; a fixed ladder always lands somewhere
 * nameable, and the ends are the clamp.
 */
export const ZOOM_STEPS = [0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2] as const;
export const DEFAULT_ZOOM = 1;
export const MIN_ZOOM = ZOOM_STEPS[0];
export const MAX_ZOOM = ZOOM_STEPS.at(-1) ?? DEFAULT_ZOOM;

/** Preview body text, in CSS px before the UI zoom above multiplies it. */
export const MIN_PREVIEW_FONT_SIZE = 9;
export const MAX_PREVIEW_FONT_SIZE = 24;
export const DEFAULT_PREVIEW_FONT_SIZE = 12;

export type ThemeMode = "system" | "dark" | "light";
export type ResolvedTheme = "dark" | "light";

export const DEFAULT_THEME: ThemeMode = "dark";

export interface AppearanceState {
  /** Scale factor applied to the whole webview, chrome included. */
  zoom: number;
  /** Code/prose size inside the preview pane only. */
  previewFontSize: number;
  /** Preferred colour scheme; `"system"` follows the OS. */
  theme: ThemeMode;
}

const DEFAULTS: AppearanceState = {
  zoom: DEFAULT_ZOOM,
  previewFontSize: DEFAULT_PREVIEW_FONT_SIZE,
  theme: DEFAULT_THEME,
};

/** Snap to the nearest stop, so a stored value from another build still works. */
export function clampZoom(zoom: number): number {
  if (!Number.isFinite(zoom)) return DEFAULT_ZOOM;
  return ZOOM_STEPS.reduce((best, step) =>
    Math.abs(step - zoom) < Math.abs(best - zoom) ? step : best,
  );
}

/** One stop along the ladder; the ends hold rather than wrap. */
export function stepZoom(zoom: number, direction: 1 | -1): number {
  const index = ZOOM_STEPS.indexOf(clampZoom(zoom) as (typeof ZOOM_STEPS)[number]);
  const next = Math.min(ZOOM_STEPS.length - 1, Math.max(0, index + direction));
  return ZOOM_STEPS[next] ?? DEFAULT_ZOOM;
}

export function clampPreviewFontSize(size: number): number {
  if (!Number.isFinite(size)) return DEFAULT_PREVIEW_FONT_SIZE;
  return Math.min(MAX_PREVIEW_FONT_SIZE, Math.max(MIN_PREVIEW_FONT_SIZE, Math.round(size)));
}

function isThemeMode(value: unknown): value is ThemeMode {
  return value === "system" || value === "dark" || value === "light";
}

/** Accept only the three named modes; anything else is the default. */
export function clampTheme(value: unknown): ThemeMode {
  return isThemeMode(value) ? value : DEFAULT_THEME;
}

/** Resolve `"system"` against the OS preference; absolute modes pass through. */
export function resolveTheme(theme: ThemeMode): ResolvedTheme {
  if (theme === "dark" || theme === "light") return theme;
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return "dark";
  }
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

function load(): AppearanceState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<AppearanceState>;
    return {
      zoom: clampZoom(parsed.zoom ?? DEFAULTS.zoom),
      previewFontSize: clampPreviewFontSize(parsed.previewFontSize ?? DEFAULTS.previewFontSize),
      theme: clampTheme(parsed.theme),
    };
  } catch {
    // Corrupt or unavailable storage must not stop the app rendering.
    return DEFAULTS;
  }
}

function persist(state: AppearanceState): void {
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        zoom: state.zoom,
        previewFontSize: state.previewFontSize,
        theme: state.theme,
      }),
    );
  } catch {
    // Ignore: losing a zoom level is not worth surfacing.
  }
}

/**
 * Zoom the webview itself rather than scaling CSS. The UI is built from fixed
 * px values (11px captions, 28px rows) by design, so only the webview's own
 * scale factor moves all of it together — including native scrollbars.
 */
function applyZoom(zoom: number): void {
  void import("@tauri-apps/api/webview")
    .then(({ getCurrentWebview }) => getCurrentWebview().setZoom(zoom))
    .catch(() => {
      // No webview (tests, a plain browser) or the platform refused: the
      // stored value stands, the window just doesn't scale.
    });
}

/** The preview reads its size from CSS, so one variable drives every rule. */
function applyPreviewFontSize(size: number): void {
  // Guarded for the same reason `applyZoom` swallows its failure: the store's
  // logic is unit-tested without a DOM, and the value it holds is still right.
  if (typeof document === "undefined") return;
  document.documentElement.style.setProperty("--preview-font-size", `${size}px`);
}

/**
 * Push the resolved theme at the document and the native title bar. The
 * dataset value is always `"dark"` or `"light"` — never `"system"` — so CSS
 * only has two ramps to consider.
 */
function applyTheme(theme: ThemeMode): void {
  if (typeof document === "undefined") return;
  const resolved = resolveTheme(theme);
  document.documentElement.dataset.theme = resolved;
  void import("@tauri-apps/api/window")
    .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(resolved))
    .catch(() => {
      // No window (tests, a plain browser) or the platform refused.
    });
}

/** One listener for the OS preference; re-created only when mode is system. */
let systemThemeMql: MediaQueryList | null = null;
let systemThemeHandler: (() => void) | null = null;

function clearSystemThemeListener(): void {
  if (systemThemeMql && systemThemeHandler) {
    systemThemeMql.removeEventListener("change", systemThemeHandler);
  }
  systemThemeMql = null;
  systemThemeHandler = null;
}

/**
 * When the mode is `"system"`, re-apply on OS preference changes. Idempotent:
 * a second `apply()` tears down any previous listener first.
 */
function syncSystemThemeListener(theme: ThemeMode): void {
  clearSystemThemeListener();
  if (theme !== "system") return;
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
  const mql = window.matchMedia("(prefers-color-scheme: light)");
  const handler = () => applyTheme("system");
  mql.addEventListener("change", handler);
  systemThemeMql = mql;
  systemThemeHandler = handler;
}

interface AppearanceStore extends AppearanceState {
  zoomIn: () => void;
  zoomOut: () => void;
  resetZoom: () => void;
  setPreviewFontSize: (size: number) => void;
  setTheme: (theme: ThemeMode) => void;
  /** Push the persisted values at the webview and the document, on startup. */
  apply: () => void;
}

export const useAppearanceStore = create<AppearanceStore>((set, get) => {
  const commit = (patch: Partial<AppearanceState>) => {
    const next = { ...get(), ...patch };
    persist({
      zoom: next.zoom,
      previewFontSize: next.previewFontSize,
      theme: next.theme,
    });
    if (patch.zoom !== undefined) applyZoom(next.zoom);
    if (patch.previewFontSize !== undefined) applyPreviewFontSize(next.previewFontSize);
    if (patch.theme !== undefined) {
      applyTheme(next.theme);
      syncSystemThemeListener(next.theme);
    }
    set(patch);
  };

  return {
    ...load(),
    zoomIn: () => commit({ zoom: stepZoom(get().zoom, 1) }),
    zoomOut: () => commit({ zoom: stepZoom(get().zoom, -1) }),
    resetZoom: () => commit({ zoom: DEFAULT_ZOOM }),
    setPreviewFontSize: (size) => commit({ previewFontSize: clampPreviewFontSize(size) }),
    setTheme: (theme) => commit({ theme: clampTheme(theme) }),
    apply: () => {
      applyZoom(get().zoom);
      applyPreviewFontSize(get().previewFontSize);
      applyTheme(get().theme);
      syncSystemThemeListener(get().theme);
    },
  };
});
