import { describe, expect, it } from "vitest";
import {
  clampFeedMaxEntries,
  countByKind,
  countFeedByKind,
  DEFAULT_FEED_MAX_ENTRIES,
  feedSortLabel,
  filterEvents,
  formatKindCounts,
  groupLabel,
  MAX_FEED_MAX_ENTRIES,
  MIN_FEED_MAX_ENTRIES,
  nextFeedSort,
  presentFeedEntries,
  splitPath,
  summarizeBatch,
  toggleKindFilter,
} from "./feed";
import type { FsEvent, FsEventKind } from "./protocol";

function event(path: string, kind: FsEvent["kind"]): FsEvent {
  return { kind, path, isDir: false, at: 0 };
}

describe("clampFeedMaxEntries", () => {
  it("defaults non-finite values", () => {
    expect(clampFeedMaxEntries(Number.NaN)).toBe(DEFAULT_FEED_MAX_ENTRIES);
    expect(clampFeedMaxEntries(Infinity)).toBe(DEFAULT_FEED_MAX_ENTRIES);
  });

  it("clamps to the safe range and rounds", () => {
    expect(clampFeedMaxEntries(1)).toBe(MIN_FEED_MAX_ENTRIES);
    expect(clampFeedMaxEntries(99_999)).toBe(MAX_FEED_MAX_ENTRIES);
    expect(clampFeedMaxEntries(250.6)).toBe(251);
  });
});

describe("groupLabel", () => {
  const now = 1_700_000_000_000;

  it("is 'just now' for anything under 10s old", () => {
    expect(groupLabel(now, now)).toBe("just now");
    expect(groupLabel(now - 9_000, now)).toBe("just now");
  });

  it("shows seconds between 10s and 60s", () => {
    expect(groupLabel(now - 12_000, now)).toBe("12s ago");
    expect(groupLabel(now - 59_000, now)).toBe("59s ago");
  });

  it("shows minutes between 1m and 1h", () => {
    expect(groupLabel(now - 60_000, now)).toBe("1m ago");
    expect(groupLabel(now - 3 * 60_000, now)).toBe("3m ago");
  });

  it("shows hours beyond 1h", () => {
    expect(groupLabel(now - 60 * 60_000, now)).toBe("1h ago");
    expect(groupLabel(now - 5 * 60 * 60_000, now)).toBe("5h ago");
  });

  it("clamps a future timestamp (clock skew) to 'just now'", () => {
    expect(groupLabel(now + 5_000, now)).toBe("just now");
  });
});

describe("summarizeBatch", () => {
  it("summarizes counts in compact mono form, most frequent first", () => {
    const events = [
      event("a.ts", "modified"),
      event("b.ts", "modified"),
      event("c.ts", "modified"),
      event("d.ts", "created"),
    ];
    expect(summarizeBatch(events)).toBe("M 3  + 1");
  });

  it("omits kinds with zero count", () => {
    const events = [event("a.ts", "deleted")];
    expect(summarizeBatch(events)).toBe("− 1");
  });

  it("returns an empty string for an empty batch", () => {
    expect(summarizeBatch([])).toBe("");
  });

  it("breaks ties using created, modified, deleted, renamed order", () => {
    const events = [event("a.ts", "renamed"), event("b.ts", "created")];
    expect(summarizeBatch(events)).toBe("+ 1  → 1");
  });
});

describe("countByKind / countFeedByKind", () => {
  it("counts each kind independently", () => {
    expect(
      countByKind([
        event("a", "created"),
        event("b", "created"),
        event("c", "deleted"),
        event("d", "modified"),
      ]),
    ).toEqual({ created: 2, modified: 1, deleted: 1, renamed: 0 });
  });

  it("aggregates only batch entries", () => {
    const counts = countFeedByKind([
      {
        kind: "batch",
        events: [event("a", "created"), event("b", "deleted")],
      },
      { kind: "gap" },
      { kind: "batch", events: [event("c", "created")] },
    ]);
    expect(counts).toEqual({ created: 2, modified: 0, deleted: 1, renamed: 0 });
  });
});

describe("formatKindCounts", () => {
  it("skips zeros and keeps status-bar spacing", () => {
    expect(formatKindCounts({ created: 2, modified: 0, deleted: 5, renamed: 0 })).toBe("− 5  + 2");
  });
});

