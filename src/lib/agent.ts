import type { AgentKind } from "./protocol";

export function agentLabel(agent: AgentKind): string {
  switch (agent) {
    case "claudeCode":
      return "Claude Code";
    case "grok":
      return "Grok";
  }
}

export function agentMark(agent: AgentKind): string {
  switch (agent) {
    case "claudeCode":
      return "C";
    case "grok":
      return "G";
  }
}

/** Identity colour — not activity state. */
export function agentTextClass(agent: AgentKind): string {
  return agent === "grok" ? "text-agent-grok" : "text-agent-claude";
}
