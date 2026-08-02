import { beforeEach, describe, expect, it } from "vitest";
import { useFeedStore } from "./feedStore";
import type { FsEvent } from "../lib/protocol";

function change(path: string): FsEvent {
  return { kind: "modified", path, isDir: false, at: Date.now() };
}

describe("feedStore", () => {
  beforeEach(() => useFeedStore.getState().clear());

  it("ignores an empty batch rather than adding a blank row", () => {
    useFeedStore.getState().addBatch([]);
    expect(useFeedStore.getState().entries).toHaveLength(0);
  });

  it("marks a disconnection as one gap, however many retries it takes", () => {
    const { beginGap } = useFeedStore.getState();
    beginGap(1_000);
    beginGap(2_000);
    beginGap(3_000);

    const entries = useFeedStore.getState().entries;
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({ kind: "gap", from: 1_000, to: null });
  });

  it("closes the gap on reconnect and leaves later batches alone", () => {
    const store = useFeedStore.getState();
    store.beginGap(1_000);
    store.endGap(4_000);
    store.addBatch([change("a.txt")]);

    const entries = useFeedStore.getState().entries;
    expect(entries[0]).toMatchObject({ kind: "batch" });
    expect(entries[1]).toMatchObject({ kind: "gap", from: 1_000, to: 4_000 });
  });

  it("does not reopen a gap that has already been closed", () => {
    const store = useFeedStore.getState();
    store.beginGap(1_000);
    store.endGap(2_000);
    store.endGap(9_000);

    expect(useFeedStore.getState().entries[0]).toMatchObject({ to: 2_000 });
  });

  it("closes the gap even when changes landed before the reconnect was announced", () => {
    // A reconnecting daemon starts its watcher as part of being restored, so
    // a batch can arrive ahead of the event saying the link is back. The gap
    // must still close — a marker stuck on "disconnected since…" is the exact
    // lie it exists to prevent.
    const store = useFeedStore.getState();
    store.beginGap(1_000);
    store.addBatch([change("a.txt")]);
    store.endGap(4_000);

    const entries = useFeedStore.getState().entries;
    expect(entries[0]).toMatchObject({ kind: "batch" });
    expect(entries[1]).toMatchObject({ kind: "gap", from: 1_000, to: 4_000 });
  });

  it("ignores a reconnect that follows no outage", () => {
    const store = useFeedStore.getState();
    store.addBatch([change("a.txt")]);
    store.endGap(5_000);

    const entries = useFeedStore.getState().entries;
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({ kind: "batch" });
  });

  it("keeps a gap that opens after activity, above the activity", () => {
    const store = useFeedStore.getState();
    store.addBatch([change("a.txt")]);
    store.beginGap(7_000);

    const entries = useFeedStore.getState().entries;
    expect(entries[0]).toMatchObject({ kind: "gap" });
    expect(entries[1]).toMatchObject({ kind: "batch" });
  });
});
