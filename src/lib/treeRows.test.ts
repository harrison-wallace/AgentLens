import { describe, expect, it } from "vitest";
import { countsFor, flattenTree, gitBadgeFor, parentDirsOf } from "./treeRows";
import type { DirEntryNode, GitFileStatus, GitStatusKind } from "./protocol";

function entry(name: string, path: string, isDir: boolean): DirEntryNode {
  return { name, path, isDir, ignored: false, agentContext: false };
}

/** Expected tree row; `ignored` is false unless a case says otherwise. */
function row(name: string, path: string, isDir: boolean, depth: number, ignored = false) {
  return { name, path, isDir, depth, ignored, agentContext: false };
}

describe("flattenTree", () => {
  it("emits nested rows with correct depth when parents are expanded", () => {
    const childrenByPath: Record<string, DirEntryNode[]> = {
      "": [entry("src", "src", true), entry("README.md", "README.md", false)],
      src: [entry("main.ts", "src/main.ts", false)],
    };
    const expanded = new Set(["src"]);

    const rows = flattenTree(childrenByPath, expanded);

    expect(rows).toEqual([
      row("src", "src", true, 0),
      row("main.ts", "src/main.ts", false, 1),
      row("README.md", "README.md", false, 0),
    ]);
  });

  it("excludes children of a collapsed directory", () => {
    const childrenByPath: Record<string, DirEntryNode[]> = {
      "": [entry("src", "src", true)],
      src: [entry("main.ts", "src/main.ts", false)],
    };

    const rows = flattenTree(childrenByPath, new Set());

    expect(rows).toEqual([row("src", "src", true, 0)]);
  });

  it("emits the row for an expanded-but-unloaded directory with no children", () => {
    const childrenByPath: Record<string, DirEntryNode[]> = {
      "": [entry("src", "src", true)],
      // "src" intentionally missing: expanded but list_dir hasn't resolved yet.
    };
    const expanded = new Set(["src"]);

    const rows = flattenTree(childrenByPath, expanded);

    expect(rows).toEqual([row("src", "src", true, 0)]);
  });

  it("returns no rows when the root itself has not loaded", () => {
    expect(flattenTree({}, new Set())).toEqual([]);
  });

  it("preserves backend ordering rather than re-sorting", () => {
    const childrenByPath: Record<string, DirEntryNode[]> = {
      "": [entry("zeta", "zeta", false), entry("alpha", "alpha", false)],
    };

    const rows = flattenTree(childrenByPath, new Set());

    expect(rows.map((r) => r.name)).toEqual(["zeta", "alpha"]);
  });

  it("recurses through multiple levels of expanded directories", () => {
    const childrenByPath: Record<string, DirEntryNode[]> = {
      "": [entry("a", "a", true)],
      a: [entry("b", "a/b", true)],
      "a/b": [entry("c.ts", "a/b/c.ts", false)],
    };
    const expanded = new Set(["a", "a/b"]);

    const rows = flattenTree(childrenByPath, expanded);

    expect(rows).toEqual([
      row("a", "a", true, 0),
      row("b", "a/b", true, 1),
      row("c.ts", "a/b/c.ts", false, 2),
    ]);
  });
});

describe("gitBadgeFor", () => {
  const kinds: [GitStatusKind, string][] = [
    ["added", "A"],
    ["modified", "M"],
    ["deleted", "D"],
    ["renamed", "R"],
    ["untracked", "?"],
    ["conflicted", "!"],
  ];

  it.each(kinds)("maps %s to badge %s", (kind, badge) => {
    expect(gitBadgeFor("a.ts", { "a.ts": kind })).toBe(badge);
  });

  it("returns null when the path has no recorded status", () => {
    expect(gitBadgeFor("a.ts", {})).toBeNull();
  });
});

describe("countsFor", () => {
  it("counts each status kind independently", () => {
    const files: GitFileStatus[] = [
      { path: "a.ts", status: "modified", staged: false },
      { path: "b.ts", status: "modified", staged: true },
      { path: "c.ts", status: "added", staged: true },
      { path: "d.ts", status: "deleted", staged: false },
      { path: "e.ts", status: "untracked", staged: false },
      { path: "f.ts", status: "renamed", staged: true },
      { path: "g.ts", status: "conflicted", staged: false },
    ];

    expect(countsFor(files)).toEqual({ modified: 2, added: 1, deleted: 1, untracked: 1 });
  });

  it("returns all zeros for an empty file list", () => {
    expect(countsFor([])).toEqual({ modified: 0, added: 0, deleted: 0, untracked: 0 });
  });
});

describe("parentDirsOf", () => {
  it("maps a root-level file to the empty-string root", () => {
    expect(parentDirsOf(["README.md"])).toEqual([""]);
  });

  it("returns the containing directory for a nested path", () => {
    expect(parentDirsOf(["src/main.ts"])).toEqual(["src"]);
  });

  it("dedupes and preserves first-seen order across mixed paths", () => {
    const dirs = parentDirsOf(["b.txt", "src/main.ts", "src/lib/x.ts", "a.txt", "src/main2.ts"]);
    expect(dirs).toEqual(["", "src", "src/lib"]);
  });

  it("returns an empty array for an empty input", () => {
    expect(parentDirsOf([])).toEqual([]);
  });
});
