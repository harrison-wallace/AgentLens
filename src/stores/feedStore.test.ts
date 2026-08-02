import { beforeEach, describe, expect, it } from "vitest";
import { useFeedStore } from "./feedStore";
import type { FsEvent } from "../lib/protocol";

function change(path: string, kind: FsEvent["kind"] = "modified"): FsEvent {
  return { kind, path, isDir: false, at: Date.now() };
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

  it("accumulates session +/− across batches and resets on clear", () => {
    const store = useFeedStore.getState();
    store.addBatch([
      change("a.ts", "created"),
      change("b.ts", "created"),
      change("c.ts", "deleted"),
      change("d.ts", "modified"),
    ]);
    store.addBatch([change("e.ts", "deleted"), change("f.ts", "created")]);

    expect(useFeedStore.getState().sessionTotals).toEqual({ created: 3, deleted: 2 });

    store.clear();
    expect(useFeedStore.getState().sessionTotals).toEqual({ created: 0, deleted: 0 });
  });

  it("trims entries when maxEntries is lowered without resetting totals", () => {
    const store = useFeedStore.getState();
    // Floor is 50; seed past the cap then lower to the floor.
    store.setMaxEntries(52);
    for (let i = 0; i < 55; i += 1) {
      store.addBatch([change(`f${i}`, i % 2 === 0 ? "created" : "deleted")]);
    }
    expect(useFeedStore.getState().entries).toHaveLength(52);
    const totals = useFeedStore.getState().sessionTotals;
    expect(totals.created + totals.deleted).toBe(55);

    store.setMaxEntries(50);
    expect(useFeedStore.getState().entries).toHaveLength(50);
    expect(useFeedStore.getState().sessionTotals).toEqual(totals);
  });
});
