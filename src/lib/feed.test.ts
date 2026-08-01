import { describe, expect, it } from "vitest";
import { groupLabel, summarizeBatch } from "./feed";
import type { FsEvent } from "./protocol";

function event(path: string, kind: FsEvent["kind"]): FsEvent {
  return { kind, path, isDir: false, at: 0 };
}

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
  it("summarizes counts by kind, most frequent first", () => {
    const events = [
      event("a.ts", "modified"),
      event("b.ts", "modified"),
      event("c.ts", "modified"),
      event("d.ts", "created"),
    ];
    expect(summarizeBatch(events)).toBe("3 modified, 1 created");
  });

  it("omits kinds with zero count", () => {
    const events = [event("a.ts", "deleted")];
    expect(summarizeBatch(events)).toBe("1 deleted");
  });

  it("returns an empty string for an empty batch", () => {
    expect(summarizeBatch([])).toBe("");
  });

  it("breaks ties using created, modified, deleted, renamed order", () => {
    const events = [event("a.ts", "renamed"), event("b.ts", "created")];
    expect(summarizeBatch(events)).toBe("1 created, 1 renamed");
  });
});
