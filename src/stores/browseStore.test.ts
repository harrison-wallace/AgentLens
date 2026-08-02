import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BrowseListing, ConnectionInfo, ConnectionTarget } from "../lib/protocol";

const listing = (path: string): BrowseListing => ({
  path,
  parent: "/home",
  entries: [{ name: "project", path: `${path}/project`, isRepository: true }],
  truncated: false,
});

const backend = {
  listing: listing("/home/you"),
  connectFails: null as string | null,
  browseFails: null as string | null,
};

vi.mock("../lib/tauri", () => ({
  connect: vi.fn(async (target: ConnectionTarget) => {
    if (backend.connectFails) throw new Error(backend.connectFails);
    return { ...live, target, state: "connected", remote: true } as ConnectionInfo;
  }),
  browseDir: vi.fn(async () => {
    if (backend.browseFails) throw new Error(backend.browseFails);
    return backend.listing;
  }),
}));

/** The connection as the rest of the app currently sees it. */
let live: ConnectionInfo;

vi.mock("./connectionStore", () => ({
  useConnectionStore: {
    getState: () => ({
      info: live,
      apply: (info: ConnectionInfo) => {
        live = info;
      },
      refresh: vi.fn(async () => undefined),
    }),
  },
}));

const { useBrowseStore } = await import("./browseStore");
const { connect, browseDir } = await import("../lib/tauri");

const LOCAL: ConnectionInfo = {
  target: { kind: "local" },
  state: "connected",
  label: "Local",
  remote: false,
  message: null,
  daemonVersion: null,
  since: 0,
};

const BOX: ConnectionTarget = { kind: "ssh", host: "box" };

describe("browseStore", () => {
  beforeEach(() => {
    live = { ...LOCAL };
    backend.listing = listing("/home/you");
    backend.connectFails = null;
    backend.browseFails = null;
    useBrowseStore.getState().close();
    vi.mocked(connect).mockClear();
    vi.mocked(browseDir).mockClear();
  });

  it("connects when the picker is pointed somewhere new", async () => {
    await useBrowseStore.getState().start(BOX);

    expect(connect).toHaveBeenCalledTimes(1);
    expect(useBrowseStore.getState().listing?.path).toBe("/home/you");
  });

  it("does not reconnect a link that is already up", async () => {
    // Reconnecting tears down a working daemon and can re-prompt for an SSH
    // passphrase, to answer a listing the existing connection already can.
    live = { ...LOCAL, target: BOX, remote: true, label: "SSH: box" };

    await useBrowseStore.getState().start(BOX);

    expect(connect).not.toHaveBeenCalled();
    expect(browseDir).toHaveBeenCalledTimes(1);
  });

  it("does reconnect when the same kind of target names another machine", async () => {
    live = { ...LOCAL, target: { kind: "ssh", host: "other" }, remote: true };

    await useBrowseStore.getState().start(BOX);

    expect(connect).toHaveBeenCalledTimes(1);
  });

  it("reconnects when the existing link is not healthy", async () => {
    live = { ...LOCAL, target: BOX, remote: true, state: "disconnected" };

    await useBrowseStore.getState().start(BOX);

    expect(connect).toHaveBeenCalledTimes(1);
  });

  it("keeps the target on a failed connect so the reason stays on screen", async () => {
    // The browser panel renders only while `target` is set, and it is the one
    // place this error is shown — clearing it would make the failure silent.
    backend.connectFails = "agentlens-daemon: command not found";

    await useBrowseStore.getState().start(BOX);

    const state = useBrowseStore.getState();
    expect(state.target).toEqual(BOX);
    expect(state.error).toContain("command not found");
    expect(state.listing).toBeNull();
    expect(state.loading).toBe(false);
  });

  it("keeps the previous listing when a directory cannot be read", async () => {
    await useBrowseStore.getState().start(BOX);
    backend.browseFails = "permission denied";

    await useBrowseStore.getState().go("/root");

    const state = useBrowseStore.getState();
    expect(state.listing?.path).toBe("/home/you");
    expect(state.error).toContain("permission denied");
  });

  it("forgets everything on close", async () => {
    await useBrowseStore.getState().start(BOX);

    useBrowseStore.getState().close();

    expect(useBrowseStore.getState().listing).toBeNull();
    expect(useBrowseStore.getState().target).toBeNull();
  });
});
