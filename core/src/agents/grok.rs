//! Grok provider.
//!
//! Grok keeps sessions under `~/.grok/sessions/<encoded-cwd>/<session-id>/`,
//! with a typed `events.jsonl` stream rather than a conversation transcript.
//! That is a better activity signal than Claude Code's two-valued registry
//! status: phases are stated (`streaming_reasoning`, `permission_prompt`, …)
//! and map almost directly onto `AgentActivity`.
//!
//! The cwd encoding is percent-encoding, so it is reversible — none of Claude
//! Code's lossy-slug collision problem. `events.jsonl` carries tool *names*
//! only, never arguments or file paths. Paths come from a separate
//! `chat_history.jsonl` beside it: that file holds prompts and edit bodies,
//! so the only thing read out of it is path-bearing argument keys, and
//! nothing else is retained.
//!
//! Live sessions are listed in `<root>/active_sessions.json` (verified
//! 2026-08-12 against a running Grok session): a JSON array of
//! `{ cwd, opened_at, pid, session_id }`. `session_id` is the child
//! directory name under `sessions/<percent-encoded-cwd>/`. The file carries
//! no activity state, so a listed session is `Idle`. A listed `pid` that is
//! gone falls through to recency — prefer under-claiming.
//!
//! The format is not a stable API. Unknown event types are skipped, malformed
//! lines are counted and ignored, and an unknown `schema_version` major stops
//! extraction for that session rather than guessing.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::{has_json_children, is_within_workspace, read_tail, AgentProvider, ParseStats};
use crate::paths::to_workspace_relative;
use crate::protocol::{AgentActivity, AgentEvent, AgentKind, SessionRef};

/// Cap on a single `poll`, so a chatty session can't produce an unbounded
/// batch. Grok is event-dense (hundreds of `phase_changed` per turn).
const MAX_EVENTS_PER_POLL: usize = 500;

/// How long a session may sit in `permission_prompt` before it counts as
/// `Blocked`.
///
/// Entering the phase is **not** blocked: under `--always-approve` every
/// prompt still fires and resolves in about zero milliseconds (measured
/// across 1139 real events: median `wait_ms` 0, max 398). `Blocked` is only
/// correct when the session is *still sitting in* that phase — no later
/// event for longer than this threshold.
const PERMISSION_PROMPT_DWELL_MS: i64 = 1000;

/// How recently a Grok session must have been written to for discovery to
/// treat it as live when it is **not** in `active_sessions.json`.
///
/// The registry is the primary live-process signal. Recency covers a
/// missing, empty, or stale registry (Grok has been seen to leave the
/// file empty). Five minutes mirrors the heartbeat fallback in claude.rs.
/// Prefer under-claiming: a session wrongly shown as live is worse than
/// one missing for a few seconds.
const FRESH_WINDOW_MS: i64 = 5 * 60 * 1000;

/// Major `schema_version` this provider understands. Anything whose major
/// does not match is skipped entirely for that session.
const KNOWN_SCHEMA_MAJOR: &str = "1";

/// Tool-argument keys that name a file. Mirrors Claude Code's set, plus
/// Grok's `target_file` (`read_file`). `run_terminal_command` has none of
/// these — a shell command is not a path, and parsing one would be a guess.
const PATH_INPUT_KEYS: [&str; 3] = ["file_path", "target_file", "path"];

#[derive(Default)]
pub struct Grok {
    /// Byte offset already consumed, per `events.jsonl`.
    offsets: HashMap<PathBuf, u64>,
    /// Last activity emitted per session, so `ActivityChanged` only fires on
    /// a real transition.
    last_activity: HashMap<String, AgentActivity>,
    /// Sessions whose `schema_version` major we do not know. Once set, every
    /// further poll returns nothing rather than guessing at a new format.
    unsupported: HashSet<String>,
    /// Most recent phase seen for a session, plus the event timestamp it
    /// arrived at. Used for the `permission_prompt` dwell rule: Blocked only
    /// when still sitting in that phase past `PERMISSION_PROMPT_DWELL_MS`.
    ///
    /// Only `phase_changed` replaces this. A tool or permission event
    /// arriving mid-prompt does not clear it — the session is still sitting
    /// in the phase, which is precisely what the dwell rule measures.
    last_phase: HashMap<String, (String, i64)>,
    /// `session_relationship` from the most recent `turn_started`, so tool
    /// events in the same turn can set `sidechain` correctly.
    relationship: HashMap<String, String>,
    /// Paths extracted from `chat_history.jsonl`, keyed by
    /// `(session_id, tool_call_id)`. Filled lazily on `tool_completed` and
    /// never from `tool_started` (which has no id to join on).
    ///
    /// Values are already workspace-relative. An empty vec means "looked,
    /// found nothing" — so a missing `chat_history` does not re-read forever,
    /// and paths outside the workspace stay dropped rather than re-tried.
    path_cache: HashMap<(String, String), Vec<String>>,
    pub stats: ParseStats,
}

impl Grok {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Decode a percent-encoded path component. Only `%XX` hex escapes are
/// handled — enough for Grok's encoded cwd, without a URL-encoding crate.
///
/// Invalid escapes (truncated, non-hex) are left as literal text so a
/// corrupted directory name degrades to a non-match rather than an error.
pub(crate) fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Normalize a Grok event `ts` to epoch milliseconds.
///
/// Grok writes a bare number, not the RFC 3339 string Claude Code uses.
/// Magnitude decides the unit: microseconds (≥ 1e14), milliseconds (≥ 1e11),
/// or seconds. The thresholds sit between the three scales for Unix time
/// around the 2020s, so a clock a few decades off still classifies correctly.
pub(crate) fn normalize_ts(ts: f64) -> i64 {
    if !ts.is_finite() || ts < 0.0 {
        return 0;
    }
    if ts >= 100_000_000_000_000.0 {
        // Microseconds → milliseconds.
        (ts / 1_000.0) as i64
    } else if ts >= 100_000_000_000.0 {
        // Already milliseconds.
        ts as i64
    } else {
        // Seconds → milliseconds.
        (ts * 1_000.0) as i64
    }
}

fn ts_of(value: &Value) -> i64 {
    value
        .get("ts")
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_i64().map(|i| i as f64))
                .or_else(|| v.as_u64().map(|u| u as f64))
        })
        .map(normalize_ts)
        .unwrap_or(0)
}

