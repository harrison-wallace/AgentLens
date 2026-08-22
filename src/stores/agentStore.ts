import { create } from "zustand";
import { agentSessions } from "../lib/tauri";
import type { AgentActivity, AgentEvent, AgentKind, SessionRef } from "../lib/protocol";

/**
 * One agent session the UI is tracking, with the extra counters the
 * session panel needs that the protocol `SessionRef` does not carry.
 *
 * `startedAt` is when we first saw the session (or the `sessionStarted`
 * timestamp). `toolCalls` is a running tally of `toolCall` events — not
 * re-derived from the backend, because the poller only reports deltas.
 * `filesTouched` is unique `toolCall.paths` for this session; uniqueness
 * lives in `filesBySession`, not on the store object.
 */
export interface AgentSession {
  id: string;
  agent: AgentKind;
  title: string | null;
  lastActivity: number;
  activity: AgentActivity;
  toolCalls: number;
  filesTouched: number;
  sidechainCalls: number;
  startedAt: number;
}

interface AgentStore {
  sessions: AgentSession[];
  /**
   * Bumped by `reset` so ambient notifications can tell a workspace switch
   * (wipe) from a real `sessionEnded`.
   */
  generation: number;
  /** Fold a batch of poller events into the session list. */
  apply: (events: AgentEvent[]) => void;
  /** Seed from `agent_sessions` (open / restore / reconnect). */
  refresh: () => Promise<void>;
  /** Drop everything — workspace closed or switched. */
  reset: () => void;
}

function fromRef(ref: SessionRef): AgentSession {
  return {
    id: ref.id,
    agent: ref.agent,
    title: ref.title,
    lastActivity: ref.lastActivity,
    activity: ref.activity,
    toolCalls: 0,
    filesTouched: 0,
    sidechainCalls: 0,
    // The protocol has no start time; last activity is the honest floor.
    startedAt: ref.lastActivity,
  };
}

function sameSession(session: AgentSession, agent: AgentKind, id: string): boolean {
  return session.agent === agent && session.id === id;
}

/** Pair when the event names an agent; id only for frames from an older daemon. */
function matchSession(session: AgentSession, agent: AgentKind | undefined, id: string): boolean {
  return agent ? sameSession(session, agent, id) : session.id === id;
}

export function sessionKey(agent: AgentKind, id: string): string {
  return `${agent}\0${id}`;
}

/** Unique workspace-relative paths seen on toolCall.paths, keyed by sessionKey. */
const filesBySession = new Map<string, Set<string>>();

function uniqueFiles(agent: AgentKind, id: string, paths: readonly string[]): number {
  const key = sessionKey(agent, id);
  let set = filesBySession.get(key);
  if (!set) {
    set = new Set();
    filesBySession.set(key, set);
  }
  for (const path of paths) set.add(path);
  return set.size;
}

/**
 * Insert `(agent, sessionId)` if the store does not already hold it, then
 * apply `update`. Keying on the pair stops a Claude session and a Grok
 * session that share an id from merging into one row.
 */
function upsert(
  sessions: AgentSession[],
  agent: AgentKind | undefined,
  sessionId: string,
  at: number,
  update: (session: AgentSession) => AgentSession,
): AgentSession[] {
  const existing = sessions.find((session) => matchSession(session, agent, sessionId));
  if (existing) {
    return sessions.map((session) =>
      matchSession(session, agent, sessionId) ? update(session) : session,
    );
  }
  return [
    update({
      id: sessionId,
      agent: agent ?? "claudeCode",
      title: null,
      lastActivity: at,
      activity: { kind: "working", detail: null },
      toolCalls: 0,
      filesTouched: 0,
      sidechainCalls: 0,
      startedAt: at,
    }),
    ...sessions,
  ];
}

/**
 * Sessions whose activity is not `stale`. Stale means the process is gone
 * or the heartbeat went cold — still listed in the raw store long enough
 * for a final event, but never drawn in the header or session panel.
 */
