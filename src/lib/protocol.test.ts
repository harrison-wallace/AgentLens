import { describe, expect, it } from "vitest";
import type { AppInfo } from "./protocol";

describe("AppInfo", () => {
  it("mirrors the camelCase field names serialized by src-tauri/src/protocol.rs", () => {
    const info: AppInfo = { name: "AgentLens", version: "0.0.1" };

    expect(Object.keys(info).sort()).toEqual(["name", "version"]);
  });
});
