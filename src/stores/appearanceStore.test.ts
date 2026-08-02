import { beforeEach, describe, expect, it } from "vitest";
import {
  clampPreviewFontSize,
  clampZoom,
  DEFAULT_PREVIEW_FONT_SIZE,
  DEFAULT_ZOOM,
  MAX_ZOOM,
  MIN_ZOOM,
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

describe("useAppearanceStore", () => {
  beforeEach(() => {
    // jsdom provides this; the node environment the pure helpers run under does not.
    if (typeof localStorage !== "undefined") localStorage.removeItem(STORAGE_KEY);
    useAppearanceStore.setState({
      zoom: DEFAULT_ZOOM,
      previewFontSize: DEFAULT_PREVIEW_FONT_SIZE,
    });
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
});
