import { agentLabel } from "./agent";
import type { AgentActivity, AgentKind } from "./protocol";

export type NotifyKind = "blocked" | "idle" | "ended";

function sessionTitle(title: string): string {
  return title.trim() || "untitled";
}

export function notificationForTransition(args: {
  prevKind: "working" | "blocked" | "idle" | "stale" | null;
  next: { kind: "working" | "blocked" | "idle" | "stale" } | "ended" | null;
  focused: boolean;
  enabled: boolean;
  agentLabel: string;
  title: string;
}): { title: string; body: string } | null {
  if (args.focused || !args.enabled) return null;

  const body = sessionTitle(args.title);

  if (args.next === "ended") {
    if (args.prevKind === null || args.prevKind === "stale") return null;
    return { title: `${args.agentLabel} session ended`, body };
  }

  if (args.next === null || args.prevKind === null) return null;

  if (args.prevKind === "working" && args.next.kind === "blocked") {
    return { title: `${args.agentLabel} is waiting`, body };
  }
  if (args.prevKind === "working" && args.next.kind === "idle") {
    return { title: `${args.agentLabel} finished`, body };
  }

  return null;
}

export function trayTooltip(
  sessions: { agent: AgentKind; activity: AgentActivity; title: string | null }[],
): string {
  if (sessions.length === 0) return "AgentLens";
  if (sessions.length === 1) {
    const session = sessions[0];
    if (!session) return "AgentLens";
    return `${agentLabel(session.agent)} · ${session.activity.kind}`;
  }
  const busy = sessions.filter(
    (session) => session.activity.kind === "working" || session.activity.kind === "blocked",
  ).length;
  return `${sessions.length} live · ${busy} working`;
}
