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

function ended(sessionId: string, agent: "claudeCode" | "grok" = "claudeCode", at = 1): AgentEvent {
  return { kind: "sessionEnded", sessionId, agent, at };
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
      filesTouched: 0,
      sidechainCalls: 0,
      startedAt: 1_000,
    });
    expect(sessions[0]?.activity).toEqual({ kind: "working", detail: null });
  });

  it("removes a session on sessionEnded", () => {
    const store = useAgentStore.getState();
    store.apply([started("s1"), started("s2", "grok", 2_000)]);
    store.apply([ended("s1", "claudeCode", 3_000)]);

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
        agent: "claudeCode",
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
        agent: "claudeCode",
        at: 1_500,
        tool: "Edit",
        summary: "fix",
        paths: ["a.ts"],
        sidechain: false,
      },
      {
        kind: "toolCall",
        sessionId: "s1",
        agent: "claudeCode",
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
        agent: "grok",
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
      agent: "grok",
      toolCalls: 1,
      lastActivity: 1,
    });
  });

  it("inserts a session on activityChanged for an unknown id", () => {
    useAgentStore.getState().apply([
      {
        kind: "activityChanged",
        sessionId: "late",
        agent: "claudeCode",
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
    useAgentStore.getState().apply([ended("never")]);
    expect(useAgentStore.getState().sessions).toHaveLength(0);
  });

  it("assistantNote with at: 0 does not clobber lastActivity", () => {
    useAgentStore.getState().apply([
      started("s1"),
      {
        kind: "toolCall",
        sessionId: "s1",
        agent: "claudeCode",
        at: 1_500,
        tool: "Edit",
        summary: null,
        paths: [],
        sidechain: false,
      },
      {
        kind: "assistantNote",
        sessionId: "s1",
        agent: "claudeCode",
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
        agent: "grok",
        at: 3_000,
        activity: { kind: "stale" },
      },
      {
        kind: "activityChanged",
        sessionId: "live",
        agent: "claudeCode",
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
        agent: "claudeCode",
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
    expect(sessions[0]?.startedAt).toBe(1_000);
  });

  it("counts unique filesTouched and sidechainCalls on toolCall", () => {
    const store = useAgentStore.getState();
    store.apply([
      started("s1"),
      {
        kind: "toolCall",
        sessionId: "s1",
        agent: "claudeCode",
        at: 1_100,
        tool: "Edit",
        summary: null,
        paths: ["a.ts"],
        sidechain: true,
      },
      {
        kind: "toolCall",
        sessionId: "s1",
        agent: "claudeCode",
        at: 1_150,
        tool: "Read",
        summary: null,
        paths: ["a.ts"],
        sidechain: false,
      },
    ]);
    expect(useAgentStore.getState().sessions[0]?.filesTouched).toBe(1);
    expect(useAgentStore.getState().sessions[0]?.sidechainCalls).toBe(1);

    store.apply([
      {
        kind: "toolCall",
        sessionId: "s1",
        agent: "claudeCode",
        at: 1_200,
        tool: "Edit",
        summary: null,
        paths: ["b.ts"],
        sidechain: false,
      },
    ]);
    const s = useAgentStore.getState().sessions[0];
    expect(s?.filesTouched).toBe(2);
    expect(s?.sidechainCalls).toBe(1);
    expect(s?.toolCalls).toBe(3);
  });

  it("does not rewind startedAt on a repeated sessionStarted", () => {
    useAgentStore.getState().apply([
      started("s1"),
      {
        kind: "sessionStarted",
        sessionId: "s1",
        agent: "claudeCode",
        title: "retitled",
        at: 9_000,
      },
    ]);
    expect(useAgentStore.getState().sessions[0]?.startedAt).toBe(1_000);
    expect(useAgentStore.getState().sessions[0]?.title).toBe("retitled");
  });

  it("does not force working on a repeated sessionStarted", () => {
    useAgentStore.getState().apply([
      started("s1"),
      {
        kind: "activityChanged",
        sessionId: "s1",
        agent: "claudeCode",
        at: 2_000,
        activity: { kind: "idle" },
      },
      {
        kind: "sessionStarted",
        sessionId: "s1",
        agent: "claudeCode",
        title: "still here",
        at: 3_000,
      },
    ]);
    const session = useAgentStore.getState().sessions[0];
    expect(session?.activity).toEqual({ kind: "idle" });
    expect(session?.title).toBe("still here");
    expect(session?.lastActivity).toBe(3_000);
  });

  it("bumps generation on reset so a wipe is not a session end", () => {
    const gen = useAgentStore.getState().generation;
    useAgentStore.getState().apply([started("s1")]);
    useAgentStore.getState().reset();
    expect(useAgentStore.getState().generation).toBe(gen + 1);
    expect(useAgentStore.getState().sessions).toHaveLength(0);
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
        agent: "claudeCode",
        at: 1_100,
        tool: "Edit",
        summary: null,
        paths: ["a.ts"],
        sidechain: true,
      },
    ]);

    await useAgentStore.getState().refresh();
    const sessions = useAgentStore.getState().sessions;
    expect(sessions).toHaveLength(2);
    const s1 = sessions.find((s) => s.id === "s1");
    expect(s1?.toolCalls).toBe(1);
    expect(s1?.filesTouched).toBe(1);
    expect(s1?.sidechainCalls).toBe(1);
    expect(s1?.startedAt).toBe(1_000);
    expect(s1?.activity).toEqual({ kind: "idle" });
    expect(sessions.find((s) => s.id === "s2")?.toolCalls).toBe(0);
  });

  it("keeps Claude and Grok sessions with the same id as two rows", () => {
    const store = useAgentStore.getState();
    store.apply([started("same", "claudeCode"), started("same", "grok", 2_000)]);
    store.apply([
      {
        kind: "toolCall",
        sessionId: "same",
        agent: "grok",
        at: 2_100,
        tool: "Edit",
        summary: null,
        paths: [],
        sidechain: false,
      },
    ]);
    store.apply([ended("same", "claudeCode", 3_000)]);

    const sessions = useAgentStore.getState().sessions;
    expect(sessions).toHaveLength(1);
    expect(sessions[0]).toMatchObject({ id: "same", agent: "grok", toolCalls: 1 });
  });

  it("an event with no agent still updates the existing row by id", () => {
    const store = useAgentStore.getState();
    store.apply([started("s1")]);
    store.apply([
      {
        kind: "toolCall",
        sessionId: "s1",
        at: 2_000,
        tool: "Edit",
        summary: null,
        paths: [],
        sidechain: false,
      },
    ]);
    expect(useAgentStore.getState().sessions).toHaveLength(1);
    expect(useAgentStore.getState().sessions[0]?.toolCalls).toBe(1);

    store.apply([{ kind: "sessionEnded", sessionId: "s1", at: 3_000 }]);
    expect(useAgentStore.getState().sessions).toHaveLength(0);
  });
});
