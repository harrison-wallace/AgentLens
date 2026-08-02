import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings, WorkspaceSettings } from "../lib/protocol";

/**
 * Stand-in backend. The real one is the source of truth for both scopes and
 * echoes back what it stored, so the fake does the same — that echo is what
 * these round-trips are checking the store honours.
 */
const backend = {
  workspace: { extraIgnores: [], showIgnored: false, pinned: [] } as WorkspaceSettings,
  app: { showAgentContext: true, agentRoots: [], daemonCommand: "agentlens-daemon" } as AppSettings,
};

vi.mock("../lib/tauri", () => ({
  workspaceSettings: vi.fn(async () => backend.workspace),
  setWorkspaceSettings: vi.fn(async (value: WorkspaceSettings) => {
    backend.workspace = value;
    return value;
  }),
  appSettings: vi.fn(async () => backend.app),
  setAppSettings: vi.fn(async (value: AppSettings) => {
    backend.app = value;
    return value;
  }),
  agentRoots: vi.fn(async () => []),
  pinnedEntries: vi.fn(async () =>
    backend.workspace.pinned.map((path) => ({
      path,
      name: path.split("/").pop() ?? path,
      isDir: false,
      exists: true,
    })),
  ),
}));

// The store re-reads the tree after every save; nothing here depends on it.
vi.mock("./treeStore", () => ({
  useTreeStore: { getState: () => ({ reloadLoaded: vi.fn(async () => undefined) }) },
}));

const { useSettingsStore } = await import("./settingsStore");

describe("settingsStore", () => {
  beforeEach(() => {
    backend.workspace = { extraIgnores: [], showIgnored: false, pinned: [] };
    backend.app = { showAgentContext: true, agentRoots: [], daemonCommand: "agentlens-daemon" };
    useSettingsStore.setState({
      settings: { extraIgnores: [], showIgnored: false, pinned: [] },
      app: { showAgentContext: true, agentRoots: [], daemonCommand: "agentlens-daemon" },
      pins: [],
      error: null,
      saving: false,
    });
  });

  it("round-trips a pin through the backend and the resolved group", async () => {
    expect(await useSettingsStore.getState().togglePin("notes/drafts")).toBe(true);

    expect(backend.workspace.pinned).toEqual(["notes/drafts"]);
    expect(useSettingsStore.getState().settings.pinned).toEqual(["notes/drafts"]);
    expect(useSettingsStore.getState().isPinned("notes/drafts")).toBe(true);
    expect(useSettingsStore.getState().pins.map((pin) => pin.name)).toEqual(["drafts"]);
  });

  it("unpins a path that is already pinned", async () => {
    await useSettingsStore.getState().togglePin("notes/drafts");
    await useSettingsStore.getState().togglePin("notes/drafts");

    expect(backend.workspace.pinned).toEqual([]);
    expect(useSettingsStore.getState().isPinned("notes/drafts")).toBe(false);
  });

  it("keeps the two scopes apart when writing", async () => {
    await useSettingsStore.getState().togglePin("notes.md");
    await useSettingsStore.getState().toggleShowAgentContext();

    expect(backend.app).toEqual({
      showAgentContext: false,
      agentRoots: [],
      daemonCommand: "agentlens-daemon",
    });
    // The workspace write must not have carried the app setting with it.
    expect(backend.workspace.pinned).toEqual(["notes.md"]);
    expect(useSettingsStore.getState().app.showAgentContext).toBe(false);
  });

  it("leaves app-level settings alone when the workspace resets", async () => {
    await useSettingsStore.getState().toggleShowAgentContext();
    await useSettingsStore.getState().togglePin("notes.md");

    useSettingsStore.getState().reset();

    expect(useSettingsStore.getState().settings.pinned).toEqual([]);
    expect(useSettingsStore.getState().pins).toEqual([]);
    // App settings outlive the workspace; re-reading them on every close would
    // flash the default back into the modal.
    expect(useSettingsStore.getState().app.showAgentContext).toBe(false);
  });

  it("does not let concurrent writes overwrite each other from stale state", async () => {
    // The real gesture: committing a textarea on blur fires one save, and the
    // click that caused the blur fires another before the first has landed.
    const first = useSettingsStore.getState().save(["*.log"]);
    const second = useSettingsStore.getState().toggleShowIgnored();
    await Promise.all([first, second]);

    expect(backend.workspace.extraIgnores).toEqual(["*.log"]);
    expect(backend.workspace.showIgnored).toBe(true);
  });

  it("applies rapid pins in order rather than losing one", async () => {
    await Promise.all([
      useSettingsStore.getState().togglePin("a.md"),
      useSettingsStore.getState().togglePin("b.md"),
    ]);

    expect(backend.workspace.pinned).toEqual(["a.md", "b.md"]);
  });

  it("keeps the queue moving after a write fails", async () => {
    const { setWorkspaceSettings } = await import("../lib/tauri");
    vi.mocked(setWorkspaceSettings).mockRejectedValueOnce(new Error("disk full"));

    const failed = useSettingsStore.getState().togglePin("a.md");
    const after = useSettingsStore.getState().togglePin("b.md");

    expect(await failed).toBe(false);
    expect(await after).toBe(true);
    expect(backend.workspace.pinned).toEqual(["b.md"]);
  });

  it("reports a failed save without changing the settings in effect", async () => {
    const { setWorkspaceSettings } = await import("../lib/tauri");
    vi.mocked(setWorkspaceSettings).mockRejectedValueOnce(new Error("disk full"));

    expect(await useSettingsStore.getState().togglePin("notes/drafts")).toBe(false);
    expect(useSettingsStore.getState().error).toBe("disk full");
    expect(useSettingsStore.getState().settings.pinned).toEqual([]);
    expect(useSettingsStore.getState().saving).toBe(false);
  });
});
