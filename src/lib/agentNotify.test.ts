import { describe, expect, it } from "vitest";
import { notificationForTransition, trayTooltip } from "./agentNotify";

const base = {
  focused: false,
  enabled: true,
  agentLabel: "Claude Code",
  title: "fix the bug",
};

describe("notificationForTransition", () => {
  it("is silent when the window is focused", () => {
    expect(
      notificationForTransition({
        ...base,
        prevKind: "working",
        next: { kind: "idle" },
        focused: true,
      }),
    ).toBeNull();
  });

  it("is silent when the setting is off", () => {
    expect(
      notificationForTransition({
        ...base,
        prevKind: "working",
        next: { kind: "idle" },
        enabled: false,
      }),
    ).toBeNull();
  });

  it("notifies when a working agent starts waiting", () => {
    expect(
      notificationForTransition({
        ...base,
        prevKind: "working",
        next: { kind: "blocked" },
      }),
    ).toEqual({ title: "Claude Code is waiting", body: "fix the bug" });
  });

  it("uses untitled when the session has no title", () => {
    expect(
      notificationForTransition({
        ...base,
        prevKind: "working",
        next: { kind: "blocked" },
        title: "  ",
      }),
    ).toEqual({ title: "Claude Code is waiting", body: "untitled" });
  });

  it("notifies when a working agent finishes", () => {
    expect(
      notificationForTransition({
        ...base,
        prevKind: "working",
        next: { kind: "idle" },
      }),
    ).toEqual({ title: "Claude Code finished", body: "fix the bug" });
  });

  it("notifies when a live session ends", () => {
    expect(
      notificationForTransition({
        ...base,
        prevKind: "idle",
        next: "ended",
      }),
    ).toEqual({ title: "Claude Code session ended", body: "fix the bug" });
  });

  it("is silent on first sight", () => {
    expect(
      notificationForTransition({
        ...base,
        prevKind: null,
        next: { kind: "working" },
      }),
    ).toBeNull();
  });

  it("is silent for working-to-working and idle-to-idle", () => {
    expect(
      notificationForTransition({
        ...base,
        prevKind: "working",
        next: { kind: "working" },
      }),
    ).toBeNull();
    expect(
      notificationForTransition({
        ...base,
        prevKind: "idle",
        next: { kind: "idle" },
      }),
    ).toBeNull();
  });

  it("is silent when activity becomes stale", () => {
    expect(
      notificationForTransition({
        ...base,
        prevKind: "working",
        next: { kind: "stale" },
      }),
    ).toBeNull();
    expect(
      notificationForTransition({
        ...base,
        prevKind: "idle",
        next: { kind: "stale" },
      }),
    ).toBeNull();
  });

  it("is silent when a stale session ends", () => {
    expect(
      notificationForTransition({
        ...base,
        prevKind: "stale",
        next: "ended",
      }),
    ).toBeNull();
  });

  it("is silent when an unseen session ends", () => {
    expect(
      notificationForTransition({
        ...base,
        prevKind: null,
        next: "ended",
      }),
    ).toBeNull();
  });
});

describe("trayTooltip", () => {
  it("is AgentLens when nothing is live", () => {
    expect(trayTooltip([])).toBe("AgentLens");
  });

  it("shows one session's agent and activity kind", () => {
    expect(
      trayTooltip([
        {
          agent: "claudeCode",
          activity: { kind: "working", detail: "thinking" },
          title: "long title ignored",
        },
      ]),
    ).toBe("Claude Code · working");
    expect(trayTooltip([{ agent: "grok", activity: { kind: "blocked" }, title: null }])).toBe(
      "Grok · blocked",
    );
  });

  it("counts live sessions and busy ones when several are live", () => {
    expect(
      trayTooltip([
        {
          agent: "claudeCode",
          activity: { kind: "working", detail: null },
          title: null,
        },
        { agent: "grok", activity: { kind: "idle" }, title: null },
        { agent: "grok", activity: { kind: "blocked" }, title: "x" },
      ]),
    ).toBe("3 live · 2 working");
  });
});
