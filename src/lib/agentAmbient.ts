import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { agentLabel } from "./agent";
import { notificationForTransition, trayTooltip } from "./agentNotify";
import {
  selectLiveSessions,
  sessionKey,
  useAgentStore,
  type AgentSession,
} from "../stores/agentStore";
import { useSettingsStore } from "../stores/settingsStore";
import type { AgentActivity, AgentKind } from "./protocol";

type PrevSession = {
  kind: AgentActivity["kind"];
  agent: AgentKind;
  title: string | null;
};

/**
 * OS notifications (unfocused only) and tray tooltip. Failures are silence —
 * never a toast about notifications or the tray.
 */
export function startAgentAmbient(): () => void {
  let cancelled = false;
  let focused = true;
  const prev = new Map<string, PrevSession>();
  let seenGeneration = useAgentStore.getState().generation;

  const sendNote = (title: string, body: string) => {
    void (async () => {
      try {
        let granted = await isPermissionGranted();
        if (!granted) {
          granted = (await requestPermission()) === "granted";
        }
        // Focus can change while permission is in flight; do not ping a
        // window the user is looking at.
        if (!granted || cancelled || focused) return;
        await sendNotification({ title, body });
      } catch {
        // Permission or send failures stay silent.
      }
    })();
  };

  const setTray = (text: string) => {
    if (cancelled) return;
    void invoke("set_tray_status", { text }).catch(() => {
      // No tray, or Linux (tooltips unsupported).
    });
  };

  const adopt = (sessions: AgentSession[]) => {
    prev.clear();
    for (const session of sessions) {
      prev.set(sessionKey(session.agent, session.id), {
        kind: session.activity.kind,
        agent: session.agent,
        title: session.title,
      });
    }
  };

  const process = (sessions: AgentSession[], generation: number) => {
    if (generation !== seenGeneration) {
      seenGeneration = generation;
      adopt(sessions);
      const live = selectLiveSessions(sessions);
      setTray(
        trayTooltip(live.map((s) => ({ agent: s.agent, activity: s.activity, title: s.title }))),
      );
      return;
    }

    const enabled = useSettingsStore.getState().app.notifyAgentState;
    const seen = new Set<string>();

    for (const session of sessions) {
      const key = sessionKey(session.agent, session.id);
      seen.add(key);
      const previous = prev.get(key);
      const note = notificationForTransition({
        prevKind: previous?.kind ?? null,
        next: session.activity,
        focused,
        enabled,
        agentLabel: agentLabel(session.agent),
        title: session.title ?? "",
      });
      prev.set(key, {
        kind: session.activity.kind,
        agent: session.agent,
        title: session.title,
      });
      if (note) sendNote(note.title, note.body);
    }

    for (const [key, previous] of [...prev.entries()]) {
      if (seen.has(key)) continue;
      const note = notificationForTransition({
        prevKind: previous.kind,
        next: "ended",
        focused,
        enabled,
        agentLabel: agentLabel(previous.agent),
        title: previous.title ?? "",
      });
      prev.delete(key);
      if (note) sendNote(note.title, note.body);
    }

    const live = selectLiveSessions(sessions);
    setTray(
      trayTooltip(live.map((s) => ({ agent: s.agent, activity: s.activity, title: s.title }))),
    );
  };

  const focusListen = (async () => {
    try {
      const win = getCurrentWindow();
      focused = await win.isFocused();
      if (cancelled) return () => undefined;
      const unlisten = await win.onFocusChanged((event) => {
        focused = event.payload;
      });
      if (cancelled) {
        unlisten();
        return () => undefined;
      }
      return unlisten;
    } catch {
      // Plain browser / tests: treat as focused so nothing notifies.
      focused = true;
      return () => undefined;
    }
  })();

  {
    const state = useAgentStore.getState();
    process(state.sessions, state.generation);
  }
  const unsub = useAgentStore.subscribe((state) => {
    process(state.sessions, state.generation);
  });

  return () => {
    cancelled = true;
    unsub();
    void focusListen.then((unlisten) => unlisten());
  };
}
