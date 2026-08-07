//! Claude Code provider.
//!
//! Claude Code writes one JSONL transcript per session under
//! `<config-root>/projects/<slug>/<session-uuid>.jsonl`, appending a record
//! per turn while the session runs. Tailing that file is how AgentLens learns
//! which tool call touched which file.
//!
//! Three things about the layout are easy to get wrong, and all are
//! load-bearing (verified against Claude Code 2.1.220 / 2.1.224):
//!
//! 1. **There is rarely one config root.** `CLAUDE_CONFIG_DIR` selects a
//!    profile — a wholly separate config directory — and a machine commonly
//!    has several. A desktop app inherits that variable from nothing, so
//!    likely profiles are enumerated from disk instead. Their naming is a
//!    habit rather than a standard, which is why detection is best-effort and
//!    the `agent_roots` setting exists to name what it misses.
//! 2. **The slug is lossy.** It is the absolute workspace path with `/` turned
//!    into `-`, so `/a/b-c` and `/a/b/c` produce the same directory name. It
//!    is therefore only good enough to *find candidates*; the authority on
//!    which workspace a session belongs to is the `cwd` field inside the
//!    records themselves.
//! 3. **Live sessions have a registry.** `<config-root>/sessions/<pid>.json`
//!    is the primary path for *running* sessions: it carries `pid`,
//!    `procStart`, `sessionId`, `cwd`, and `status` (`busy` / `idle`) with
//!    heartbeats. Slug-based discovery stays as the fallback for sessions
//!    with no live process. The registry also carries
//!    `messagingSocketPath` — **do not connect to it**; AgentLens only reads
//!    files.
//!
//! The format is not a stable API, so every read here is defensive: unknown
//! record types are skipped, malformed lines are counted and ignored, and a
//! missing directory means "no agent detected" rather than an error.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::{epoch_millis, is_within_workspace, read_tail, AgentProvider, ParseStats};
use crate::paths::to_workspace_relative;
use crate::protocol::{AgentActivity, AgentEvent, AgentKind, SessionRef};

/// Cap on a single `poll`, so a session that was appended to heavily while the
/// app was busy can't produce an unbounded batch.
const MAX_EVENTS_PER_POLL: usize = 500;

/// Tool inputs that name a file. `Bash` deliberately isn't here — it names a
/// command, and what it touched is left to the correlation engine.
const PATH_INPUT_KEYS: [&str; 2] = ["file_path", "path"];

/// How long a registry heartbeat may lag before a non-Linux host treats the
/// session as dead. Linux uses `/proc/<pid>` instead; this is only the
/// documented fallback where that check is unavailable.
#[cfg(not(target_os = "linux"))]
const HEARTBEAT_STALE_MS: i64 = 5 * 60 * 1000;

#[derive(Default)]
pub struct ClaudeCode {
    /// Byte offset already consumed, per transcript.
    offsets: HashMap<PathBuf, u64>,
    /// Last activity state emitted for a session id. `ActivityChanged` only
    /// fires on a real transition — re-emitting the same state would thrash
    /// the header indicator for nothing.
    last_activity: HashMap<String, AgentActivity>,
    pub stats: ParseStats,
}

impl ClaudeCode {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether any read offset is being held. Only used to prove `reset`
    /// really cleared them — nothing else needs to know.
    #[cfg(test)]
    pub fn offsets_is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
}

/// Claude Code profiles this machine appears to have.
///
/// A profile is a whole separate config directory — own login, own settings,
/// own `projects/` history — selected by pointing `CLAUDE_CONFIG_DIR` at it.
/// Nothing is inherited between them.
///
/// **Reading the env var is not enough.** AgentLens is a desktop app: launched
/// from an icon it inherits no `CLAUDE_CONFIG_DIR` at all, and launched from a
/// terminal it inherits at most whichever single profile that shell was using.
/// Either way it would miss the profiles the work actually happens in. So
/// likely siblings are enumerated from disk, and the env var is treated as one
/// more root to include rather than as the answer.
///
/// **`~/.claude*` is a guess, not a rule.** Claude Code defines only
/// `CLAUDE_CONFIG_DIR`; it has no notion of profile naming and keeps no
/// registry of profiles, so there is nothing authoritative to enumerate.
/// The name is a widely-used habit, not a contract — anyone keeping profiles
/// somewhere else is served by the `agent_roots` setting, not by this.
fn detected_profiles() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut push = |root: PathBuf| {
        if looks_like_profile(&root) && !roots.contains(&root) {
            roots.push(root);
        }
    };

    // An explicit override may point somewhere that breaks the convention.
    if let Some(configured) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        push(PathBuf::from(configured));
    }

    if let Some(home) = home_dir() {
        push(home.join(".claude"));

        // `~/.claude*` siblings — `-work`, `_work`, `2` all get picked up.
        // Sorted for a stable ordering, since these are shown to the user.
        let mut profiles: Vec<PathBuf> = std::fs::read_dir(&home)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(".claude") && name != ".claude")
            })
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        profiles.sort();
        for profile in profiles {
            push(profile);
        }
    }
    roots
}

/// A directory with the shape of a Claude Code profile. `projects/` is the
/// load-bearing part: it keeps `~/.claude.json` (a file) and an unrelated
/// `.claude-*` directory from contributing phantom sessions.
fn looks_like_profile(dir: &Path) -> bool {
    dir.join("projects").is_dir()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// The project directory name Claude Code derives from a workspace path:
/// the absolute path with separators replaced by `-`.
///
/// Lossy by construction, and only ever used forwards. A wrong guess costs
/// nothing worse than falling through to the loose name match below.
pub fn slug_for(workspace: &Path) -> String {
    workspace
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '\\' { '-' } else { c })
        .collect()
}

