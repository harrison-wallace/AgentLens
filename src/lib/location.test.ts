import { describe, expect, it } from "vitest";
import { formatLocation } from "./location";

describe("formatLocation", () => {
  it("leaves a local path unchanged", () => {
    expect(formatLocation({ kind: "local" }, "/home/h/proj")).toBe("/home/h/proj");
    expect(formatLocation({ kind: "local" }, "C:/Users/h/proj")).toBe("C:/Users/h/proj");
  });

  it("prefixes WSL and SSH so the same path on two machines stays distinct", () => {
    expect(formatLocation({ kind: "wsl", distro: "Ubuntu" }, "/home/h/proj")).toBe(
      "wsl://Ubuntu/home/h/proj",
    );
    expect(formatLocation({ kind: "ssh", host: "box" }, "/srv/app")).toBe("ssh://box/srv/app");
  });

  it("adds a leading slash on remote paths that lack one", () => {
    expect(formatLocation({ kind: "ssh", host: "box" }, "srv/app")).toBe("ssh://box/srv/app");
  });
});