describe("filterEvents / toggleKindFilter", () => {
  it("passes everything through an empty filter", () => {
    const events = [event("a", "created"), event("b", "deleted")];
    expect(filterEvents(events, new Set())).toEqual(events);
  });

  it("keeps only selected kinds", () => {
    const events = [event("a", "created"), event("b", "deleted"), event("c", "modified")];
    expect(filterEvents(events, new Set<FsEventKind>(["deleted"]))).toEqual([
      event("b", "deleted"),
    ]);
  });

  it("toggles kinds on and off", () => {
    const once = toggleKindFilter(new Set(), "deleted");
    expect([...once]).toEqual(["deleted"]);
    const twice = toggleKindFilter(once, "deleted");
    expect(twice.size).toBe(0);
  });
});

describe("nextFeedSort", () => {
  it("cycles time → created → deleted → events → time", () => {
    expect(nextFeedSort("time")).toBe("created");
    expect(nextFeedSort("created")).toBe("deleted");
    expect(nextFeedSort("deleted")).toBe("events");
    expect(nextFeedSort("events")).toBe("time");
  });

  it("labels stay short", () => {
    expect(feedSortLabel("time")).toBe("time");
    expect(feedSortLabel("created")).toBe("most +");
    expect(feedSortLabel("deleted")).toBe("most −");
    expect(feedSortLabel("events")).toBe("most");
  });
});

describe("presentFeedEntries", () => {
  const batches = [
    {
      kind: "batch" as const,
      id: "b1",
      at: 300,
      events: [event("a", "created"), event("b", "created"), event("c", "modified")],
    },
    {
      kind: "batch" as const,
      id: "b2",
      at: 200,
      events: [event("d", "deleted"), event("e", "deleted"), event("f", "deleted")],
    },
    {
      kind: "batch" as const,
      id: "b3",
      at: 100,
      events: [event("g", "created")],
    },
  ];
  const gap = {
    kind: "gap" as const,
    id: "g1",
    at: 250,
    from: 240,
    to: 250,
  };

  it("preserves newest-first order for time sort", () => {
    const entries = [batches[0]!, gap, batches[1]!, batches[2]!];
    expect(presentFeedEntries(entries, new Set(), "time").map((e) => e.id)).toEqual([
      "b1",
      "g1",
      "b2",
      "b3",
    ]);
  });

  it("filters events and drops empty batches; gaps always stay", () => {
    const entries = [batches[0]!, gap, batches[1]!];
    const presented = presentFeedEntries(entries, new Set<FsEventKind>(["deleted"]), "time");
    expect(presented.map((e) => e.id)).toEqual(["g1", "b2"]);
    expect(presented[1]).toMatchObject({
      kind: "batch",
      events: [event("d", "deleted"), event("e", "deleted"), event("f", "deleted")],
    });
  });

  it("sorts batches by most deleted, then appends gaps", () => {
    const entries = [batches[0]!, gap, batches[1]!, batches[2]!];
    expect(presentFeedEntries(entries, new Set(), "deleted").map((e) => e.id)).toEqual([
      "b2",
      "b1",
      "b3",
      "g1",
    ]);
  });

  it("sorts batches by most created", () => {
    const entries = [batches[0]!, batches[1]!, batches[2]!];
    expect(presentFeedEntries(entries, new Set(), "created").map((e) => e.id)).toEqual([
      "b1",
      "b3",
      "b2",
    ]);
  });

  it("sorts batches by total event count", () => {
    const entries = [batches[2]!, batches[0]!, batches[1]!];
    expect(presentFeedEntries(entries, new Set(), "events").map((e) => e.id)).toEqual([
      "b1",
      "b2",
      "b3",
    ]);
  });
});

describe("splitPath", () => {
  it("splits a nested path into filename and directory", () => {
    expect(splitPath("src/components/AuthProvider.tsx")).toEqual({
      name: "AuthProvider.tsx",
      dir: "src/components",
    });
  });

  it("leaves a root-level file with no directory", () => {
    expect(splitPath("README.md")).toEqual({ name: "README.md", dir: "" });
  });

  it("splits a directory path the same way", () => {
    expect(splitPath("src/components")).toEqual({ name: "components", dir: "src" });
  });
});
