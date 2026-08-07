import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  clampPreviewFontSize,
  clampTheme,
  clampZoom,
  DEFAULT_PREVIEW_FONT_SIZE,
  DEFAULT_THEME,
  DEFAULT_ZOOM,
  MAX_ZOOM,
  MIN_ZOOM,
  resolveTheme,
  stepZoom,
  useAppearanceStore,
} from "./appearanceStore";

/** Must match `STORAGE_KEY` in the store; tests clear it so cases don't share persistence. */
const STORAGE_KEY = "agentlens.appearance";

describe("clampZoom", () => {
  it("keeps a value that is already a stop", () => {
    expect(clampZoom(1.25)).toBe(1.25);
  });

  it("snaps a value from between stops to the nearest one", () => {
    expect(clampZoom(1.2)).toBe(1.25);
    expect(clampZoom(1.13)).toBe(1.1);
  });

  it("clamps beyond either end of the ladder", () => {
    expect(clampZoom(0.1)).toBe(MIN_ZOOM);
    expect(clampZoom(12)).toBe(MAX_ZOOM);
  });

  it("falls back to 100% for a non-finite zoom", () => {
    expect(clampZoom(Number.NaN)).toBe(DEFAULT_ZOOM);
  });
});

describe("stepZoom", () => {
  it("moves one stop at a time", () => {
    expect(stepZoom(1, 1)).toBe(1.1);
    expect(stepZoom(1, -1)).toBe(0.9);
  });

  it("holds at the ends rather than wrapping", () => {
    expect(stepZoom(MIN_ZOOM, -1)).toBe(MIN_ZOOM);
    expect(stepZoom(MAX_ZOOM, 1)).toBe(MAX_ZOOM);
  });
});

describe("clampPreviewFontSize", () => {
  it("clamps to the allowed range and rounds", () => {
    expect(clampPreviewFontSize(2)).toBe(9);
    expect(clampPreviewFontSize(99)).toBe(24);
    expect(clampPreviewFontSize(13.4)).toBe(13);
  });

  it("falls back to the default for a non-finite size", () => {
    expect(clampPreviewFontSize(Number.NaN)).toBe(DEFAULT_PREVIEW_FONT_SIZE);
  });
});

describe("clampTheme", () => {
  it("accepts the three named modes", () => {
    expect(clampTheme("system")).toBe("system");
    expect(clampTheme("dark")).toBe("dark");
    expect(clampTheme("light")).toBe("light");
  });

  it("falls back for anything else", () => {
    expect(clampTheme("auto")).toBe(DEFAULT_THEME);
    expect(clampTheme(null)).toBe(DEFAULT_THEME);
    expect(clampTheme(1)).toBe(DEFAULT_THEME);
  });
});

function stubMatchMedia(matches: boolean): void {
  // Node vitest has no window; resolveTheme reads window.matchMedia.
  vi.stubGlobal("window", {
    matchMedia: (query: string) => ({
      matches,
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
      onchange: null,
    }),
  });
}

/** Minimal in-memory localStorage for the node test environment. */
function stubLocalStorage(initial: Record<string, string> = {}): Map<string, string> {
  const map = new Map(Object.entries(initial));
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => {
      map.set(key, value);
    },
    removeItem: (key: string) => {
      map.delete(key);
    },
    clear: () => map.clear(),
  });
  return map;
}

describe("resolveTheme", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns absolute modes as-is", () => {
    expect(resolveTheme("dark")).toBe("dark");
    expect(resolveTheme("light")).toBe("light");
  });

  it("picks light when matchMedia reports a light preference", () => {
    stubMatchMedia(true);
    expect(resolveTheme("system")).toBe("light");
  });

  it("picks dark when matchMedia reports a dark preference", () => {
    stubMatchMedia(false);
    expect(resolveTheme("system")).toBe("dark");
  });
});

describe("useAppearanceStore", () => {
  beforeEach(() => {
    // Node vitest has no DOM storage; give the store somewhere to write.
    stubLocalStorage();
    useAppearanceStore.setState({
      zoom: DEFAULT_ZOOM,
      previewFontSize: DEFAULT_PREVIEW_FONT_SIZE,
      theme: DEFAULT_THEME,
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("defaults theme to dark", () => {
    expect(useAppearanceStore.getState().theme).toBe("dark");
  });

  it("steps and resets the zoom", () => {
    useAppearanceStore.getState().zoomIn();
    expect(useAppearanceStore.getState().zoom).toBe(1.1);
    useAppearanceStore.getState().zoomOut();
    useAppearanceStore.getState().zoomOut();
    expect(useAppearanceStore.getState().zoom).toBe(0.9);
    useAppearanceStore.getState().resetZoom();
    expect(useAppearanceStore.getState().zoom).toBe(DEFAULT_ZOOM);
  });

  it("clamps a preview font size set through the store", () => {
    useAppearanceStore.getState().setPreviewFontSize(16);
    expect(useAppearanceStore.getState().previewFontSize).toBe(16);

    useAppearanceStore.getState().setPreviewFontSize(400);
    expect(useAppearanceStore.getState().previewFontSize).toBe(24);
  });

  it("persists setTheme", () => {
    useAppearanceStore.getState().setTheme("light");
    expect(useAppearanceStore.getState().theme).toBe("light");
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as { theme?: string };
    expect(stored.theme).toBe("light");
  });

  it("falls back when a stored theme is corrupt", async () => {
    stubLocalStorage({
      [STORAGE_KEY]: JSON.stringify({ zoom: 1, previewFontSize: 12, theme: "neon" }),
    });
    vi.resetModules();
    const { useAppearanceStore: reloaded } = await import("./appearanceStore");
    expect(reloaded.getState().theme).toBe(DEFAULT_THEME);
  });
});
