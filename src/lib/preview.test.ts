import { describe, expect, it } from "vitest";
import { canRenderRich, formatBytes, RICH_RENDER_MAX_BYTES } from "./preview";

describe("canRenderRich", () => {
  it("allows an ordinary source file", () => {
    expect(canRenderRich("fn main() {}\n")).toBe(true);
  });

  it("allows a file exactly at the cap", () => {
    expect(canRenderRich("a".repeat(RICH_RENDER_MAX_BYTES))).toBe(true);
  });

  it("rejects a file one byte over the cap", () => {
    expect(canRenderRich("a".repeat(RICH_RENDER_MAX_BYTES + 1))).toBe(false);
  });

  it("counts UTF-8 bytes, not UTF-16 units", () => {
    // Each "€" is 3 bytes, so a third of the cap in characters exceeds it.
    const nearly = "€".repeat(Math.floor(RICH_RENDER_MAX_BYTES / 3) + 1);
    expect(nearly.length).toBeLessThan(RICH_RENDER_MAX_BYTES);
    expect(canRenderRich(nearly)).toBe(false);
  });

  it("counts a surrogate pair as four bytes", () => {
    const emoji = "😀".repeat(RICH_RENDER_MAX_BYTES / 4 + 1);
    expect(canRenderRich(emoji)).toBe(false);
  });
});

describe("formatBytes", () => {
  it("formats bytes, kilobytes, and megabytes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(3 * 1024 * 1024)).toBe("3.0 MB");
  });
});
