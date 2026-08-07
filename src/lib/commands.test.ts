import { describe, expect, it } from "vitest";
import { appCommands } from "./commands";

describe("appCommands", () => {
  it("has unique ids", () => {
    const ids = appCommands().map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("gives every command a non-empty title", () => {
    for (const command of appCommands()) {
      expect(command.title.trim().length).toBeGreaterThan(0);
    }
  });

  it("uses each keys string at most once", () => {
    const keys = appCommands()
      .map((c) => c.keys)
      .filter((k): k is string => typeof k === "string" && k.length > 0);
    expect(new Set(keys).size).toBe(keys.length);
  });
});