/// Does `dir` look like a Grok root?
///
/// Grok's registry is directories under `sessions/`
/// (`sessions/<percent-encoded-cwd>/<session-id>/`); Claude's is files
/// (`sessions/<pid>.json`). A bare `sessions/` check claims Claude's root
/// too, so reject any root whose sessions dir holds `*.json` files.
fn looks_like_root(dir: &Path) -> bool {
    let sessions = dir.join("sessions");
    sessions.is_dir() && !has_json_children(&sessions)
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Map a Grok phase string onto activity. `permission_prompt` is handled by
/// the dwell rule, not here — entering it is not yet Blocked.
fn activity_for_phase(phase: &str) -> Option<AgentActivity> {
    match phase {
        "waiting_for_model" => Some(AgentActivity::Working {
            detail: Some("waiting".into()),
        }),
        "streaming_reasoning" => Some(AgentActivity::Working {
            detail: Some("thinking".into()),
        }),
        "streaming_text" => Some(AgentActivity::Working {
            detail: Some("streaming".into()),
        }),
        "tool_execution" => Some(AgentActivity::Working {
            detail: Some("tool".into()),
        }),
        // permission_prompt: dwell rule decides Working vs Blocked.
        "permission_prompt" => None,
        _ => None,
    }
}

/// Is `schema_version` a major this code knows? Only `1.x` is supported.
fn schema_supported(version: &str) -> bool {
    let major = version.split('.').next().unwrap_or(version);
    major == KNOWN_SCHEMA_MAJOR
}

impl AgentProvider for Grok {
    fn kind(&self) -> AgentKind {
        AgentKind::Grok
    }

    fn detect_roots(&self) -> Vec<PathBuf> {
        // No well-known env override (unlike CLAUDE_CONFIG_DIR); only the
        // conventional home directory is detected. The agent_roots setting
        // covers anything else.
        let mut roots = Vec::new();
        if let Some(home) = crate::paths::home_dir() {
            let candidate = home.join(".grok");
            if looks_like_root(&candidate) {
                roots.push(candidate);
            }
        }
        roots
    }

    fn claims_root(&self, dir: &Path) -> bool {
        looks_like_root(dir)
    }

    fn discover(&self, workspace: &Path, roots: &[PathBuf]) -> Vec<SessionRef> {
        let now = now_millis();
        let mut sessions = Vec::new();
        for root in roots {
            // One read per root, not per session. Missing or malformed is
            // an empty set — recency still applies, and discover never
            // errors.
            let live = live_session_ids(root);
            let sessions_dir = root.join("sessions");
            let Ok(cwd_entries) = std::fs::read_dir(&sessions_dir) else {
                continue;
            };
            for cwd_entry in cwd_entries.filter_map(|e| e.ok()) {
                let cwd_path = cwd_entry.path();
                if !cwd_path.is_dir() {
                    continue;
                }
                let encoded = cwd_entry.file_name().to_string_lossy().into_owned();
                let decoded = percent_decode(&encoded);
                if !is_within_workspace(&decoded, workspace) {
                    continue;
                }
                let Ok(session_entries) = std::fs::read_dir(&cwd_path) else {
                    continue;
                };
                for session_entry in session_entries.filter_map(|e| e.ok()) {
                    let session_path = session_entry.path();
                    if !session_path.is_dir() {
                        continue;
                    }
                    let events_file = session_path.join("events.jsonl");
                    if !events_file.is_file() {
                        continue;
                    }
                    let id = session_entry.file_name().to_string_lossy().into_owned();
                    let last_activity = file_last_activity(&events_file);
                    // Registry first (a listed live pid is Idle), then
                    // recency, then Stale. The registry carries no phase
                    // state, so listed never means Working.
                    let activity = if live.contains(&id)
                        || now.saturating_sub(last_activity) <= FRESH_WINDOW_MS
                    {
                        AgentActivity::Idle
                    } else {
                        AgentActivity::Stale
                    };
                    // Stale sessions are never rendered (selectLiveSessions
                    // filters them), so reading summary.json is pure I/O on
                    // a one-second discover tick — skip it for the dead ones.
                    let title = if activity == AgentActivity::Stale {
                        None
                    } else {
                        summary_title(&session_path)
                    };
                    sessions.push(SessionRef {
                        id,
                        agent: AgentKind::Grok,
                        title,
                        last_activity,
                        activity,
                    });
                }
            }
        }
        sessions.sort_by_key(|session| -session.last_activity);
        sessions
    }

    fn poll(
        &mut self,
        workspace: &Path,
        session: &SessionRef,
        roots: &[PathBuf],
    ) -> Vec<AgentEvent> {
        if self.unsupported.contains(&session.id) {
            return Vec::new();
        }

        let Some(path) = self.events_path(session, roots) else {
            return self.maybe_dwell(session);
        };
        let Ok(mut file) = File::open(&path) else {
            return self.maybe_dwell(session);
        };
        let Ok(len) = file.metadata().map(|m| m.len()) else {
            return self.maybe_dwell(session);
        };

        // First sight starts at the end — same "watching since" rule as
        // Claude Code and the phase-1 watcher.
        let offset = *self.offsets.entry(path.clone()).or_insert(len);
        let offset = if offset > len { 0 } else { offset };

        let mut events = Vec::new();

        if offset < len {
            if file.seek(SeekFrom::Start(offset)).is_err() {
                return self.maybe_dwell(session);
            }
            let mut bytes = Vec::new();
            if file.read_to_end(&mut bytes).is_err() {
                return self.maybe_dwell(session);
            }

            // Only complete lines. A partial trailing record stays for the
            // next poll, exactly as the Claude Code tailer does.
            let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') else {
                return self.maybe_dwell(session);
            };
            let complete = &bytes[..=last_newline];
            self.offsets.insert(path, offset + complete.len() as u64);

            for line in String::from_utf8_lossy(complete).lines() {
                if line.trim().is_empty() {
                    continue;
                }
                self.stats.records += 1;
                match serde_json::from_str::<Value>(line) {
                    Ok(record) => {
                        if let Some(batch) = self.events_from(&record, session, workspace, roots) {
                            events.extend(batch);
                        } else if self.unsupported.contains(&session.id) {
                            // Unknown schema: drop everything for this
                            // session and report zero events, no error.
                            return Vec::new();
                        }
                    }
                    Err(_) => self.stats.skipped += 1,
                }
                if events.len() >= MAX_EVENTS_PER_POLL {
                    break;
                }
            }
        }

        // Dwell check runs even when no new lines arrived — Blocked is a
        // wall-clock condition, not an event.
        events.extend(self.maybe_dwell(session));
        events
    }
}