export function selectLiveSessions(sessions: readonly AgentSession[]): AgentSession[] {
  return sessions.filter((s) => s.activity.kind !== "stale");
}

export const useAgentStore = create<AgentStore>((set, get) => ({
  sessions: [],
  generation: 0,

  apply: (events) => {
    if (events.length === 0) return;
    let sessions = get().sessions;

    for (const event of events) {
      switch (event.kind) {
        case "sessionStarted": {
          // A re-start for a pair we already hold is a re-open after a
          // reconnect, not a second row.
          const existing = sessions.find((s) => sameSession(s, event.agent, event.sessionId));
          if (existing) {
            // Do not force `working`. The poller re-emits SessionStarted
            // after open/reconnect for sessions it just discovered; activity
            // already came from AgentSessions or a later ActivityChanged.
            sessions = sessions.map((s) =>
              sameSession(s, event.agent, event.sessionId)
                ? {
                    ...s,
                    agent: event.agent,
                    title: event.title ?? s.title,
                    lastActivity: event.at,
                  }
                : s,
            );
          } else {
            sessions = [
              {
                id: event.sessionId,
                agent: event.agent,
                title: event.title,
                lastActivity: event.at,
                activity: { kind: "working", detail: null },
                toolCalls: 0,
                filesTouched: 0,
                sidechainCalls: 0,
                startedAt: event.at,
              },
              ...sessions,
            ];
          }
          break;
        }
        case "sessionEnded": {
          for (const session of sessions) {
            if (matchSession(session, event.agent, event.sessionId)) {
              filesBySession.delete(sessionKey(session.agent, session.id));
            }
          }
          sessions = sessions.filter((s) => !matchSession(s, event.agent, event.sessionId));
          break;
        }
        case "activityChanged": {
          sessions = upsert(sessions, event.agent, event.sessionId, event.at, (s) => ({
            ...s,
            activity: event.activity,
            lastActivity: event.at,
          }));
          break;
        }
        case "toolCall": {
          sessions = upsert(sessions, event.agent, event.sessionId, event.at, (s) => ({
            ...s,
            toolCalls: s.toolCalls + 1,
            sidechainCalls: s.sidechainCalls + (event.sidechain ? 1 : 0),
            filesTouched: uniqueFiles(s.agent, s.id, event.paths),
            lastActivity: event.at,
          }));
          break;
        }
        case "assistantNote": {
          // Notes do not change activity or tallies; bump last-seen only
          // so a chatty session does not look abandoned. last-prompt records
          // carry no timestamp (at: 0) — a zero must not rewind the clock.
          sessions = upsert(sessions, event.agent, event.sessionId, event.at, (s) => ({
            ...s,
            lastActivity: Math.max(s.lastActivity, event.at),
          }));
          break;
        }
      }
    }

    set({ sessions });
  },

  refresh: async () => {
    try {
      const refs = await agentSessions();
      // Preserve tool tallies, file/sidechain counters, and startedAt for
      // sessions we already knew — a refresh mid-session would otherwise
      // wipe the panel stats and rewind "running for".
      const prev = new Map(get().sessions.map((s) => [sessionKey(s.agent, s.id), s]));
      const sessions = refs.map((ref) => {
        const known = prev.get(sessionKey(ref.agent, ref.id));
        if (!known) return fromRef(ref);
        return {
          ...fromRef(ref),
          toolCalls: known.toolCalls,
          filesTouched: known.filesTouched,
          sidechainCalls: known.sidechainCalls,
          startedAt: known.startedAt,
        };
      });
      const keep = new Set(sessions.map((s) => sessionKey(s.agent, s.id)));
      for (const key of [...filesBySession.keys()]) {
        if (!keep.has(key)) filesBySession.delete(key);
      }
      set({ sessions });
    } catch {
      // No backend (plain browser / tests) — leave the list alone rather
      // than flash empty over whatever events already arrived.
    }
  },

  reset: () => {
    filesBySession.clear();
    set({ sessions: [], generation: get().generation + 1 });
  },
}));