/// Alphanumerics only, lowercased. Reduces a path and a project directory
/// name to a form that survives *any* separator-substitution scheme, so
/// `/home/h/git/App`, `-home-h-git-App` and `C--home-h-git-app` all collapse
/// together.
fn loose_key(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Transcript files that might belong to `workspace`.
///
/// The slug directory is the fast path. When a root doesn't have it, that is
/// nearly always because *that profile has no sessions for this workspace* —
/// with several profiles configured, most roots won't. It occasionally means
/// the slug scheme differs (an unverified platform), so the fallback is a
/// cheap loose name match rather than a scan.
///
/// It used to take every project directory in the root instead, which read the
/// tail of every transcript on the machine: measured at ~600 ms and 300+ files
/// per call here, repeated on every poll. Matching on the name first costs
/// microseconds and reads only directories that could plausibly be this
/// workspace; `cwd` still has the final say.
fn candidate_transcripts(workspace: &Path, roots: &[PathBuf]) -> Vec<PathBuf> {
    let slug = slug_for(workspace);
    let wanted = loose_key(&workspace.to_string_lossy());
    let mut candidates = Vec::new();

    for root in roots {
        let projects = root.join("projects");
        let exact = projects.join(&slug);
        let dirs: Vec<PathBuf> = if exact.is_dir() {
            vec![exact]
        } else {
            let Ok(entries) = std::fs::read_dir(&projects) else {
                continue;
            };
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| loose_key(name) == wanted)
                })
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        };

        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            candidates.extend(
                entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl")),
            );
        }
    }
    candidates
}

/// What a transcript's recent records say about it.
struct Metadata {
    /// Every distinct `cwd` seen in the window. Plural on purpose: `cwd` is
    /// where the agent was standing when it wrote that record, and a session
    /// that runs `cd` moves around inside the workspace during its life.
    cwds: Vec<String>,
    title: Option<String>,
    last_activity: i64,
}

fn read_metadata(path: &Path) -> Option<Metadata> {
    let tail = read_tail(path)?;
    let mut meta = Metadata {
        cwds: Vec::new(),
        title: None,
        last_activity: 0,
    };

    for line in tail.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(cwd) = record.get("cwd").and_then(Value::as_str) {
            if !meta.cwds.iter().any(|seen| seen == cwd) {
                meta.cwds.push(cwd.to_string());
            }
        }
        if let Some(title) = record.get("aiTitle").and_then(Value::as_str) {
            meta.title = Some(title.to_string());
        }
        if let Some(at) = record.get("timestamp").and_then(Value::as_str) {
            if let Some(millis) = epoch_millis(at) {
                meta.last_activity = meta.last_activity.max(millis);
            }
        }
    }
    Some(meta)
}

