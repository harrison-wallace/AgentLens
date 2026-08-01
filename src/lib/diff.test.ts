import { describe, expect, it } from "vitest";
import {
  collapseContext,
  diffUnavailableReason,
  summarizeDiff,
  toDiffRows,
  truncateDisplay,
} from "./diff";
import type { SessionDiff } from "./protocol";

function sessionDiff(overrides: Partial<SessionDiff>): SessionDiff {
  return { path: "a.txt", baseline: null, current: null, unavailable: false, ...overrides };
}

describe("toDiffRows", () => {
  it("marks added, removed, and context lines with line numbers", () => {
    const rows = toDiffRows("one\ntwo\n", "one\ntwo\nthree\n");
    expect(rows).toEqual([
      { kind: "context", text: "one", baselineLine: 1, currentLine: 1 },
      { kind: "context", text: "two", baselineLine: 2, currentLine: 2 },
      { kind: "added", text: "three", baselineLine: null, currentLine: 3 },
    ]);
  });

  it("does not invent a trailing row for the final newline", () => {
    expect(toDiffRows("one\n", "one\n")).toEqual([
      { kind: "context", text: "one", baselineLine: 1, currentLine: 1 },
    ]);
  });

  it("treats a null baseline as a wholly added file", () => {
    const rows = toDiffRows(null, "new\n");
    expect(rows).toEqual([{ kind: "added", text: "new", baselineLine: null, currentLine: 1 }]);
  });

  it("treats a null current as a wholly removed file", () => {
    const rows = toDiffRows("gone\n", null);
    expect(rows).toEqual([{ kind: "removed", text: "gone", baselineLine: 1, currentLine: null }]);
  });

  it("keeps the two sides' line numbers independent across a replacement", () => {
    const rows = toDiffRows("a\nb\nc\n", "a\nB\nc\n");
    expect(rows.find((r) => r.kind === "removed")).toEqual({
      kind: "removed",
      text: "b",
      baselineLine: 2,
      currentLine: null,
    });
    expect(rows.find((r) => r.kind === "added")).toEqual({
      kind: "added",
      text: "B",
      baselineLine: null,
      currentLine: 2,
    });
    expect(rows[rows.length - 1]).toEqual({
      kind: "context",
      text: "c",
      baselineLine: 3,
      currentLine: 3,
    });
  });
});

describe("summarizeDiff", () => {
  it("counts added and removed lines only", () => {
    expect(summarizeDiff(toDiffRows("a\nb\n", "a\nB\nc\n"))).toEqual({ added: 2, removed: 1 });
  });

  it("reports zeroes for an identical file", () => {
    expect(summarizeDiff(toDiffRows("same\n", "same\n"))).toEqual({ added: 0, removed: 0 });
  });
});

describe("collapseContext", () => {
  it("hides unchanged stretches beyond the context window", () => {
    const baseline = Array.from({ length: 20 }, (_, i) => `line${i}`).join("\n") + "\n";
    const current = baseline.replace("line10", "CHANGED");
    const display = collapseContext(toDiffRows(baseline, current), 2);

    const gaps = display.filter((d) => d.type === "gap");
    expect(gaps.length).toBeGreaterThan(0);
    expect(display.some((d) => d.type === "row" && d.row.text === "CHANGED")).toBe(true);
    expect(display.some((d) => d.type === "row" && d.row.text === "line0")).toBe(false);
  });

  it("keeps everything when the file is entirely changed", () => {
    const display = collapseContext(toDiffRows("a\n", "b\n"), 3);
    expect(display.every((d) => d.type === "row")).toBe(true);
  });

  it("collapses an unchanged file to a single gap", () => {
    const display = collapseContext(toDiffRows("a\nb\nc\n", "a\nb\nc\n"), 2);
    expect(display).toEqual([{ type: "gap", hidden: 3 }]);
  });
});

describe("truncateDisplay", () => {
  it("leaves a diff that fits untouched", () => {
    const display = collapseContext(toDiffRows("a\n", "b\n"), 3);
    expect(truncateDisplay(display, 100)).toEqual({ display, truncated: 0 });
  });

  it("caps a wholesale rewrite and reports the remainder", () => {
    // Every line differs, so `collapseContext` elides nothing.
    const baseline = Array.from({ length: 500 }, (_, i) => `old${i}`).join("\n") + "\n";
    const current = Array.from({ length: 500 }, (_, i) => `new${i}`).join("\n") + "\n";
    const display = collapseContext(toDiffRows(baseline, current), 3);
    expect(display.length).toBe(1000);

    const capped = truncateDisplay(display, 100);
    expect(capped.display).toHaveLength(100);
    expect(capped.truncated).toBe(900);
  });
});

describe("diffUnavailableReason", () => {
  it("explains that a non-repository has no baseline", () => {
    expect(diffUnavailableReason(sessionDiff({ unavailable: true }))).toMatch(/git repository/);
  });

  it("explains when neither side has readable content", () => {
    expect(diffUnavailableReason(sessionDiff({}))).toMatch(/binary/);
  });

  it("returns null when there is something to show", () => {
    expect(diffUnavailableReason(sessionDiff({ current: "x" }))).toBeNull();
  });
});
