import { describe, expect, it } from "vitest";
import { fuzzyFilter, scorePath } from "./fuzzy";

describe("scorePath", () => {
  it("matches a subsequence anywhere in the path", () => {
    expect(scorePath("src/lib/treeRows.ts", "trs")).not.toBeNull();
    expect(scorePath("src/lib/treeRows.ts", "srl")).not.toBeNull();
  });

  it("returns null when a character is missing or out of order", () => {
    expect(scorePath("src/main.rs", "xyz")).toBeNull();
    expect(scorePath("src/main.rs", "niam")).toBeNull();
  });

  it("is case-insensitive", () => {
    expect(scorePath("src/App.tsx", "app")).not.toBeNull();
    expect(scorePath("src/app.tsx", "APP")).not.toBeNull();
  });

  it("reports the matched positions for highlighting", () => {
    expect(scorePath("abc", "ac")?.positions).toEqual([0, 2]);
  });

  it("treats an empty query as a match with no positions", () => {
    const match = scorePath("src/main.rs", "");
    expect(match?.positions).toEqual([]);
  });
});

describe("fuzzyFilter", () => {
  it("ranks a filename match above a directory-only match", () => {
    const paths = ["config/app/other.ts", "src/config.ts"];
    expect(fuzzyFilter(paths, "config", 10)[0]?.path).toBe("src/config.ts");
  });

  it("ranks consecutive matches above scattered ones", () => {
    const paths = ["s-o-m-e-t-h-i-n-g.ts", "something.ts"];
    expect(fuzzyFilter(paths, "something", 10)[0]?.path).toBe("something.ts");
  });

  it("prefers the shorter path when scores are otherwise equal", () => {
    const paths = ["a/b/c/d/main.rs", "main.rs"];
    expect(fuzzyFilter(paths, "main", 10)[0]?.path).toBe("main.rs");
  });

  it("drops non-matches and respects the limit", () => {
    const paths = ["src/a.ts", "src/b.ts", "docs/readme.md"];
    const results = fuzzyFilter(paths, "src", 2);
    expect(results).toHaveLength(2);
    expect(results.every((r) => r.path.startsWith("src/"))).toBe(true);
  });

  it("orders ties stably by path", () => {
    const paths = ["b.ts", "a.ts"];
    expect(fuzzyFilter(paths, "ts", 10).map((r) => r.path)).toEqual(["a.ts", "b.ts"]);
  });
});