impl Grok {
    fn events_path(&self, session: &SessionRef, roots: &[PathBuf]) -> Option<PathBuf> {
        for root in roots {
            let sessions_dir = root.join("sessions");
            let Ok(cwd_entries) = std::fs::read_dir(&sessions_dir) else {
                continue;
            };
            for cwd_entry in cwd_entries.filter_map(|e| e.ok()) {
                let cwd_path = cwd_entry.path();
                if !cwd_path.is_dir() {
                    continue;
                }
                let candidate = cwd_path.join(&session.id).join("events.jsonl");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    /// Convert one event record. Returns `None` when the session's schema is
    /// unsupported (caller must abandon the batch). Empty vec = skip quietly.
    fn events_from(
        &mut self,
        record: &Value,
        session: &SessionRef,
        workspace: &Path,
        roots: &[PathBuf],
    ) -> Option<Vec<AgentEvent>> {
        let event_type = record.get("type").and_then(Value::as_str).unwrap_or("");
        let at = ts_of(record);
        let session_id = session.id.clone();

        let mut out = Vec::new();
        match event_type {
            "turn_started" => {
                if let Some(version) = record.get("schema_version").and_then(Value::as_str) {
                    if !schema_supported(version) {
                        self.unsupported.insert(session_id);
                        return None;
                    }
                }
                if let Some(rel) = record.get("session_relationship").and_then(Value::as_str) {
                    self.relationship
                        .insert(session.id.clone(), rel.to_string());
                }
                // Lifecycle is owned by the poller (discover set diff), not
                // by per-event guesses — a turn start is not a session start.
            }
            "tool_started" | "tool_completed" => {
                let tool = record
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let sidechain = self
                    .relationship
                    .get(&session.id)
                    .is_some_and(|r| r != "primary");
                // `tool_started` has no `tool_call_id`, so it cannot join to
                // chat_history. Only `tool_completed` can carry paths.
                let paths = if event_type == "tool_completed" {
                    record
                        .get("tool_call_id")
                        .and_then(Value::as_str)
                        .map(|id| self.paths_for(session, id, workspace, roots))
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                out.push(AgentEvent::ToolCall {
                    session_id: session_id.clone(),
                    agent: Some(AgentKind::Grok),
                    at,
                    tool,
                    summary: None,
                    paths,
                    sidechain,
                });
            }
            "phase_changed" => {
                let phase = record
                    .get("phase")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                self.last_phase
                    .insert(session.id.clone(), (phase.clone(), at));

                if phase == "permission_prompt" {
                    // Not Blocked yet — dwell rule decides on a later poll.
                    // Surface Working so the indicator is not stuck on Idle
                    // while the prompt is still resolving under yolo mode.
                    if let Some(event) = self.emit_activity(
                        &session_id,
                        at,
                        AgentActivity::Working {
                            detail: Some("permission".into()),
                        },
                    ) {
                        out.push(event);
                    }
                } else if let Some(activity) = activity_for_phase(&phase) {
                    if let Some(event) = self.emit_activity(&session_id, at, activity) {
                        out.push(event);
                    }
                } else {
                    self.stats.skipped += 1;
                }
            }
            "turn_ended" => {
                self.last_phase.remove(&session.id);
                if let Some(event) = self.emit_activity(&session_id, at, AgentActivity::Idle) {
                    out.push(event);
                }
                // A turn ending is not a session ending: the session is idle,
                // waiting for the next prompt, and must stay visible.
            }
            // Known but not mapped to protocol events — count as seen, skip.
            "permission_requested"
            | "permission_resolved"
            | "loop_started"
            | "first_token"
            | "mcp_server_starting" => {}
            // Unknown type: skip and tally, never error.
            _ => {
                self.stats.skipped += 1;
            }
        }
        Some(out)
    }

    fn emit_activity(
        &mut self,
        session_id: &str,
        at: i64,
        activity: AgentActivity,
    ) -> Option<AgentEvent> {
        if self.last_activity.get(session_id) == Some(&activity) {
            return None;
        }
        self.last_activity
            .insert(session_id.to_string(), activity.clone());
        Some(AgentEvent::ActivityChanged {
            session_id: session_id.to_string(),
            agent: Some(AgentKind::Grok),
            at,
            activity,
        })
    }

    /// If the session is still in `permission_prompt` past the dwell
    /// threshold, emit Blocked. Called every poll, including quiet ones.
    fn maybe_dwell(&mut self, session: &SessionRef) -> Vec<AgentEvent> {
        let Some((phase, since)) = self.last_phase.get(&session.id).cloned() else {
            return Vec::new();
        };
        if phase != "permission_prompt" {
            return Vec::new();
        }
        let now = now_millis();
        if now.saturating_sub(since) < PERMISSION_PROMPT_DWELL_MS {
            return Vec::new();
        }
        self.emit_activity(&session.id, now, AgentActivity::Blocked)
            .into_iter()
            .collect()
    }

    /// Workspace-relative paths for a `tool_call_id`, filled from
    /// `chat_history.jsonl` on first sight. Missing or unreadable history is
    /// empty paths, never an error — the app must look like phase 1 when the
    /// richer file is absent.
    fn paths_for(
        &mut self,
        session: &SessionRef,
        tool_call_id: &str,
        workspace: &Path,
        roots: &[PathBuf],
    ) -> Vec<String> {
        let key = (session.id.clone(), tool_call_id.to_string());
        if let Some(paths) = self.path_cache.get(&key) {
            return paths.clone();
        }
        self.refresh_path_cache(session, workspace, roots);
        // A full scan that still found nothing for this id is a permanent
        // miss for this workspace open: insert empty so the next poll does
        // not re-read the tail for every tool without arguments.
        self.path_cache.entry(key).or_default().clone()
    }

    /// Re-scan the session's `chat_history.jsonl` tail and fold every
    /// `tool_calls[].id` → paths into the cache. Only paths are kept; the
    /// rest of each record (prompts, edit bodies) is discarded as soon as
    /// the line is parsed.
    fn refresh_path_cache(&mut self, session: &SessionRef, workspace: &Path, roots: &[PathBuf]) {
        let Some(events_path) = self.events_path(session, roots) else {
            return;
        };
        let Some(session_dir) = events_path.parent() else {
            return;
        };
        let chat_path = session_dir.join("chat_history.jsonl");
        let Some(text) = read_tail(&chat_path) else {
            return;
        };
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(record) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if record.get("type").and_then(Value::as_str) != Some("assistant") {
                continue;
            }
            let Some(calls) = record.get("tool_calls").and_then(Value::as_array) else {
                continue;
            };
            for call in calls {
                let Some(id) = call.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let paths = call
                    .get("arguments")
                    .map(|args| paths_from_arguments(args, workspace))
                    .unwrap_or_default();
                self.path_cache
                    .insert((session.id.clone(), id.to_string()), paths);
            }
        }
    }
}

/// Path-like tool arguments, converted to workspace-relative. Outside the
/// workspace is dropped: the protocol only carries relative paths, and a
/// file the agent touched elsewhere has nothing in the tree to attribute to.
///
/// `arguments` is either a JSON object or a JSON *string that needs a second
/// parse* — both shapes appear in the wild, so both are accepted.
fn paths_from_arguments(arguments: &Value, workspace: &Path) -> Vec<String> {
    let parsed;
    let object = match arguments {
        Value::Object(_) => arguments,
        Value::String(raw) => match serde_json::from_str::<Value>(raw) {
            Ok(value) => {
                parsed = value;
                &parsed
            }
            Err(_) => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    PATH_INPUT_KEYS
        .iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_str))
        .filter_map(|raw| to_workspace_relative(workspace, Path::new(raw)))
        .filter(|relative| !relative.is_empty())
        .collect()
}

/// Session ids listed in `<root>/active_sessions.json` whose process still
/// looks alive. Missing, empty, or malformed file → empty set, never error.
///
/// Verified shape (one running Grok session, 2026-08-12):
/// `[{ "cwd", "opened_at", "pid", "session_id" }]`.
/// `session_id` is the directory name `discover` already uses. A listed
/// `pid` that is gone is dropped so a stale registry cannot keep a dead
/// session live forever.
fn live_session_ids(root: &Path) -> HashSet<String> {
    let text = match std::fs::read_to_string(root.join("active_sessions.json")) {
        Ok(text) => text,
        Err(_) => return HashSet::new(),
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => return HashSet::new(),
    };
    let Some(entries) = value.as_array() else {
        return HashSet::new();
    };
    let mut live = HashSet::new();
    for entry in entries {
        let Some(id) = entry.get("session_id").and_then(Value::as_str) else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        if let Some(pid) = entry.get("pid").and_then(Value::as_u64) {
            if pid == 0 || pid > u64::from(u32::MAX) || !pid_is_alive(pid as u32) {
                continue;
            }
        }
        live.insert(id.to_string());
    }
    live
}

/// Is `pid` a running process? Used only to reject a stale registry entry.
/// When we cannot tell (non-Linux, non-Windows), trust the listing.
fn pid_is_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        PathBuf::from(format!("/proc/{pid}")).is_dir()
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        const SYNCHRONIZE: u32 = 0x0010_0000;
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            return false;
        }
        let alive = unsafe { WaitForSingleObject(handle, 0) } == WAIT_TIMEOUT;
        unsafe {
            CloseHandle(handle);
        }
        alive
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = pid;
        true
    }
}

