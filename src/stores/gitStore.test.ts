import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GitStatusSnapshot } from "../lib/protocol";

const EMPTY: GitStatusSnapshot = { isRepository: true, branch: "main", files: [] };

/** Order in which the backend actually saw the calls. */
const calls: string[] = [];

/** Resolves only when released, so overlap is observable. */
function deferred() {
  let release!: () => void;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { gate, release };
}

vi.mock("../lib/tauri", () => ({
  gitStatus: vi.fn(async () => EMPTY),
  gitCapabilities: vi.fn(async () => ({ canMutate: true, version: "git version 2", reason: null })),
  gitBranches: vi.fn(async () => ({ current: "main", branches: ["main"] })),
  gitStage: vi.fn(async (paths: string[]) => {
    calls.push(`stage:${paths.join(",")}`);
    return EMPTY;
  }),
  gitStageAll: vi.fn(async () => EMPTY),
  gitUnstage: vi.fn(async () => EMPTY),
  gitUnstageAll: vi.fn(async () => EMPTY),
  gitCommit: vi.fn(async () => EMPTY),
  gitSwitchBranch: vi.fn(async () => EMPTY),
  gitCreateBranch: vi.fn(async () => EMPTY),
  gitStashPush: vi.fn(async () => EMPTY),
  gitStashPop: vi.fn(async () => EMPTY),
}));

const { useGitStore } = await import("./gitStore");

describe("gitStore", () => {
  beforeEach(() => {
    calls.length = 0;
    useGitStore.getState().reset();
    vi.clearAllMocks();
  });

  it("applies the status returned by the mutation itself", async () => {
    const status: GitStatusSnapshot = {
      isRepository: true,
      branch: "main",
      files: [{ path: "a.rs", status: "added", staged: true }],
    };
    const { gitStage } = await import("../lib/tauri");
    vi.mocked(gitStage).mockResolvedValueOnce(status);

    expect(await useGitStore.getState().stage(["a.rs"])).toBe(true);

    // No second round trip: the write's own response is the new truth.
    expect(useGitStore.getState().status).toEqual(status);
    expect(useGitStore.getState().statusByPath).toEqual({ "a.rs": "added" });
  });

  it("surfaces git's own message on failure and stays usable", async () => {
    const { gitCommit } = await import("../lib/tauri");
    vi.mocked(gitCommit).mockRejectedValueOnce(new Error("nothing to commit, working tree clean"));

    expect(await useGitStore.getState().commit("wip")).toBe(false);
    expect(useGitStore.getState().error).toBe("nothing to commit, working tree clean");
    expect(useGitStore.getState().busy).toBe(false);

    // The next attempt clears it rather than needing a manual dismiss.
    expect(await useGitStore.getState().commit("wip")).toBe(true);
    expect(useGitStore.getState().error).toBeNull();
  });

  it("serializes mutations so two git processes never overlap", async () => {
    // Concurrent writes to one repository collide on the index lock, and a
    // hook can hold the first for seconds.
    const { gitStage } = await import("../lib/tauri");
    const first = deferred();

    vi.mocked(gitStage).mockImplementationOnce(async (paths: string[]) => {
      calls.push(`start:${paths.join(",")}`);
      await first.gate;
      calls.push(`end:${paths.join(",")}`);
      return EMPTY;
    });

    const a = useGitStore.getState().stage(["a.rs"]);
    const b = useGitStore.getState().stage(["b.rs"]);

    first.release();
    await Promise.all([a, b]);

    expect(calls).toEqual(["start:a.rs", "end:a.rs", "stage:b.rs"]);
  });

  it("keeps the queue moving after a failure", async () => {
    const { gitStage } = await import("../lib/tauri");
    vi.mocked(gitStage).mockRejectedValueOnce(new Error("index.lock exists"));

    const failed = useGitStore.getState().stage(["a.rs"]);
    const after = useGitStore.getState().stage(["b.rs"]);

    expect(await failed).toBe(false);
    expect(await after).toBe(true);
  });

  it("re-reads branches after switching, since the checkout moved", async () => {
    const { gitBranches } = await import("../lib/tauri");
    vi.mocked(gitBranches).mockResolvedValueOnce({
      current: "feature",
      branches: ["main", "feature"],
    });

    await useGitStore.getState().switchBranch("feature");
    expect(useGitStore.getState().branches?.current).toBe("feature");
  });

  it("does not re-read branches when the switch failed", async () => {
    const { gitSwitchBranch, gitBranches } = await import("../lib/tauri");
    vi.mocked(gitSwitchBranch).mockRejectedValueOnce(new Error("no such branch"));

    expect(await useGitStore.getState().switchBranch("nope")).toBe(false);
    expect(vi.mocked(gitBranches)).not.toHaveBeenCalled();
  });

  it("reports mutations unavailable, with the reason, when capabilities can't be read", async () => {
    const { gitCapabilities } = await import("../lib/tauri");
    vi.mocked(gitCapabilities).mockRejectedValueOnce(new Error("no workspace"));

    await useGitStore.getState().refreshCapabilities();
    expect(useGitStore.getState().capabilities).toEqual({
      canMutate: false,
      version: null,
      reason: "no workspace",
    });
  });

  it("starts with capabilities unknown rather than unavailable", async () => {
    // The panel must be able to tell "still loading" from "git is missing",
    // or it flashes an unavailable notice on every workspace open.
    expect(useGitStore.getState().capabilities).toBeNull();
  });
});
