import { describe, expect, it } from "vitest";
import { clampWidth, useLayoutStore, visiblePanels, type LayoutState } from "./layoutStore";

function state(overrides: Partial<LayoutState>): LayoutState {
  return {
    treeWidth: 320,
    feedWidth: 320,
    treeCollapsed: false,
    feedCollapsed: false,
    previewCollapsed: false,
    ...overrides,
  };
}

describe("clampWidth", () => {
  it("keeps a sensible width unchanged", () => {
    expect(clampWidth(320)).toBe(320);
  });

  it("clamps to the allowed range and rounds", () => {
    expect(clampWidth(10)).toBe(180);
    expect(clampWidth(5000)).toBe(640);
    expect(clampWidth(320.6)).toBe(321);
  });

  it("falls back to the default for a non-finite width", () => {
    expect(clampWidth(Number.NaN)).toBe(320);
  });
});

describe("visiblePanels", () => {
  it("counts the panels still on screen", () => {
    expect(visiblePanels(state({}))).toBe(3);
    expect(visiblePanels(state({ treeCollapsed: true }))).toBe(2);
    expect(visiblePanels(state({ treeCollapsed: true, feedCollapsed: true }))).toBe(1);
  });
});

describe("useLayoutStore", () => {
  it("refuses to collapse the last visible panel", () => {
    const store = useLayoutStore.getState();
    store.toggleTree();
    store.toggleFeed();
    expect(visiblePanels(useLayoutStore.getState())).toBe(1);
    expect(useLayoutStore.getState().previewCollapsed).toBe(false);

    // The preview is all that's left; hiding it would blank the window.
    useLayoutStore.getState().togglePreview();
    expect(useLayoutStore.getState().previewCollapsed).toBe(false);
    expect(visiblePanels(useLayoutStore.getState())).toBe(1);

    useLayoutStore.getState().toggleTree();
    useLayoutStore.getState().toggleFeed();
  });

  it("clamps widths set through the store", () => {
    useLayoutStore.getState().setTreeWidth(5);
    expect(useLayoutStore.getState().treeWidth).toBe(180);
    useLayoutStore.getState().setFeedWidth(320);
  });
});
