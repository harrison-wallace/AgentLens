import { describe, expect, it } from "vitest";
import type {
  AppInfo,
  DirEntryNode,
  GitFileStatus,
  GitStatusKind,
  GitStatusSnapshot,
  WorkspaceInfo,
} from "./protocol";

describe("AppInfo", () => {
  it("mirrors the camelCase field names serialized by src-tauri/src/protocol.rs", () => {
    const info: AppInfo = { name: "AgentLens", version: "0.0.1" };

    expect(Object.keys(info).sort()).toEqual(["name", "version"]);
  });
});

describe("WorkspaceInfo", () => {
  it("mirrors the camelCase field names serialized by src-tauri/src/protocol.rs", () => {
    const info: WorkspaceInfo = {
      root: "/home/user/project",
      name: "project",
      watchingSince: 0,
    };

    expect(Object.keys(info).sort()).toEqual(["name", "root", "watchingSince"]);
  });
});

describe("DirEntryNode", () => {
  it("mirrors the camelCase field names serialized by src-tauri/src/protocol.rs", () => {
    const entry: DirEntryNode = { name: "src", path: "src", isDir: true, ignored: false };

    expect(Object.keys(entry).sort()).toEqual(["ignored", "isDir", "name", "path"]);
  });
});

describe("GitStatusKind", () => {
  it("matches the Rust enum variants (camelCase-serialized) exactly", () => {
    const kinds: GitStatusKind[] = [
      "added",
      "modified",
      "deleted",
      "renamed",
      "untracked",
      "conflicted",
    ];

    expect(new Set(kinds).size).toBe(6);
  });
});

describe("GitFileStatus / GitStatusSnapshot", () => {
  it("mirrors the camelCase field names serialized by src-tauri/src/protocol.rs", () => {
    const file: GitFileStatus = { path: "src/main.rs", status: "modified", staged: false };
    const snapshot: GitStatusSnapshot = { isRepository: true, branch: "main", files: [file] };

    expect(Object.keys(file).sort()).toEqual(["path", "staged", "status"]);
    expect(Object.keys(snapshot).sort()).toEqual(["branch", "files", "isRepository"]);
  });
});