/// Every `tool_use` block in one assistant record, as events.
fn tool_calls_from(record: &Value, workspace: &Path, session_id: &str, at: i64) -> Vec<AgentEvent> {
    let sidechain = record
        .get("isSidechain")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    record
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
                .filter_map(|block| {
                    let tool = block.get("name").and_then(Value::as_str)?;
                    let input = block.get("input");
                    Some(AgentEvent::ToolCall {
                        session_id: session_id.to_string(),
                        at,
                        tool: tool.to_string(),
                        // `Bash` carries a human-written description; the file
                        // tools don't need one, their path says it.
                        summary: input
                            .and_then(|input| input.get("description"))
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        paths: input
                            .map(|i| workspace_paths(i, workspace))
                            .unwrap_or_default(),
                        sidechain,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Path-like tool inputs, converted to workspace-relative. Anything outside
/// the workspace is dropped: the protocol only carries relative paths, and a
/// file the agent touched elsewhere has nothing in the tree to attribute to.
fn workspace_paths(input: &Value, workspace: &Path) -> Vec<String> {
    PATH_INPUT_KEYS
        .iter()
        .filter_map(|key| input.get(key).and_then(Value::as_str))
        .filter_map(|raw| to_workspace_relative(workspace, Path::new(raw)))
        .filter(|relative| !relative.is_empty())
        .collect()
}

/// One transcript record to zero or more events. Unknown types return nothing
/// rather than erroring — new record types appear without warning.
fn events_from(record: &Value, workspace: &Path) -> Vec<AgentEvent> {
    let session_id = record
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    match record.get("type").and_then(Value::as_str) {
        Some("assistant") => {
            let Some(at) = record
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(epoch_millis)
            else {
                return Vec::new();
            };
            tool_calls_from(record, workspace, &session_id, at)
        }
        // The user's actual instruction — the "why" behind the surrounding
        // tool calls, and a better intent line than truncated assistant prose.
        Some("last-prompt") => record
            .get("lastPrompt")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(|text| {
                vec![AgentEvent::AssistantNote {
                    session_id,
                    // These records carry no timestamp of their own.
                    at: 0,
                    text: text.to_string(),
                }]
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

impl AgentProvider for ClaudeCode {
    fn kind(&self) -> AgentKind {
        AgentKind::ClaudeCode
    }

    fn detect_roots(&self) -> Vec<PathBuf> {
        detected_profiles()
    }

    fn claims_root(&self, dir: &Path) -> bool {
        looks_like_profile(dir)
    }

    fn discover(&self, workspace: &Path, roots: &[PathBuf]) -> Vec<SessionRef> {
        // Registry first: it answers "which sessions are live, where, and are
        // they working" from one small file per process. Slug discovery fills
        // in everything the registry no longer tracks (process gone).
        let mut by_id: HashMap<String, SessionRef> = HashMap::new();

        for session in registry_sessions(workspace, roots) {
            by_id.insert(session.id.clone(), session);
        }

        for path in candidate_transcripts(workspace, roots) {
            let Some(meta) = read_metadata(&path) else {
                continue;
            };
            // Any recorded position inside the workspace claims the
            // session; none at all means we can't attribute it to anything.
            if !meta
                .cwds
                .iter()
                .any(|cwd| is_within_workspace(cwd, workspace))
            {
                continue;
            }
            let Some(id) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            // Live registry entry wins: it already carries real activity.
            // Transcript-only sessions have no live process → Stale.
            by_id.entry(id.clone()).or_insert(SessionRef {
                id,
                agent: AgentKind::ClaudeCode,
                title: meta.title,
                last_activity: meta.last_activity,
                activity: AgentActivity::Stale,
            });
        }

        let mut sessions: Vec<SessionRef> = by_id.into_values().collect();
        // Negated rather than reversed so the most recent session is first.
        sessions.sort_by_key(|session| -session.last_activity);
        sessions
    }

    fn poll(
        &mut self,
        workspace: &Path,
        session: &SessionRef,
        roots: &[PathBuf],
    ) -> Vec<AgentEvent> {
        let mut events = Vec::new();

        // Activity is driven by the registry, not the transcript: status is
        // two-valued and lives in the small per-pid file. Re-check every poll
        // so a busy→idle flip surfaces even when the transcript is quiet.
        if let Some((activity, at)) = registry_activity_for(&session.id, roots) {
            if self.last_activity.get(&session.id) != Some(&activity) {
                self.last_activity
                    .insert(session.id.clone(), activity.clone());
                events.push(AgentEvent::ActivityChanged {
                    session_id: session.id.clone(),
                    at,
                    activity,
                });
            }
        } else if self.last_activity.get(&session.id) != Some(&AgentActivity::Stale) {
            // No live registry entry → process is gone.
            let at = now_millis();
            self.last_activity
                .insert(session.id.clone(), AgentActivity::Stale);
            events.push(AgentEvent::ActivityChanged {
                session_id: session.id.clone(),
                at,
                activity: AgentActivity::Stale,
            });
        }

        let Some(path) = self.transcript_path(workspace, session, roots) else {
            return events;
        };
        let Ok(mut file) = File::open(&path) else {
            return events;
        };
        let Ok(len) = file.metadata().map(|m| m.len()) else {
            return events;
        };

        // First sight of a session starts at the end: a workspace opened
        // mid-task must not replay an hour of history into the feed, which
        // is the same "watching since" rule the phase-1 watcher follows.
        let offset = *self.offsets.entry(path.clone()).or_insert(len);
        // Truncated or replaced (a new session reusing the name) — start over.
        let offset = if offset > len { 0 } else { offset };

        if offset == len {
            return events;
        }
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return events;
        }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return events;
        }

        // Only consume through the last newline. A record still being written
        // stays unread and is picked up whole on the next poll.
        let Some(last_newline) = bytes.iter().rposition(|byte| *byte == b'\n') else {
            return events;
        };
        let complete = &bytes[..=last_newline];
        self.offsets.insert(path, offset + complete.len() as u64);

        for line in String::from_utf8_lossy(complete).lines() {
            if line.trim().is_empty() {
                continue;
            }
            self.stats.records += 1;
            match serde_json::from_str::<Value>(line) {
                Ok(record) => events.extend(events_from(&record, workspace)),
                Err(_) => self.stats.skipped += 1,
            }
            if events.len() >= MAX_EVENTS_PER_POLL {
                break;
            }
        }
        events
    }
}

// -- live session registry --------------------------------------------------

/// One entry from `<config-root>/sessions/<pid>.json`.
struct RegistryEntry {
    session_id: String,
    cwd: String,
    title: Option<String>,
    status: String,
    pid: u32,
    proc_start: Option<String>,
    updated_at: i64,
    status_updated_at: i64,
}

/// Live (or recently live) sessions from every root's registry, filtered to
/// those whose `cwd` is the workspace or a descendant.
fn registry_sessions(workspace: &Path, roots: &[PathBuf]) -> Vec<SessionRef> {
    let mut out = Vec::new();
    for root in roots {
        let sessions_dir = root.join("sessions");
        let Ok(entries) = std::fs::read_dir(&sessions_dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let Some(reg) = read_registry_entry(&path) else {
                continue;
            };
            if !is_within_workspace(&reg.cwd, workspace) {
                continue;
            }
            let activity = activity_from_registry(&reg);
            out.push(SessionRef {
                id: reg.session_id,
                agent: AgentKind::ClaudeCode,
                title: reg.title,
                last_activity: reg.status_updated_at.max(reg.updated_at),
                activity,
            });
        }
    }
    out
}

/// Current activity for `session_id` from any root's registry, if a live
/// entry still exists.
fn registry_activity_for(session_id: &str, roots: &[PathBuf]) -> Option<(AgentActivity, i64)> {
    for root in roots {
        let sessions_dir = root.join("sessions");
        let Ok(entries) = std::fs::read_dir(&sessions_dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let Some(reg) = read_registry_entry(&path) else {
                continue;
            };
            if reg.session_id != session_id {
                continue;
            }
            let at = reg.status_updated_at.max(reg.updated_at);
            return Some((activity_from_registry(&reg), at));
        }
    }
    None
}

fn read_registry_entry(path: &Path) -> Option<RegistryEntry> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let session_id = value.get("sessionId")?.as_str()?.to_string();
    let cwd = value.get("cwd")?.as_str()?.to_string();
    let pid = value.get("pid")?.as_u64()? as u32;
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("idle")
        .to_string();
    let proc_start = value
        .get("procStart")
        .and_then(Value::as_str)
        .map(str::to_string);
    let title = value
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string);
    // Heartbeats are epoch milliseconds (observed against 2.1.224).
    let updated_at = value.get("updatedAt").and_then(Value::as_i64).unwrap_or(0);
    let status_updated_at = value
        .get("statusUpdatedAt")
        .and_then(Value::as_i64)
        .unwrap_or(updated_at);
    Some(RegistryEntry {
        session_id,
        cwd,
        title,
        status,
        pid,
        proc_start,
        updated_at,
        status_updated_at,
    })
}

/// Map a registry entry to activity.
///
/// Claude Code's `status` is two-valued (`busy` / `idle`). A permission
/// prompt still reads `busy`, so **Blocked cannot be reported** from this
/// source. Inventing a heuristic (unmatched `tool_use` + quiet transcript)
/// is available but lower-confidence; under-claiming is the standing rule,
/// so busy always maps to `Working` rather than falsely claiming Blocked.
fn activity_from_registry(reg: &RegistryEntry) -> AgentActivity {
    if !process_is_alive(reg.pid, reg.proc_start.as_deref(), reg.status_updated_at) {
        return AgentActivity::Stale;
    }
    match reg.status.as_str() {
        "busy" => AgentActivity::Working { detail: None },
        "idle" => AgentActivity::Idle,
        // Unknown status values degrade to Idle rather than guessing busy.
        _ => AgentActivity::Idle,
    }
}

/// Is the process at `pid` still the one that wrote this registry entry?
///
/// On Linux, `/proc/<pid>` existing is the liveness check; `procStart` (the
/// kernel start-time tick from `/proc/<pid>/stat` field 22) guards against
/// PID reuse when present. On non-Linux there is no `/proc`, so the
/// documented fallback is heartbeat freshness using `statusUpdatedAt`.
fn process_is_alive(pid: u32, proc_start: Option<&str>, status_updated_at: i64) -> bool {
    #[cfg(target_os = "linux")]
    {
        let _ = status_updated_at;
        let proc_dir = PathBuf::from(format!("/proc/{pid}"));
        if !proc_dir.is_dir() {
            return false;
        }
        if let Some(expected) = proc_start {
            if let Some(actual) = linux_proc_start(pid) {
                return actual == expected;
            }
        }
        true
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (pid, proc_start);
        let now = now_millis();
        now.saturating_sub(status_updated_at) <= HEARTBEAT_STALE_MS
    }
}

/// Kernel start-time ticks for `pid` (`/proc/<pid>/stat` field 22), when
/// readable. Compared to the registry's `procStart` to defeat PID reuse.
#[cfg(target_os = "linux")]
fn linux_proc_start(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // `stat` is `pid (comm) state ppid ... starttime ...`. `comm` can hold
    // spaces and parentheses, so split on the final `) ` and then take the
    // 20th field of what remains (1-indexed field 22 overall = index 19
    // after the state field that follows `)`).
    let after_comm = stat.rsplit_once(") ")?.1;
    after_comm.split_whitespace().nth(19).map(str::to_string)
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl ClaudeCode {
    /// Locate the transcript backing `session`, by id rather than by
    /// remembering a path: the file can appear under a different root than
    /// the one discovery happened to find it in.
    fn transcript_path(
        &self,
        workspace: &Path,
        session: &SessionRef,
        roots: &[PathBuf],
    ) -> Option<PathBuf> {
        candidate_transcripts(workspace, roots)
            .into_iter()
            .find(|path| {
                path.file_stem()
                    .is_some_and(|stem| stem == session.id.as_str())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASIC: &str = include_str!("fixtures/session-basic.jsonl");
    const HOSTILE: &str = include_str!("fixtures/session-hostile.jsonl");
    const MOVED_CWD: &str = include_str!("fixtures/session-moved-cwd.jsonl");

    /// The workspace the fixtures claim to have run in.
    fn workspace() -> &'static Path {
        Path::new("/work/demo")
    }

    fn events_of(jsonl: &str) -> Vec<AgentEvent> {
        jsonl
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .flat_map(|record| events_from(&record, workspace()))
            .collect()
    }

    // -- slug ---------------------------------------------------------------

    #[test]
    fn slug_is_the_absolute_path_with_separators_replaced() {
        assert_eq!(
            slug_for(Path::new("/home/h/git/AgentLens")),
            "-home-h-git-AgentLens"
        );
        // Case is preserved, and Windows separators collapse the same way.
        assert_eq!(slug_for(Path::new("C:\\Users\\h\\Proj")), "C:-Users-h-Proj");
    }

    #[test]
    fn slug_is_lossy_which_is_why_cwd_is_the_authority() {
        // Both of these are real, different directories. If the slug were
        // trusted as identity, one workspace would tail the other's session.
        assert_eq!(slug_for(Path::new("/a/b-c")), slug_for(Path::new("/a/b/c")));
    }

    #[test]
    fn workspace_match_ignores_separator_and_trailing_slash_noise() {
        assert!(is_within_workspace("/work/demo", workspace()));
        assert!(is_within_workspace("/work/demo/", workspace()));
        assert!(!is_within_workspace("/work/demo-other", workspace()));
    }

    #[test]
    fn a_session_that_cds_into_a_subdirectory_still_belongs_to_the_workspace() {
        // `cwd` is where the agent was standing, not the workspace root, and
        // it moves: one `cd src-tauri` would otherwise lose the session.
        assert!(is_within_workspace("/work/demo/src/deep", workspace()));
    }

    #[test]
    fn a_session_started_above_the_workspace_is_not_claimed() {
        // It may be working on something else entirely; the plan's rule is to
        // prefer under-claiming to wrong attribution.
        assert!(!is_within_workspace("/work", workspace()));
        assert!(!is_within_workspace("/", workspace()));
    }

    // -- extraction ---------------------------------------------------------

    #[test]
    fn extracts_tool_calls_with_relative_paths_and_bash_descriptions() {
        let events = events_of(BASIC);

        let calls: Vec<(&str, Vec<String>, Option<&str>)> = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::ToolCall {
                    tool,
                    paths,
                    summary,
                    ..
                } => Some((tool.as_str(), paths.clone(), summary.as_deref())),
                _ => None,
            })
            .collect();

        assert_eq!(
            calls,
            vec![
                ("Read", vec!["src/server.rs".to_string()], None),
                ("Edit", vec!["src/server.rs".to_string()], None),
                ("Write", vec!["src/health.rs".to_string()], None),
                // Bash names a command, not a file; its description is the
                // one-line intent the feed shows.
                ("Bash", vec![], Some("Run the test suite")),
            ]
        );
    }

    #[test]
    fn two_tool_calls_in_one_record_become_two_events() {
        // A single assistant turn can batch several calls; collapsing them
        // would lose which file each one touched.
        let events = events_of(BASIC);
        let at_same_moment = events
            .iter()
            .filter(|event| matches!(event, AgentEvent::ToolCall { at, .. } if *at == 1_772_614_809_000))
            .count();
        assert_eq!(at_same_moment, 2);
    }

    #[test]
    fn the_user_instruction_becomes_the_intent_note() {
        let events = events_of(BASIC);
        let notes: Vec<&str> = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::AssistantNote { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(notes, vec!["add a health endpoint"]);
    }

    #[test]
    fn timestamps_become_epoch_millis() {
        let first = events_of(BASIC)
            .into_iter()
            .find_map(|event| match event {
                AgentEvent::ToolCall { at, .. } => Some(at),
                _ => None,
            })
            .expect("a tool call");
        assert_eq!(first, epoch_millis("2026-03-04T09:00:05.250Z").unwrap());
    }

    // -- hostile input ------------------------------------------------------

    #[test]
    fn survives_unknown_types_bad_json_and_null_fields() {
        let events = events_of(HOSTILE);
        let paths: Vec<String> = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::ToolCall { paths, .. } => Some(paths.clone()),
                _ => None,
            })
            .flatten()
            .collect();

        // The record with null `version`/`isSidechain` still yields its call.
        assert_eq!(paths, vec!["kept.rs".to_string()]);
    }

    #[test]
    fn a_broken_timestamp_drops_the_record_rather_than_dating_it_1970() {
        // Attribution is timing-based, so a record at the epoch would claim
        // filesystem events it had nothing to do with.
        let events = events_of(HOSTILE);
        assert!(events.iter().all(|event| !matches!(
            event,
            AgentEvent::ToolCall { at, .. } if *at == 0
        )));
    }

    #[test]
    fn paths_outside_the_workspace_are_dropped() {
        // The protocol only carries workspace-relative paths, and there is
        // nothing in the tree to attribute an outside file to.
        let events = events_of(HOSTILE);
        assert!(events.iter().all(|event| !matches!(
            event,
            AgentEvent::ToolCall { paths, .. } if paths.iter().any(|p| p.contains("outside"))
        )));
    }

    #[test]
    fn subagent_work_is_flagged() {
        let sidechained: Vec<bool> = events_of(HOSTILE)
            .iter()
            .filter_map(|event| match event {
                AgentEvent::ToolCall { sidechain, .. } => Some(*sidechain),
                _ => None,
            })
            .collect();
        // Only the `isSidechain: true` record survives with paths dropped, so
        // the flag is what distinguishes it.
        assert!(sidechained.contains(&true));
    }

    // -- tailing ------------------------------------------------------------

    /// A transcript tree on disk: `<root>/projects/<slug>/<id>.jsonl`.
    /// Returns the tempdir (kept alive by the caller), the root to hand the
    /// provider, and the transcript path.
    fn transcript_tree(contents: &str, id: &str) -> (tempfile::TempDir, Vec<PathBuf>, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("projects").join(slug_for(workspace()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(format!("{id}.jsonl"));
        std::fs::write(&file, contents).unwrap();
        let roots = vec![root.path().to_path_buf()];
        (root, roots, file)
    }

    fn session(id: &str) -> SessionRef {
        SessionRef {
            id: id.to_string(),
            agent: AgentKind::ClaudeCode,
            title: None,
            last_activity: 0,
            activity: AgentActivity::Stale,
        }
    }

    /// Only *detection* reads process-global env now — everything else takes
    /// its roots as an argument. These few take a lock rather than racing.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Run `body` with the environment profile discovery depends on. `HOME` is
    /// always redirected: without it these tests would also enumerate whatever
    /// real `~/.claude*` profiles the developer's machine happens to have.
    fn with_env<T>(config_dir: Option<&Path>, home: &Path, body: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous_config = std::env::var_os("CLAUDE_CONFIG_DIR");
        let previous_home = std::env::var_os("HOME");

        match config_dir {
            Some(dir) => std::env::set_var("CLAUDE_CONFIG_DIR", dir),
            None => std::env::remove_var("CLAUDE_CONFIG_DIR"),
        }
        std::env::set_var("HOME", home);

        let out = body();

        match previous_config {
            Some(value) => std::env::set_var("CLAUDE_CONFIG_DIR", value),
            None => std::env::remove_var("CLAUDE_CONFIG_DIR"),
        }
        match previous_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        out
    }

    /// A profile directory holding one transcript for the test workspace.
    fn profile_with_session(home: &Path, name: &str, session_file: &str) {
        let dir = home.join(name).join("projects").join(slug_for(workspace()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(session_file), BASIC).unwrap();
    }

    #[test]
    fn finds_sessions_across_every_profile_not_just_the_active_one() {
        // The app is launched from a desktop icon, so it inherits no
        // CLAUDE_CONFIG_DIR at all — enumerating the siblings from disk is the
        // only way it sees the profiles the work actually happens in.
        let home = tempfile::tempdir().unwrap();
        profile_with_session(home.path(), ".claude", "default.jsonl");
        profile_with_session(home.path(), ".claude-work", "work.jsonl");
        profile_with_session(home.path(), ".claude-crypto", "crypto.jsonl");

        let roots = with_env(None, home.path(), || ClaudeCode::new().detect_roots());
        let found = ClaudeCode::new().discover(workspace(), &roots);
        let mut ids: Vec<&str> = found.iter().map(|s| s.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["crypto", "default", "work"]);
    }

    #[test]
    fn ignores_things_that_look_like_profiles_but_are_not() {
        let home = tempfile::tempdir().unwrap();
        profile_with_session(home.path(), ".claude-real", "real.jsonl");
        // A config *file*, not a directory — `~/.claude.json` really exists.
        std::fs::write(home.path().join(".claude.json"), "{}").unwrap();
        // A directory with no `projects/`: a backup, not a live profile.
        std::fs::create_dir_all(home.path().join(".claude-backup/skills")).unwrap();

        let roots = with_env(None, home.path(), || ClaudeCode::new().detect_roots());
        assert_eq!(roots, vec![home.path().join(".claude-real")]);
    }

    #[test]
    fn an_explicit_config_dir_is_searched_as_well_as_the_siblings() {
        // Someone whose profile breaks the naming convention still works.
        let home = tempfile::tempdir().unwrap();
        profile_with_session(home.path(), ".claude", "default.jsonl");

        let elsewhere = tempfile::tempdir().unwrap();
        let dir = elsewhere
            .path()
            .join("projects")
            .join(slug_for(workspace()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("odd.jsonl"), BASIC).unwrap();

        let roots = with_env(Some(elsewhere.path()), home.path(), || {
            ClaudeCode::new().detect_roots()
        });
        let found = ClaudeCode::new().discover(workspace(), &roots);
        let mut ids: Vec<&str> = found.iter().map(|s| s.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["default", "odd"]);
    }

    /// Strip activity transitions so tailing assertions stay about the
    /// transcript. Without a registry entry the first poll emits Stale once.
    fn tool_events(events: &[AgentEvent]) -> Vec<&AgentEvent> {
        events
            .iter()
            .filter(|event| !matches!(event, AgentEvent::ActivityChanged { .. }))
            .collect()
    }

    #[test]
    fn first_poll_starts_at_the_end_rather_than_replaying_history() {
        let id = "1111aaaa-0000-4000-8000-000000000001";
        let (_dir, roots, file) = transcript_tree(BASIC, id);

        let mut provider = ClaudeCode::new();
        let first = provider.poll(workspace(), &session(id), &roots);
        assert!(
            tool_events(&first).is_empty(),
            "opening a workspace mid-task must not flood the feed"
        );

        // Now append: only the new record comes through.
        let appended = format!(
            "{}\n",
            r#"{"type":"assistant","cwd":"/work/demo","sessionId":"s","timestamp":"2026-03-04T09:01:00.000Z","message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/work/demo/late.rs"}}]}}"#
        );
        let mut handle = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        std::io::Write::write_all(&mut handle, appended.as_bytes()).unwrap();

        let events = provider.poll(workspace(), &session(id), &roots);
        let tools = tool_events(&events);
        assert_eq!(tools.len(), 1);
        assert!(matches!(
            tools[0],
            AgentEvent::ToolCall { paths, .. } if paths == &["late.rs".to_string()]
        ));
    }

    #[test]
    fn a_half_written_record_waits_for_its_newline() {
        let id = "1111aaaa-0000-4000-8000-000000000001";
        let (_dir, roots, file) = transcript_tree("", id);

        let mut provider = ClaudeCode::new();
        provider.poll(workspace(), &session(id), &roots);

        let whole = r#"{"type":"assistant","cwd":"/work/demo","sessionId":"s","timestamp":"2026-03-04T09:02:00.000Z","message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/work/demo/partial.rs"}}]}}"#;
        let (head, tail) = whole.split_at(60);

        std::fs::write(&file, head).unwrap();
        assert!(
            provider.poll(workspace(), &session(id), &roots).is_empty(),
            "a record still being written must not be parsed"
        );

        std::fs::write(&file, format!("{whole}\n")).unwrap();
        let events = provider.poll(workspace(), &session(id), &roots);
        assert_eq!(
            tool_events(&events).len(),
            1,
            "and must arrive whole once complete"
        );
        assert!(!tail.is_empty());
    }

    #[test]
    fn a_truncated_transcript_restarts_instead_of_reading_past_the_end() {
        let id = "1111aaaa-0000-4000-8000-000000000001";
        let (_dir, roots, file) = transcript_tree(BASIC, id);

        let mut provider = ClaudeCode::new();
        provider.poll(workspace(), &session(id), &roots);

        // A new session reusing the name, shorter than the old offset.
        std::fs::write(
            &file,
            format!(
                "{}\n",
                r#"{"type":"assistant","cwd":"/work/demo","sessionId":"s","timestamp":"2026-03-04T09:03:00.000Z","message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/work/demo/fresh.rs"}}]}}"#
            ),
        )
        .unwrap();

        let events = provider.poll(workspace(), &session(id), &roots);
        assert_eq!(tool_events(&events).len(), 1);
    }

    #[test]
    fn counts_unparseable_lines_without_failing() {
        let id = "2222bbbb-0000-4000-8000-000000000002";
        let (_dir, roots, file) = transcript_tree("", id);

        let mut provider = ClaudeCode::new();
        provider.poll(workspace(), &session(id), &roots);
        std::fs::write(&file, HOSTILE).unwrap();
        provider.poll(workspace(), &session(id), &roots);

        assert_eq!(provider.stats.skipped, 1, "the one non-JSON line");
        assert!(provider.stats.records >= 5);
    }

    // -- discovery ----------------------------------------------------------

    #[test]
    fn discovers_only_sessions_whose_cwd_matches_the_workspace() {
        let root = tempfile::tempdir().unwrap();
        let projects = root.path().join("projects");

        // Right slug, right cwd.
        let ours = projects.join(slug_for(workspace()));
        std::fs::create_dir_all(&ours).unwrap();
        std::fs::write(ours.join("mine.jsonl"), BASIC).unwrap();

        // Colliding slug, different cwd — the case the lossy slug creates.
        std::fs::write(
            ours.join("theirs.jsonl"),
            format!(
                "{}\n",
                r#"{"type":"assistant","cwd":"/work/demo-other","sessionId":"x","timestamp":"2026-03-04T11:00:00.000Z"}"#
            ),
        )
        .unwrap();

        let found = ClaudeCode::new().discover(workspace(), &[root.path().to_path_buf()]);
        let ids: Vec<&str> = found.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["mine"]);
        assert_eq!(found[0].title.as_deref(), Some("Add a health endpoint"));
        assert_eq!(
            found[0].last_activity,
            epoch_millis("2026-03-04T09:00:14.500Z").unwrap()
        );
    }

    #[test]
    fn discovers_a_session_whose_recent_records_moved_into_a_subdirectory() {
        // Regression: caught only by running against a live transcript. The
        // last records of a real session said `cwd` was a subdirectory, and
        // an equality check dropped the session that was running right then.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("projects").join(slug_for(workspace()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("moved.jsonl"), MOVED_CWD).unwrap();

        let found = ClaudeCode::new().discover(workspace(), &[root.path().to_path_buf()]);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].title.as_deref(),
            Some("Work that changes directory")
        );
    }

    #[test]
    fn finds_the_session_when_the_slug_scheme_differs() {
        // Guards the platform we can't verify here: if Windows substitutes a
        // different separator, discovery must still find the session rather
        // than silently reporting no agent. Any scheme that only swaps
        // punctuation reduces to the same key. A scheme that adds or drops a
        // path *component* legitimately doesn't match — that would be a
        // different workspace.
        for scheme in ["_work_demo", "work-demo", "-WORK-DEMO"] {
            let root = tempfile::tempdir().unwrap();
            let odd = root.path().join("projects").join(scheme);
            std::fs::create_dir_all(&odd).unwrap();
            std::fs::write(odd.join("mine.jsonl"), BASIC).unwrap();

            let found = ClaudeCode::new().discover(workspace(), &[root.path().to_path_buf()]);
            assert_eq!(found.len(), 1, "{scheme} should have matched");
        }
    }

    #[test]
    fn a_root_without_this_workspace_reads_no_transcripts_at_all() {
        // Regression, and the expensive kind. This used to fall back to every
        // project directory in the root, reading the tail of every transcript
        // on the machine — ~600 ms and 300+ files per call here, repeated on
        // every poll. With several profiles configured most roots legitimately
        // have nothing for a given workspace, so this is the common path.
        let root = tempfile::tempdir().unwrap();
        let projects = root.path().join("projects");
        for other in ["-home-someone-else", "-var-tmp-scratch", "-work-other"] {
            let dir = projects.join(other);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("theirs.jsonl"), BASIC).unwrap();
        }

        let considered = candidate_transcripts(workspace(), &[root.path().to_path_buf()]);
        assert!(
            considered.is_empty(),
            "no directory names this workspace, so nothing should be opened: {considered:?}"
        );
    }

    #[test]
    fn loose_key_collapses_any_separator_scheme() {
        let path = loose_key("/home/h/git/AgentLens");
        assert_eq!(loose_key("-home-h-git-AgentLens"), path);
        assert_eq!(loose_key("_home_h_git_agentlens"), path);
        assert_ne!(loose_key("-home-h-git-AgentLens2"), path);
    }

    #[test]
    fn no_transcripts_anywhere_is_an_empty_list_not_an_error() {
        let root = tempfile::tempdir().unwrap();
        let roots = vec![root.path().to_path_buf()];
        assert!(ClaudeCode::new().discover(workspace(), &roots).is_empty());
        // No registry either: first poll surfaces Stale once, then silence.
        let mut provider = ClaudeCode::new();
        let first = provider.poll(workspace(), &session("nope"), &roots);
        assert!(matches!(
            &first[..],
            [AgentEvent::ActivityChanged {
                activity: AgentActivity::Stale,
                ..
            }]
        ));
        assert!(provider
            .poll(workspace(), &session("nope"), &roots)
            .is_empty());
    }

    // -- live session registry ----------------------------------------------

    /// Write a registry entry under `<root>/sessions/<pid>.json`.
    fn write_registry(root: &Path, pid: u32, body: &str) {
        let dir = root.join("sessions");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{pid}.json")), body).unwrap();
    }

    /// A synthetic registry file. Paths and ids are invented — never real.
    fn registry_json(pid: u32, session_id: &str, cwd: &str, status: &str) -> String {
        format!(
            r#"{{
                "pid": {pid},
                "sessionId": "{session_id}",
                "cwd": "{cwd}",
                "startedAt": 1786121838290,
                "procStart": "1",
                "version": "2.1.224",
                "kind": "interactive",
                "entrypoint": "cli",
                "name": "project-63",
                "status": "{status}",
                "updatedAt": 1786122165624,
                "statusUpdatedAt": 1786122165624
            }}"#
        )
    }

    /// A live registry entry for this test process. Heartbeats use *now* so
    /// the non-Linux freshness fallback also treats the session as alive;
    /// on Linux, `procStart` matches `/proc/self`.
    fn live_registry_json(session_id: &str, status: &str) -> String {
        let me = std::process::id();
        let start = linux_start_for_tests(me);
        let now = now_millis();
        format!(
            r#"{{
                "pid": {me},
                "sessionId": "{session_id}",
                "cwd": "/work/demo",
                "startedAt": {now},
                "procStart": "{start}",
                "version": "2.1.224",
                "kind": "interactive",
                "entrypoint": "cli",
                "name": "project-63",
                "status": "{status}",
                "updatedAt": {now},
                "statusUpdatedAt": {now}
            }}"#
        )
    }

    #[test]
    fn registry_busy_maps_to_working() {
        // busy is the only live "doing something" signal Claude Code exposes.
        // It cannot mean Blocked — a permission prompt still reads busy — so
        // Working is the honest under-claim.
        let root = tempfile::tempdir().unwrap();
        let me = std::process::id();
        write_registry(root.path(), me, &live_registry_json("live-busy", "busy"));

        let found = ClaudeCode::new().discover(workspace(), &[root.path().to_path_buf()]);
        let live = found
            .iter()
            .find(|s| s.id == "live-busy")
            .expect("registry session");
        assert_eq!(
            live.activity,
            AgentActivity::Working { detail: None },
            "busy → Working; never Blocked from Claude Code"
        );
    }

    #[test]
    fn registry_idle_maps_to_idle() {
        let root = tempfile::tempdir().unwrap();
        let me = std::process::id();
        write_registry(root.path(), me, &live_registry_json("live-idle", "idle"));

        let found = ClaudeCode::new().discover(workspace(), &[root.path().to_path_buf()]);
        let live = found
            .iter()
            .find(|s| s.id == "live-idle")
            .expect("registry session");
        assert_eq!(live.activity, AgentActivity::Idle);
    }

    #[test]
    fn registry_dead_pid_maps_to_stale() {
        // A registry file whose process is gone is a stale session, not a
        // live one. A non-existent PID with a cold heartbeat covers both
        // Linux (/proc missing) and the non-Linux freshness fallback.
        let root = tempfile::tempdir().unwrap();
        write_registry(
            root.path(),
            4_294_967_294,
            &registry_json(4_294_967_294, "dead-session", "/work/demo", "busy"),
        );

        let found = ClaudeCode::new().discover(workspace(), &[root.path().to_path_buf()]);
        let dead = found
            .iter()
            .find(|s| s.id == "dead-session")
            .expect("stale registry entry still surfaces");
        assert_eq!(dead.activity, AgentActivity::Stale);
    }

    #[test]
    fn registry_is_primary_slug_fills_in_the_rest() {
        let root = tempfile::tempdir().unwrap();
        // Transcript-only (no live process).
        let projects = root.path().join("projects").join(slug_for(workspace()));
        std::fs::create_dir_all(&projects).unwrap();
        std::fs::write(projects.join("old-only.jsonl"), BASIC).unwrap();

        // Live registry session with no transcript yet.
        let me = std::process::id();
        write_registry(root.path(), me, &live_registry_json("brand-new", "busy"));

        let found = ClaudeCode::new().discover(workspace(), &[root.path().to_path_buf()]);
        let mut ids: Vec<&str> = found.iter().map(|s| s.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["brand-new", "old-only"]);
        let old = found.iter().find(|s| s.id == "old-only").unwrap();
        assert_eq!(
            old.activity,
            AgentActivity::Stale,
            "transcript without a live process is Stale"
        );
    }

    /// `procStart` for this process on Linux; a dummy on other hosts where
    /// the heartbeat fallback is what decides liveness.
    fn linux_start_for_tests(pid: u32) -> String {
        #[cfg(target_os = "linux")]
        {
            linux_proc_start(pid).unwrap_or_else(|| "0".into())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pid;
            "0".into()
        }
    }
}
