import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  EVENT_AGENT_EVENTS,
  EVENT_ATTRIBUTED,
  EVENT_CONNECTION,
  EVENT_FS_CHANGES,
  EVENT_GIT_STATUS,
  EVENT_WATCHER_STATUS,
  type AgentEvent,
  type AttributedEvent,
  type ConnectionInfo,
  type FsEvent,
  type GitStatusSnapshot,
  type WatcherStatus,
} from "./protocol";

/** Thin typed wrapper over `listen("fs-changes", ...)`. */
export function onFsChanges(cb: (events: FsEvent[]) => void): Promise<UnlistenFn> {
  return listen<FsEvent[]>(EVENT_FS_CHANGES, (event) => cb(event.payload));
}

/** Thin typed wrapper over `listen("git-status", ...)`. */
export function onGitStatus(cb: (snapshot: GitStatusSnapshot) => void): Promise<UnlistenFn> {
  return listen<GitStatusSnapshot>(EVENT_GIT_STATUS, (event) => cb(event.payload));
}

/** Thin typed wrapper over `listen("watcher-status", ...)`. */
export function onWatcherStatus(cb: (status: WatcherStatus) => void): Promise<UnlistenFn> {
  return listen<WatcherStatus>(EVENT_WATCHER_STATUS, (event) => cb(event.payload));
}

/**
 * Thin typed wrapper over `listen("connection", ...)`. Fires on connect and
 * on every state change a remote link goes through afterwards.
 */
export function onConnection(cb: (info: ConnectionInfo) => void): Promise<UnlistenFn> {
  return listen<ConnectionInfo>(EVENT_CONNECTION, (event) => cb(event.payload));
}

/**
 * Thin typed wrapper over `listen("agent-events", ...)`. Batches from the
 * background poller: session lifecycle, tool calls, and activity changes.
 */
export function onAgentEvents(cb: (events: AgentEvent[]) => void): Promise<UnlistenFn> {
  return listen<AgentEvent[]>(EVENT_AGENT_EVENTS, (event) => cb(event.payload));
}

/**
 * Thin typed wrapper over `listen("attributed-changes", ...)`. Arrives after
 * the raw fs-changes batch — the feed upgrades rows in place.
 */
export function onAttributedChanges(cb: (events: AttributedEvent[]) => void): Promise<UnlistenFn> {
  return listen<AttributedEvent[]>(EVENT_ATTRIBUTED, (event) => cb(event.payload));
}
