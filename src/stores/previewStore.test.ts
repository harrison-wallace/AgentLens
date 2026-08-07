import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PreviewPayload, SessionDiff } from "../lib/protocol";

const textPayload = (path: string): PreviewPayload => ({
  kind: "text",
  path,
  text: `// ${path}`,
  language: "typescript",
});

const emptyDiff = (path: string, tag = ""): SessionDiff => ({
  path,
  baseline: tag,
  current: tag,
  unavailable: null,
});

vi.mock("../lib/tauri", () => ({
  readPreview: vi.fn(async (path: string) => textPayload(path)),
  sessionDiff: vi.fn(async (path: string) => emptyDiff(path, "session")),
  gitDiff: vi.fn(async (path: string, staged: boolean) =>
    emptyDiff(path, staged ? "staged" : "working"),
  ),
}));

const { usePreviewStore, MAX_OPEN_TABS } = await import("./previewStore");
const { readPreview, sessionDiff, gitDiff } = await import("../lib/tauri");

/** Minimal localStorage so persistence tests run under node. */
function installMemoryStorage() {
  const map = new Map<string, string>();
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
}

describe("previewStore tabs", () => {
  beforeEach(() => {
    installMemoryStorage();
    usePreviewStore.getState().reset();
    vi.clearAllMocks();
  });

  it("openPreview creates a non-permanent tab and loads content", async () => {
    await usePreviewStore.getState().openPreview("src/a.ts");
    const state = usePreviewStore.getState();
    expect(state.tabs).toEqual([{ path: "src/a.ts", permanent: false, mode: "current" }]);
    expect(state.activePath).toBe("src/a.ts");
    expect(state.payload).toEqual(textPayload("src/a.ts"));
    expect(vi.mocked(readPreview)).toHaveBeenCalledWith("src/a.ts");
  });

  it("a second openPreview replaces the preview tab rather than stacking", async () => {
    await usePreviewStore.getState().openPreview("src/a.ts");
    await usePreviewStore.getState().openPreview("src/b.ts");
    const state = usePreviewStore.getState();
    expect(state.tabs.map((t) => t.path)).toEqual(["src/b.ts"]);
    expect(state.tabs[0]?.permanent).toBe(false);
    expect(state.activePath).toBe("src/b.ts");
  });

  it("openPermanent keeps tabs and promotes a preview", async () => {
    await usePreviewStore.getState().openPreview("src/a.ts");
    await usePreviewStore.getState().openPermanent("src/a.ts");
    await usePreviewStore.getState().openPermanent("src/b.ts");
    const state = usePreviewStore.getState();
    expect(state.tabs).toEqual([
      { path: "src/a.ts", permanent: true, mode: "current" },
      { path: "src/b.ts", permanent: true, mode: "current" },
    ]);
  });

  it("closing the active tab activates a neighbour", async () => {
    await usePreviewStore.getState().openPermanent("a.ts");
    await usePreviewStore.getState().openPermanent("b.ts");
    await usePreviewStore.getState().openPermanent("c.ts");
    await usePreviewStore.getState().activate("b.ts");
    await usePreviewStore.getState().close("b.ts");
    expect(usePreviewStore.getState().activePath).toBe("c.ts");
    expect(usePreviewStore.getState().tabs.map((t) => t.path)).toEqual(["a.ts", "c.ts"]);
  });

  it("nextTab and prevTab cycle", async () => {
    await usePreviewStore.getState().openPermanent("a.ts");
    await usePreviewStore.getState().openPermanent("b.ts");
    await usePreviewStore.getState().openPermanent("c.ts");
    await usePreviewStore.getState().activate("a.ts");
    await usePreviewStore.getState().nextTab();
    expect(usePreviewStore.getState().activePath).toBe("b.ts");
    await usePreviewStore.getState().prevTab();
    expect(usePreviewStore.getState().activePath).toBe("a.ts");
  });

  it("setMode loads the diff side", async () => {
    await usePreviewStore.getState().openPermanent("a.ts");
    await usePreviewStore.getState().setMode("diff");
    expect(usePreviewStore.getState().tabs[0]?.mode).toBe("diff");
    expect(vi.mocked(sessionDiff)).toHaveBeenCalledWith("a.ts");
    expect(usePreviewStore.getState().diff).toEqual(emptyDiff("a.ts", "session"));
  });

  it("caches the three diff modes independently for the same path", async () => {
    await usePreviewStore.getState().openPermanent("a.ts");
    await usePreviewStore.getState().setMode("diff");
    await usePreviewStore.getState().setMode("gitWorking");
    await usePreviewStore.getState().setMode("gitStaged");

    expect(vi.mocked(sessionDiff)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(gitDiff)).toHaveBeenCalledWith("a.ts", false);
    expect(vi.mocked(gitDiff)).toHaveBeenCalledWith("a.ts", true);
    expect(usePreviewStore.getState().diff).toEqual(emptyDiff("a.ts", "staged"));

    vi.mocked(sessionDiff).mockClear();
    vi.mocked(gitDiff).mockClear();

    await usePreviewStore.getState().setMode("diff");
    expect(vi.mocked(sessionDiff)).not.toHaveBeenCalled();
    expect(usePreviewStore.getState().diff).toEqual(emptyDiff("a.ts", "session"));

    await usePreviewStore.getState().setMode("gitWorking");
    expect(vi.mocked(gitDiff)).not.toHaveBeenCalled();
    expect(usePreviewStore.getState().diff).toEqual(emptyDiff("a.ts", "working"));
  });

  it("caches content so reactivating does not re-fetch", async () => {
    await usePreviewStore.getState().openPermanent("a.ts");
    await usePreviewStore.getState().openPermanent("b.ts");
    vi.mocked(readPreview).mockClear();
    await usePreviewStore.getState().activate("a.ts");
    expect(vi.mocked(readPreview)).not.toHaveBeenCalled();
    expect(usePreviewStore.getState().payload).toEqual(textPayload("a.ts"));
  });

  it("persists and restores tabs for a workspace key", async () => {
    await usePreviewStore.getState().bindWorkspace("/proj");
    await usePreviewStore.getState().openPermanent("src/a.ts");
    await usePreviewStore.getState().openPermanent("src/b.ts");
    await usePreviewStore.getState().setMode("diff");

    usePreviewStore.getState().reset();
    await usePreviewStore.getState().bindWorkspace("/proj");

    const state = usePreviewStore.getState();
    expect(state.tabs.map((t) => t.path)).toEqual(["src/a.ts", "src/b.ts"]);
    expect(state.activePath).toBe("src/b.ts");
    expect(state.tabs.find((t) => t.path === "src/b.ts")?.mode).toBe("diff");
  });

  it("falls back unknown persisted modes to current", async () => {
    localStorage.setItem(
      "agentlens.open-tabs",
      JSON.stringify({
        "/proj": {
          tabs: [{ path: "a.ts", permanent: true, mode: "legacyMode" }],
          active: "a.ts",
        },
      }),
    );
    await usePreviewStore.getState().bindWorkspace("/proj");
    expect(usePreviewStore.getState().tabs[0]?.mode).toBe("current");
  });

  it("keeps separate tab sets for the same root on different machines", async () => {
    await usePreviewStore.getState().bindWorkspace("ssh://box/home/h/proj");
    await usePreviewStore.getState().openPermanent("src/remote.ts");

    await usePreviewStore.getState().bindWorkspace("ssh://other/home/h/proj");
    expect(usePreviewStore.getState().tabs).toEqual([]);
    await usePreviewStore.getState().openPermanent("src/other.ts");

    await usePreviewStore.getState().bindWorkspace("ssh://box/home/h/proj");
    expect(usePreviewStore.getState().tabs.map((t) => t.path)).toEqual(["src/remote.ts"]);
  });

  it("caps the number of open tabs", async () => {
    for (let i = 0; i < MAX_OPEN_TABS + 3; i++) {
      await usePreviewStore.getState().openPermanent(`f${i}.ts`);
    }
    expect(usePreviewStore.getState().tabs.length).toBe(MAX_OPEN_TABS);
    expect(usePreviewStore.getState().tabs[0]?.path).toBe("f3.ts");
  });
});
