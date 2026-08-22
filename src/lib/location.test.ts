import { describe, expect, it } from "vitest";
import { formatLocation, sameTarget, shouldRefreshHostSettings } from "./location";

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

  it("treats two SSH hosts as different machines", () => {
    expect(sameTarget({ kind: "ssh", host: "a" }, { kind: "ssh", host: "b" })).toBe(false);
    expect(sameTarget({ kind: "ssh", host: "a" }, { kind: "ssh", host: "a" })).toBe(true);
    expect(sameTarget({ kind: "local" }, { kind: "ssh", host: "a" })).toBe(false);
  });

  it("refreshes host settings only once the new backend is connected", () => {
    const local = { target: { kind: "local" as const }, state: "connected" as const };
    const installing = {
      target: { kind: "ssh" as const, host: "box" },
      state: "installing" as const,
    };
    const remote = {
      target: { kind: "ssh" as const, host: "box" },
      state: "connected" as const,
    };
    expect(shouldRefreshHostSettings(local, installing)).toBe(false);
    expect(shouldRefreshHostSettings(installing, remote)).toBe(true);
    expect(shouldRefreshHostSettings(remote, remote)).toBe(false);
    expect(shouldRefreshHostSettings(remote, local)).toBe(true);
  });

  it("adds a leading slash on remote paths that lack one", () => {
    expect(formatLocation({ kind: "ssh", host: "box" }, "srv/app")).toBe("ssh://box/srv/app");
  });
});
