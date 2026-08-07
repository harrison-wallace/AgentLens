import { useEffect, useState } from "react";
import { selectLiveSessions, useAgentStore, type AgentSession } from "../stores/agentStore";
import { useLayoutStore } from "../stores/layoutStore";
import type { AgentActivity, AgentKind } from "../lib/protocol";

/** How often "running for 3m" labels refresh. */
const TICK_INTERVAL_MS = 15_000;

function agentLabel(agent: AgentKind): string {
  switch (agent) {
    case "claudeCode":
      return "Claude Code";
    case "grok":
      return "Grok";
  }
}

function agentMark(agent: AgentKind): string {
  switch (agent) {
    case "claudeCode":
      return "C";
    case "grok":
      return "G";
  }
}

/** Activity in words for the panel — room enough for the working detail. */
function activityWords(activity: AgentActivity): string {
  switch (activity.kind) {
    case "working":
      return activity.detail ? `working — ${activity.detail}` : "working";
    case "blocked":
      return "blocked";
    case "idle":
      return "idle";
    case "stale":
      return "stale";
  }
}

function runningLabel(startedAt: number, now: number): string {
  const diffMs = Math.max(0, now - startedAt);
  if (diffMs < 60_000) return `${Math.max(1, Math.floor(diffMs / 1_000))}s`;
  if (diffMs < 3_600_000) return `${Math.floor(diffMs / 60_000)}m`;
  return `${Math.floor(diffMs / 3_600_000)}h`;
}

function SessionRow({ session, now }: { session: AgentSession; now: number }) {
  const title = session.title?.trim() || "untitled";
  return (
    <div className="flex flex-col gap-0.5 px-3 py-1.5">
      <div className="flex min-w-0 items-baseline gap-2 text-xs">
        <span
          className="w-3 shrink-0 text-center text-[11px] font-medium text-text-muted"
          title={agentLabel(session.agent)}
        >
          {agentMark(session.agent)}
        </span>
        <span className="min-w-0 flex-1 truncate text-text-body" title={title}>
          {title}
        </span>
        <span className="shrink-0 text-[11px] tabular-nums text-text-muted">
          {runningLabel(session.startedAt, now)}
        </span>
      </div>
      <div className="flex min-w-0 items-baseline gap-2 pl-5 text-[11px] text-text-muted">
        <span className="min-w-0 flex-1 truncate">{activityWords(session.activity)}</span>
        <span className="shrink-0 tabular-nums" title="Tool calls this session">
          {session.toolCalls} tool{session.toolCalls === 1 ? "" : "s"}
        </span>
      </div>
    </div>
  );
}

/**
 * Live agent sessions above the activity feed. A list from the outset —
 * concurrent sessions is the normal case. Collapsed state lives in
 * layoutStore with the other panel chrome flags.
 *
 * Renders nothing when no session is live, so the common idle case costs
 * no DOM and no timers.
 */
export default function SessionPanel() {
  const sessions = useAgentStore((s) => s.sessions);
  const live = selectLiveSessions(sessions);
  const collapsed = useLayoutStore((s) => s.sessionsCollapsed);
  const toggle = useLayoutStore((s) => s.toggleSessions);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (live.length === 0) return;
    const id = setInterval(() => setNow(Date.now()), TICK_INTERVAL_MS);
    return () => clearInterval(id);
  }, [live.length]);

  if (live.length === 0) return null;

  return (
    <div className="shrink-0 border-b border-border">
      <button
        type="button"
        onClick={toggle}
        className="flex h-7 w-full items-center gap-2 px-3 text-left text-[11px] text-text-muted hover:bg-hover hover:text-text"
        aria-expanded={!collapsed}
      >
        <span className="w-3 shrink-0 text-center" aria-hidden>
          {collapsed ? "▸" : "▾"}
        </span>
        <span className="font-medium tracking-wide uppercase">Sessions</span>
        <span className="tabular-nums">{live.length}</span>
      </button>
      {!collapsed && (
        <ul className="max-h-40 overflow-y-auto border-t border-border">
          {live.map((session) => (
            <li key={session.id} className="border-b border-border last:border-b-0">
              <SessionRow session={session} now={now} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
