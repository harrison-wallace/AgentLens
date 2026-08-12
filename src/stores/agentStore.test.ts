import { beforeEach, describe, expect, it, vi } from "vitest";
import { selectLiveSessions, useAgentStore } from "./agentStore";
import type { AgentEvent, SessionRef } from "../lib/protocol";

vi.mock("../lib/tauri", () => ({
  agentSessions: vi.fn(async (): Promise<SessionRef[]> => []),
}));

function started(
  sessionId: string,
  agent: "claudeCode" | "grok" = "claudeCode",
  at = 1_000,
): AgentEvent {
  return { kind: "sessionStarted", sessionId, agent, title: "t", at };
}

describe("agentStore", () => {
  beforeEach(() => {
    useAgentStore.getState().reset();
  });

  it("adds a session on sessionStarted", () => {
    useAgentStore.getState().apply([started("s1")]);
    const sessions = useAgentStore.getState().sessions;
    expect(sessions).toHaveLength(1);
    expect(sessions[0]).toMatchObject({
      id: "s1",
      agent: "claudeCode",
      title: "t",
      toolCalls: 0,
      startedAt: 1_000,
    });
    expect(sessions[0]?.activity).toEqual({ kind: "working", detail: null });
  });

  it("removes a session on sessionEnded", () => {
    const store = useAgentStore.getState();
    store.apply([started("s1"), started("s2", "grok", 2_000)]);
    store.apply([{ kind: "sessionEnded", sessionId: "s1", at: 3_000 }]);

    const sessions = useAgentStore.getState().sessions;
    expect(sessions).toHaveLength(1);
    expect(sessions[0]?.id).toBe("s2");
  });

  it("updates activity on activityChanged", () => {
    useAgentStore.getState().apply([
      started("s1"),
      {
        kind: "activityChanged",
        sessionId: "s1",
        at: 2_000,
        activity: { kind: "blocked" },
      },
    ]);
    expect(useAgentStore.getState().sessions[0]?.activity).toEqual({ kind: "blocked" });
    expect(useAgentStore.getState().sessions[0]?.lastActivity).toBe(2_000);
  });

  it("bumps tool tally and lastActivity on toolCall", () => {
    useAgentStore.getState().apply([
      started("s1"),
      {
        kind: "toolCall",
        sessionId: "s1",
        at: 1_500,
        tool: "Edit",
        summary: "fix",
        paths: ["a.ts"],
        sidechain: false,
      },
      {
        kind: "toolCall",
        sessionId: "s1",
        at: 1_600,
        tool: "Read",
        summary: null,
        paths: [],
        sidechain: false,
      },
    ]);
    const s = useAgentStore.getState().sessions[0];
    expect(s?.toolCalls).toBe(2);
    expect(s?.lastActivity).toBe(1_600);
  });

  it("inserts a session on toolCall for an unknown id", () => {
    useAgentStore.getState().apply([
      {
        kind: "toolCall",
        sessionId: "ghost",
        at: 1,
        tool: "Edit",
        summary: null,
        paths: [],
        sidechain: false,
      },
    ]);
    const sessions = useAgentStore.getState().sessions;
    expect(sessions).toHaveLength(1);
    expect(sessions[0]).toMatchObject({
      id: "ghost",
      toolCalls: 1,
      lastActivity: 1,
    });
  });

  it("inserts a session on activityChanged for an unknown id", () => {
    useAgentStore.getState().apply([
      {
        kind: "activityChanged",
        sessionId: "late",
        at: 2_000,
        activity: { kind: "blocked" },
      },
    ]);
    const sessions = useAgentStore.getState().sessions;
    expect(sessions).toHaveLength(1);
    expect(sessions[0]?.id).toBe("late");
    expect(sessions[0]?.activity).toEqual({ kind: "blocked" });
  });

  it("does not insert on sessionEnded for an unknown id", () => {
    useAgentStore.getState().apply([{ kind: "sessionEnded", sessionId: "never", at: 1 }]);
    expect(useAgentStore.getState().sessions).toHaveLength(0);
  });

  it("assistantNote with at: 0 does not clobber lastActivity", () => {
    useAgentStore.getState().apply([
      started("s1"),
      {
        kind: "toolCall",
        sessionId: "s1",
        at: 1_500,
        tool: "Edit",
        summary: null,
        paths: [],
        sidechain: false,
      },
      {
        kind: "assistantNote",
        sessionId: "s1",
        at: 0,
        text: "last prompt with no timestamp",
      },
    ]);
    expect(useAgentStore.getState().sessions[0]?.lastActivity).toBe(1_500);
  });

  it("excludes stale sessions from the live selector", () => {
    useAgentStore.getState().apply([
      started("live"),
      started("dead", "grok", 2_000),
      {
        kind: "activityChanged",
        sessionId: "dead",
        at: 3_000,
        activity: { kind: "stale" },
      },
      {
        kind: "activityChanged",
        sessionId: "live",
        at: 3_100,
        activity: { kind: "idle" },
      },
    ]);
    const live = selectLiveSessions(useAgentStore.getState().sessions);
    expect(live.map((s) => s.id)).toEqual(["live"]);
  });

  it("does not invent a second row when sessionStarted repeats an id", () => {
    useAgentStore.getState().apply([
      started("s1"),
      {
        kind: "toolCall",
        sessionId: "s1",
        at: 1_100,
        tool: "Edit",
        summary: null,
        paths: [],
        sidechain: false,
      },
      {
        kind: "sessionStarted",
        sessionId: "s1",
        agent: "claudeCode",
        title: "retitled",
        at: 2_000,
      },
    ]);
    const sessions = useAgentStore.getState().sessions;
    expect(sessions).toHaveLength(1);
    expect(sessions[0]?.title).toBe("retitled");
    // Tool tally survives a re-announce; only identity fields refresh.
    expect(sessions[0]?.toolCalls).toBe(1);
  });

  it("seeds from refresh and preserves known tool tallies", async () => {
    const { agentSessions } = await import("../lib/tauri");
    vi.mocked(agentSessions).mockResolvedValueOnce([
      {
        id: "s1",
        agent: "claudeCode",
        title: "from backend",
        lastActivity: 9_000,
        activity: { kind: "idle" },
      },
      {
        id: "s2",
        agent: "grok",
        title: null,
        lastActivity: 8_000,
        activity: { kind: "working", detail: "thinking" },
      },
    ]);

    useAgentStore.getState().apply([
      started("s1"),
      {
        kind: "toolCall",
        sessionId: "s1",
        at: 1_100,
        tool: "Edit",
        summary: null,
        paths: [],
        sidechain: false,
      },
    ]);

    await useAgentStore.getState().refresh();
    const sessions = useAgentStore.getState().sessions;
    expect(sessions).toHaveLength(2);
    const s1 = sessions.find((s) => s.id === "s1");
    expect(s1?.toolCalls).toBe(1);
    expect(s1?.activity).toEqual({ kind: "idle" });
    expect(sessions.find((s) => s.id === "s2")?.toolCalls).toBe(0);
  });
});
