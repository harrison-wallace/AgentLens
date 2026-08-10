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
 */
export interface AgentSession {
  id: string;
  agent: AgentKind;
  title: string | null;
  lastActivity: number;
  activity: AgentActivity;
  toolCalls: number;
  startedAt: number;
}

interface AgentStore {
  sessions: AgentSession[];
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
    // The protocol has no start time; last activity is the honest floor.
    startedAt: ref.lastActivity,
  };
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

  apply: (events) => {
    if (events.length === 0) return;
    let sessions = get().sessions;

    for (const event of events) {
      switch (event.kind) {
        case "sessionStarted": {
          // A re-start for an id we already hold is a re-open after a
          // reconnect, not a second row.
          const existing = sessions.find((s) => s.id === event.sessionId);
          if (existing) {
            sessions = sessions.map((s) =>
              s.id === event.sessionId
                ? {
                    ...s,
                    agent: event.agent,
                    title: event.title ?? s.title,
                    lastActivity: event.at,
                    activity: { kind: "working", detail: null },
                    startedAt: event.at,
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
                startedAt: event.at,
              },
              ...sessions,
            ];
          }
          break;
        }
        case "sessionEnded": {
          sessions = sessions.filter((s) => s.id !== event.sessionId);
          break;
        }
        case "activityChanged": {
          sessions = sessions.map((s) =>
            s.id === event.sessionId
              ? { ...s, activity: event.activity, lastActivity: event.at }
              : s,
          );
          break;
        }
        case "toolCall": {
          sessions = sessions.map((s) =>
            s.id === event.sessionId
              ? {
                  ...s,
                  toolCalls: s.toolCalls + 1,
                  lastActivity: event.at,
                }
              : s,
          );
          break;
        }
        case "assistantNote": {
          // Notes do not change activity or tallies; bump last-seen only
          // so a chatty session does not look abandoned. last-prompt records
          // carry no timestamp (at: 0) — a zero must not rewind the clock.
          sessions = sessions.map((s) =>
            s.id === event.sessionId
              ? { ...s, lastActivity: Math.max(s.lastActivity, event.at) }
              : s,
          );
          break;
        }
      }
    }

    set({ sessions });
  },

  refresh: async () => {
    try {
      const refs = await agentSessions();
      // Preserve tool tallies and startedAt for sessions we already knew
      // about — a refresh mid-session would otherwise wipe the panel stats.
      const prev = new Map(get().sessions.map((s) => [s.id, s]));
      set({
        sessions: refs.map((ref) => {
          const known = prev.get(ref.id);
          if (!known) return fromRef(ref);
          return {
            ...fromRef(ref),
            toolCalls: known.toolCalls,
            startedAt: known.startedAt,
          };
        }),
      });
    } catch {
      // No backend (plain browser / tests) — leave the list alone rather
      // than flash empty over whatever events already arrived.
    }
  },

  reset: () => set({ sessions: [] }),
}));