/// Best-effort last-activity time from the events file: prefer the last
/// parseable `ts`, fall back to mtime.
///
/// Reads only the tail. `events.jsonl` grows for as long as the session runs
/// — one observed session held 854 phase events — and discovery runs on every
/// refresh, so reading the whole file to find its last line would make
/// discovery cost scale with session length.
fn file_last_activity(path: &Path) -> i64 {
    if let Some(text) = read_tail(path) {
        for line in text.lines().rev() {
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                let at = ts_of(&value);
                if at > 0 {
                    return at;
                }
            }
        }
    }
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Generated session title from `summary.json`, when present and well-formed.
///
/// This file is not a stable API — missing, empty, or malformed means
/// `None`, never an error. Only `session_summary` is read; no prompts or
/// message content. Same category as Claude Code's `ai-title`.
fn summary_title(session_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(session_dir.join("summary.json")).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let summary = value.get("session_summary")?.as_str()?.trim();
    if summary.is_empty() {
        None
    } else {
        Some(summary.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVENTS: &str = include_str!("fixtures/grok-events.jsonl");

    fn workspace() -> &'static Path {
        Path::new("/home/dev/project")
    }

    fn session(id: &str) -> SessionRef {
        SessionRef {
            id: id.to_string(),
            agent: AgentKind::Grok,
            title: None,
            last_activity: 0,
            activity: AgentActivity::Idle,
        }
    }

    /// `<root>/sessions/<encoded-cwd>/<id>/events.jsonl`.
    fn session_tree(
        contents: &str,
        id: &str,
        cwd: &str,
    ) -> (tempfile::TempDir, Vec<PathBuf>, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let encoded = cwd
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect::<String>();
        let dir = root.path().join("sessions").join(&encoded).join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("events.jsonl");
        std::fs::write(&file, contents).unwrap();
        let roots = vec![root.path().to_path_buf()];
        (root, roots, file)
    }

    // -- percent-decode -----------------------------------------------------

    #[test]
    fn percent_decode_reverses_encoded_cwd() {
        assert_eq!(
            percent_decode("%2Fhome%2Fdev%2Fproject"),
            "/home/dev/project"
        );
    }

    #[test]
    fn percent_decode_preserves_literal_hyphen_and_space_escape() {
        // A path component with a literal `-` must survive (not an escape).
        assert_eq!(percent_decode("%2Fhome%2Fdev%2Fmy-app"), "/home/dev/my-app");
        // Space as %20.
        assert_eq!(
            percent_decode("%2Fhome%2Fdev%2Fmy%20app"),
            "/home/dev/my app"
        );
    }

    // -- workspace match ----------------------------------------------------

    #[test]
    fn workspace_match_accepts_subdirectory_rejects_sibling() {
        assert!(is_within_workspace("/home/dev/project", workspace()));
        assert!(is_within_workspace("/home/dev/project/src", workspace()));
        assert!(!is_within_workspace("/home/dev/other", workspace()));
        assert!(!is_within_workspace("/home/dev", workspace()));
    }

    // -- ts normalization ---------------------------------------------------

    #[test]
    fn ts_normalization_to_epoch_millis() {
        // Seconds around 2026.
        assert_eq!(normalize_ts(1_786_121_838.0), 1_786_121_838_000);
        // Milliseconds (the unit Grok actually writes).
        assert_eq!(normalize_ts(1_786_121_838_290.0), 1_786_121_838_290);
        // Microseconds.
        assert_eq!(normalize_ts(1_786_121_838_290_000.0), 1_786_121_838_290);
    }

    // -- discovery ----------------------------------------------------------

    #[test]
    fn discovers_sessions_under_matching_encoded_cwd() {
        let id = "sess-discover-1";
        let (_dir, roots, _) = session_tree(EVENTS, id, "/home/dev/project");
        // Sibling workspace must not be claimed.
        let other = roots[0]
            .join("sessions")
            .join("%2Fhome%2Fdev%2Fother")
            .join("sess-other");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("events.jsonl"), EVENTS).unwrap();

        let found = Grok::new().discover(workspace(), &roots);
        let ids: Vec<&str> = found.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec![id]);
    }

    #[test]
    fn claims_a_root_with_sessions_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!Grok::new().claims_root(dir.path()));
        std::fs::create_dir_all(dir.path().join("sessions")).unwrap();
        assert!(Grok::new().claims_root(dir.path()));
    }

    #[test]
    fn discovery_treats_a_recent_session_as_idle_and_an_old_one_as_stale() {
        // Recency is the fallback when the registry is missing. Without it
        // every session ever run in a workspace stays listed as live.
        let recent = format!(
            r#"{{"type":"phase_changed","ts":{},"phase":"streaming_text"}}"#,
            now_millis()
        );
        let (_fresh_dir, fresh_roots, _) = session_tree(&recent, "sess-fresh", "/home/dev/project");
        let found = Grok::new().discover(workspace(), &fresh_roots);
        assert_eq!(found[0].activity, AgentActivity::Idle, "{found:?}");

        // Same shape, last event in 1970.
        let (_old_dir, old_roots, _) = session_tree(
            r#"{"type":"phase_changed","ts":1,"phase":"streaming_text"}"#,
            "sess-old",
            "/home/dev/project",
        );
        let found = Grok::new().discover(workspace(), &old_roots);
        assert_eq!(found[0].activity, AgentActivity::Stale, "{found:?}");
    }

    /// Write `<root>/active_sessions.json` with the verified 2026-08-12 shape.
    /// Paths and ids are invented — never real session content.
    fn write_active_sessions(root: &Path, entries: &[(&str, u32)]) {
        let body = entries
            .iter()
            .map(|(id, pid)| {
                format!(
                    r#"{{"cwd":"/home/dev/project","opened_at":"2026-08-12T16:57:00.123Z","pid":{pid},"session_id":"{id}"}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        std::fs::write(root.join("active_sessions.json"), format!("[{body}]")).unwrap();
    }

    #[test]
    fn discovery_treats_a_registry_session_as_live_even_when_events_are_old() {
        // The registry is why an idle session older than the freshness
        // window stays listed: recency alone would mark it Stale.
        let (_dir, roots, _) = session_tree(
            r#"{"type":"phase_changed","ts":1,"phase":"streaming_text"}"#,
            "sess-registry",
            "/home/dev/project",
        );
        write_active_sessions(&roots[0], &[("sess-registry", std::process::id())]);
        let found = Grok::new().discover(workspace(), &roots);
        assert_eq!(found[0].id, "sess-registry");
        assert_eq!(found[0].activity, AgentActivity::Idle, "{found:?}");
    }

    #[test]
    fn discovery_treats_a_dead_registry_pid_as_stale_when_events_are_old() {
        // Prefer under-claiming: a leftover listing must not keep a dead
        // session in the header.
        let (_dir, roots, _) = session_tree(
            r#"{"type":"phase_changed","ts":1,"phase":"streaming_text"}"#,
            "sess-dead-pid",
            "/home/dev/project",
        );
        write_active_sessions(&roots[0], &[("sess-dead-pid", 4_294_967_294)]);
        let found = Grok::new().discover(workspace(), &roots);
        assert_eq!(found[0].activity, AgentActivity::Stale, "{found:?}");
    }

    #[test]
    fn missing_or_malformed_registry_degrades_to_recency_and_never_errors() {
        let recent = format!(
            r#"{{"type":"phase_changed","ts":{},"phase":"streaming_text"}}"#,
            now_millis()
        );

        // No file: same as today's recency-only behaviour.
        let (_missing, missing_roots, _) =
            session_tree(&recent, "sess-no-reg", "/home/dev/project");
        let found = Grok::new().discover(workspace(), &missing_roots);
        assert_eq!(found[0].activity, AgentActivity::Idle, "{found:?}");

        // Truncated JSON: ignore the file, do not panic.
        let (_bad, bad_roots, _) = session_tree(&recent, "sess-bad-reg", "/home/dev/project");
        std::fs::write(bad_roots[0].join("active_sessions.json"), "{not json").unwrap();
        let found = Grok::new().discover(workspace(), &bad_roots);
        assert_eq!(found[0].activity, AgentActivity::Idle, "{found:?}");

        // Object instead of array: same degradation.
        let (_obj, obj_roots, _) = session_tree(&recent, "sess-obj-reg", "/home/dev/project");
        std::fs::write(
            obj_roots[0].join("active_sessions.json"),
            r#"{"session_id":"sess-obj-reg"}"#,
        )
        .unwrap();
        let found = Grok::new().discover(workspace(), &obj_roots);
        assert_eq!(found[0].activity, AgentActivity::Idle, "{found:?}");

        // Empty array + old events: Stale (registry names no one).
        let (_empty, empty_roots, _) = session_tree(
            r#"{"type":"phase_changed","ts":1,"phase":"streaming_text"}"#,
            "sess-empty-reg",
            "/home/dev/project",
        );
        std::fs::write(empty_roots[0].join("active_sessions.json"), "[]").unwrap();
        let found = Grok::new().discover(workspace(), &empty_roots);
        assert_eq!(found[0].activity, AgentActivity::Stale, "{found:?}");
    }

    #[test]
    fn session_title_comes_from_summary_json_and_tolerates_its_absence() {
        // Fresh event so the session is Idle; titles are only read for
        // non-stale sessions (see discover).
        let recent = format!(
            r#"{{"type":"phase_changed","ts":{},"phase":"streaming_text"}}"#,
            now_millis()
        );
        let (dir, roots, events) = session_tree(&recent, "sess-title", "/home/dev/project");
        // No summary.json beside the events file → untitled, never an error.
        assert_eq!(Grok::new().discover(workspace(), &roots)[0].title, None);

        let session_dir = events.parent().unwrap();
        std::fs::write(
            session_dir.join("summary.json"),
            r#"{"session_summary":"  Fix the pin button  ","info":{"id":"sess-title"}}"#,
        )
        .unwrap();
        assert_eq!(
            Grok::new().discover(workspace(), &roots)[0]
                .title
                .as_deref(),
            Some("Fix the pin button"),
        );

        // Malformed is the same as absent — this file is not a stable API.
        std::fs::write(session_dir.join("summary.json"), "{not json").unwrap();
        assert_eq!(Grok::new().discover(workspace(), &roots)[0].title, None);
        drop(dir);
    }

    #[test]
    fn stale_session_is_discovered_without_reading_its_title() {
        // Last event in 1970 → outside the freshness window. A well-formed
        // summary.json must still yield title: None — discover is on a
        // one-second tick and stale sessions are never rendered.
        let (_dir, roots, events) = session_tree(
            r#"{"type":"phase_changed","ts":1,"phase":"streaming_text"}"#,
            "sess-stale-title",
            "/home/dev/project",
        );
        let session_dir = events.parent().unwrap();
        std::fs::write(
            session_dir.join("summary.json"),
            r#"{"session_summary":"Should not be read","info":{"id":"sess-stale-title"}}"#,
        )
        .unwrap();
        let found = Grok::new().discover(workspace(), &roots);
        assert_eq!(found[0].activity, AgentActivity::Stale, "{found:?}");
        assert_eq!(found[0].title, None, "{found:?}");
    }

    // -- tailing ------------------------------------------------------------

    #[test]
    fn offset_tailer_returns_only_new_complete_lines() {
        let id = "sess-tail-1";
        let (_dir, roots, file) = session_tree("", id, "/home/dev/project");
        let mut provider = Grok::new();

        // First poll seeds the offset at EOF.
        assert!(provider.poll(workspace(), &session(id), &roots).is_empty());

        // Partial line — must not parse.
        std::fs::write(&file, r#"{"type":"turn_started","ts":1786121838290,"#).unwrap();
        assert!(
            provider.poll(workspace(), &session(id), &roots).is_empty(),
            "partial trailing line must wait for its newline"
        );

        // Complete the line plus one more; both arrive once, never re-emitted.
        let line1 = concat!(
            r#"{"type":"turn_started","ts":1786121838290,"session_id":"s","#,
            r#""model_id":"grok-4.5","turn_number":1,"schema_version":"1.0","#,
            r#""session_relationship":"primary","yolo_mode":true,"#,
            r#""conversation_message_count":3}"#,
        );
        let line2 = r#"{"type":"phase_changed","ts":1786121838300,"phase":"streaming_reasoning"}"#;
        std::fs::write(&file, format!("{line1}\n{line2}\n")).unwrap();
        let first = provider.poll(workspace(), &session(id), &roots);
        assert!(
            first
                .iter()
                .all(|e| !matches!(e, AgentEvent::SessionStarted { .. })),
            "turn_started must not emit SessionStarted (poller owns lifecycle): {first:?}"
        );
        assert!(
            first.iter().any(|e| matches!(
                e,
                AgentEvent::ActivityChanged {
                    activity: AgentActivity::Working { detail: Some(d) },
                    ..
                } if d == "thinking"
            )),
            "streaming_reasoning → Working thinking: {first:?}"
        );

        // Second poll with no new data → nothing.
        assert!(provider.poll(workspace(), &session(id), &roots).is_empty());
    }

    #[test]
    fn malformed_and_unknown_lines_are_skipped_and_counted() {
        let id = "sess-hostile-1";
        let (_dir, roots, file) = session_tree("", id, "/home/dev/project");
        let mut provider = Grok::new();
        provider.poll(workspace(), &session(id), &roots);

        // Fixture covers unknown type + malformed line + partial trailing.
        std::fs::write(&file, EVENTS).unwrap();
        let events = provider.poll(workspace(), &session(id), &roots);

        assert!(provider.stats.skipped >= 2, "unknown type + bad json");
        assert!(provider.stats.records >= 5);
        // Must not have errored: we got some real events out.
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolCall { .. })));
    }

    #[test]
    fn unknown_schema_version_yields_zero_events_and_no_error() {
        let id = "sess-schema-2";
        let body = r#"{"type":"turn_started","ts":1786121838290,"session_id":"s","model_id":"grok-9","turn_number":1,"schema_version":"2.0","session_relationship":"primary","yolo_mode":false,"conversation_message_count":0}
{"type":"phase_changed","ts":1786121838300,"phase":"streaming_text"}
"#;
        let (_dir, roots, file) = session_tree("", id, "/home/dev/project");
        let mut provider = Grok::new();
        provider.poll(workspace(), &session(id), &roots);
        std::fs::write(&file, body).unwrap();
        let events = provider.poll(workspace(), &session(id), &roots);
        assert!(
            events.is_empty(),
            "unknown major must not guess: {events:?}"
        );
        // Subsequent polls stay quiet.
        assert!(provider.poll(workspace(), &session(id), &roots).is_empty());
    }

    // -- permission_prompt dwell rule ---------------------------------------

    #[test]
    fn fast_permission_allow_never_produces_blocked() {
        // Under --always-approve every prompt resolves in ~0 ms. Entering
        // permission_prompt and leaving it in the same batch must never
        // surface Blocked.
        let id = "sess-perm-fast";
        let body = r#"{"type":"turn_started","ts":1786121838290,"session_id":"s","model_id":"grok-4.5","turn_number":1,"schema_version":"1.0","session_relationship":"primary","yolo_mode":true,"conversation_message_count":1}
{"type":"phase_changed","ts":1786121838400,"phase":"permission_prompt"}
{"type":"permission_requested","ts":1786121838401,"tool_name":"search_replace"}
{"type":"permission_resolved","ts":1786121838401,"tool_name":"search_replace","decision":"allow","wait_ms":0}
{"type":"phase_changed","ts":1786121838402,"phase":"tool_execution"}
{"type":"tool_started","ts":1786121838403,"tool_name":"search_replace"}
{"type":"tool_completed","ts":1786121838500,"tool_name":"search_replace","tool_call_id":"c1","duration_ms":97,"outcome":"success"}
"#;
        let (_dir, roots, file) = session_tree("", id, "/home/dev/project");
        let mut provider = Grok::new();
        provider.poll(workspace(), &session(id), &roots);
        std::fs::write(&file, body).unwrap();
        let events = provider.poll(workspace(), &session(id), &roots);
        assert!(
            events.iter().all(|e| !matches!(
                e,
                AgentEvent::ActivityChanged {
                    activity: AgentActivity::Blocked,
                    ..
                }
            )),
            "fast allow must not be Blocked: {events:?}"
        );
    }

    #[test]
    fn dwelling_permission_prompt_becomes_blocked() {
        // A prompt with no follow-up past the threshold is the real blocked
        // case: someone has to answer.
        let id = "sess-perm-dwell";
        // Timestamp far enough in the past that now - ts > dwell threshold.
        let old_ts = now_millis() - PERMISSION_PROMPT_DWELL_MS - 500;
        let body = format!(
            r#"{{"type":"turn_started","ts":{old_ts},"session_id":"s","model_id":"grok-4.5","turn_number":1,"schema_version":"1.0","session_relationship":"primary","yolo_mode":false,"conversation_message_count":1}}
{{"type":"phase_changed","ts":{old_ts},"phase":"permission_prompt"}}
"#
        );
        let (_dir, roots, file) = session_tree("", id, "/home/dev/project");
        let mut provider = Grok::new();
        provider.poll(workspace(), &session(id), &roots);
        std::fs::write(&file, body).unwrap();
        let events = provider.poll(workspace(), &session(id), &roots);
        assert!(
            events.iter().any(|e| matches!(
                e,
                AgentEvent::ActivityChanged {
                    activity: AgentActivity::Blocked,
                    ..
                }
            )),
            "dwelling prompt must be Blocked: {events:?}"
        );
    }

    #[test]
    fn normal_turn_emits_session_tools_and_idle() {
        let id = "sess-normal";
        let (_dir, roots, file) = session_tree("", id, "/home/dev/project");
        let mut provider = Grok::new();
        provider.poll(workspace(), &session(id), &roots);
        std::fs::write(&file, EVENTS).unwrap();
        let events = provider.poll(workspace(), &session(id), &roots);

        assert!(
            events
                .iter()
                .all(|e| !matches!(e, AgentEvent::SessionStarted { .. })),
            "turn_started must not emit SessionStarted: {events:?}"
        );
        // No chat_history beside the fixture → empty paths, never an error.
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolCall { tool, paths, .. }
                if tool == "search_replace" && paths.is_empty()
        )));
        assert!(
            events.iter().any(|e| matches!(
                e,
                AgentEvent::ActivityChanged {
                    activity: AgentActivity::Idle,
                    ..
                }
            )),
            "turn_ended → ActivityChanged Idle: {events:?}"
        );
        assert!(
            events
                .iter()
                .all(|e| !matches!(e, AgentEvent::SessionEnded { .. })),
            "turn_ended must not emit SessionEnded (session stays visible): {events:?}"
        );
    }

    // -- chat_history path extraction ---------------------------------------

    /// Write a synthetic `chat_history.jsonl` next to the events file. Paths
    /// and prompts are invented — never real session content.
    fn write_chat_history(events_file: &Path, body: &str) {
        let chat = events_file
            .parent()
            .expect("events live in a session dir")
            .join("chat_history.jsonl");
        std::fs::write(chat, body).unwrap();
    }

    fn tool_completed_line(tool: &str, id: &str, ts: i64) -> String {
        format!(
            r#"{{"type":"tool_completed","ts":{ts},"tool_name":"{tool}","tool_call_id":"{id}","duration_ms":10,"outcome":"success"}}"#
        )
    }

    #[test]
    fn tool_completed_joins_search_replace_file_path_from_chat_history() {
        let id = "sess-paths-sr";
        let (_dir, roots, file) = session_tree("", id, "/home/dev/project");
        // arguments as a JSON object (already parsed shape).
        write_chat_history(
            &file,
            concat!(
                r#"{"type":"assistant","tool_calls":[{"id":"tc-sr","name":"search_replace","#,
                r#""arguments":{"file_path":"/home/dev/project/src/main.rs","#,
                r#""old_string":"a","new_string":"b"}}]}"#,
                "\n",
            ),
        );
        let mut provider = Grok::new();
        provider.poll(workspace(), &session(id), &roots);
        std::fs::write(
            &file,
            format!(
                "{}\n",
                tool_completed_line("search_replace", "tc-sr", 1_786_121_838_000)
            ),
        )
        .unwrap();
        let events = provider.poll(workspace(), &session(id), &roots);
        let paths = events.iter().find_map(|e| match e {
            AgentEvent::ToolCall { tool, paths, .. } if tool == "search_replace" => {
                Some(paths.clone())
            }
            _ => None,
        });
        assert_eq!(paths, Some(vec!["src/main.rs".to_string()]));
    }

    #[test]
    fn target_file_and_path_argument_keys_are_extracted() {
        let id = "sess-paths-keys";
        let (_dir, roots, file) = session_tree("", id, "/home/dev/project");
        write_chat_history(
            &file,
            concat!(
                r#"{"type":"assistant","tool_calls":[{"id":"tc-read","name":"read_file","#,
                r#""arguments":{"target_file":"/home/dev/project/lib/mod.rs"}},"#,
                r#"{"id":"tc-grep","name":"grep","arguments":{"path":"/home/dev/project/src","#,
                r#""pattern":"fn main"}}]}"#,
                "\n",
            ),
        );
        let mut provider = Grok::new();
        provider.poll(workspace(), &session(id), &roots);
        let body = format!(
            "{}\n{}\n",
            tool_completed_line("read_file", "tc-read", 1_786_121_838_000),
            tool_completed_line("grep", "tc-grep", 1_786_121_838_100),
        );
        std::fs::write(&file, body).unwrap();
        let events = provider.poll(workspace(), &session(id), &roots);

        let read_paths = events.iter().find_map(|e| match e {
            AgentEvent::ToolCall { tool, paths, .. } if tool == "read_file" => Some(paths.clone()),
            _ => None,
        });
        let grep_paths = events.iter().find_map(|e| match e {
            AgentEvent::ToolCall { tool, paths, .. } if tool == "grep" => Some(paths.clone()),
            _ => None,
        });
        assert_eq!(read_paths, Some(vec!["lib/mod.rs".to_string()]));
        assert_eq!(grep_paths, Some(vec!["src".to_string()]));
    }

    #[test]
    fn path_outside_the_workspace_is_dropped() {
        let id = "sess-paths-out";
        let (_dir, roots, file) = session_tree("", id, "/home/dev/project");
        write_chat_history(
            &file,
            concat!(
                r#"{"type":"assistant","tool_calls":[{"id":"tc-out","name":"search_replace","#,
                r#""arguments":{"file_path":"/home/dev/other/secret.rs","#,
                r#""old_string":"x","new_string":"y"}}]}"#,
                "\n",
            ),
        );
        let mut provider = Grok::new();
        provider.poll(workspace(), &session(id), &roots);
        std::fs::write(
            &file,
            format!(
                "{}\n",
                tool_completed_line("search_replace", "tc-out", 1_786_121_838_000)
            ),
        )
        .unwrap();
        let events = provider.poll(workspace(), &session(id), &roots);
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolCall { paths, .. } if paths.is_empty()
        )));
    }

    #[test]
    fn arguments_given_as_a_json_string_parse_the_same_as_an_object() {
        let id = "sess-paths-str";
        let (_dir, roots, file) = session_tree("", id, "/home/dev/project");
        // Double-encoded: arguments is a string containing JSON.
        write_chat_history(
            &file,
            concat!(
                r#"{"type":"assistant","tool_calls":[{"id":"tc-str","name":"search_replace","#,
                r#""arguments":"{\"file_path\":\"/home/dev/project/src/main.rs\","#,
                r#"\"old_string\":\"a\",\"new_string\":\"b\"}"}]}"#,
                "\n",
            ),
        );
        let mut provider = Grok::new();
        provider.poll(workspace(), &session(id), &roots);
        std::fs::write(
            &file,
            format!(
                "{}\n",
                tool_completed_line("search_replace", "tc-str", 1_786_121_838_000)
            ),
        )
        .unwrap();
        let events = provider.poll(workspace(), &session(id), &roots);
        let paths = events.iter().find_map(|e| match e {
            AgentEvent::ToolCall { paths, .. } => Some(paths.clone()),
            _ => None,
        });
        assert_eq!(paths, Some(vec!["src/main.rs".to_string()]));
    }

    #[test]
    fn missing_chat_history_yields_empty_paths_and_no_error() {
        let id = "sess-paths-missing";
        let (_dir, roots, file) = session_tree("", id, "/home/dev/project");
        // Deliberately no chat_history.jsonl.
        let mut provider = Grok::new();
        provider.poll(workspace(), &session(id), &roots);
        std::fs::write(
            &file,
            format!(
                "{}\n",
                tool_completed_line("search_replace", "tc-miss", 1_786_121_838_000)
            ),
        )
        .unwrap();
        let events = provider.poll(workspace(), &session(id), &roots);
        assert!(
            events.iter().any(|e| matches!(
                e,
                AgentEvent::ToolCall { tool, paths, .. }
                    if tool == "search_replace" && paths.is_empty()
            )),
            "missing history must not error: {events:?}"
        );
    }
}
