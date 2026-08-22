import { agentLabel } from "../lib/agent";
import DotMatrix from "./DotMatrix";
import { selectLiveSessions, useAgentStore, type AgentSession } from "../stores/agentStore";
import type { AgentActivity } from "../lib/protocol";

/** Header is 40 px; leave a little air so chips don't touch the border. */
const CHIP_SIZE = 14;
const CHIP_DOT = 2;
const MAX_CHIPS = 4;

function activityColor(activity: AgentActivity): string {
  switch (activity.kind) {
    case "working":
      return "var(--color-agent-working)";
    case "blocked":
      return "var(--color-agent-blocked)";
    case "idle":
      return "var(--color-agent-idle)";
    case "stale":
      return "var(--color-agent-stale)";
  }
}

function chipTitle(session: AgentSession): string {
  const name = agentLabel(session.agent);
  const title = session.title?.trim() || "untitled session";
  if (session.activity.kind === "working" && session.activity.detail) {
    return `${name} · ${title} · ${session.activity.detail}`;
  }
  return `${name} · ${title}`;
}

/**
 * One live-session chip for the header cluster. Colour = activity state;
 * ripple vs snake = which agent; blocked alone gets speed + bloom.
 */
function SessionChip({ session }: { session: AgentSession }) {
  const activity = session.activity;
  const animated = activity.kind === "working" || activity.kind === "blocked";
  const blocked = activity.kind === "blocked";

  return (
    <span
      className="flex items-center justify-center"
      title={chipTitle(session)}
      // pointer-events restored here so the title tooltip works; the
      // cluster wrapper is pointer-events-none so the path underneath
      // still receives clicks outside the chips.
      style={{ pointerEvents: "auto" }}
    >
      <DotMatrix
        variant={session.agent === "grok" ? "snake" : "ripple"}
        color={activityColor(activity)}
        animated={animated}
        speed={blocked ? 2.2 : 1}
        bloom={blocked}
        size={CHIP_SIZE}
        dotSize={CHIP_DOT}
        ariaLabel={chipTitle(session)}
      />
    </span>
  );
}

/**
 * Centred header overlay of live agent chips. Renders `null` when nothing
 * is live so no animation frames run in the common no-agent case.
 *
 * Absolutely positioned: the path span is `flex-1 truncate`, so a normal
 * flex child would sit wherever the path ends rather than the header centre.
 */
export default function AgentActivityCluster() {
  const sessions = useAgentStore((s) => s.sessions);
  const live = selectLiveSessions(sessions);

  if (live.length === 0) return null;

  const shown = live.slice(0, MAX_CHIPS);
  const overflow = live.length - shown.length;

  return (
    <div
      // Opaque, in the header's own colour: this is an overlay sitting on top
      // of a `flex-1 truncate` path, so without a background the chips land on
      // the path's glyphs whenever the workspace path is long enough to reach
      // the centre — which is most of them.
      className="pointer-events-none absolute left-1/2 top-1/2 flex -translate-x-1/2 -translate-y-1/2 items-center gap-2 rounded bg-surface-raised px-2"
      aria-label="Agent activity"
    >
      {shown.map((session) => (
        <SessionChip key={`${session.agent}:${session.id}`} session={session} />
      ))}
      {overflow > 0 && (
        <span className="text-[11px] text-text-muted" title={`${overflow} more live sessions`}>
          +{overflow}
        </span>
      )}
    </div>
  );
}
