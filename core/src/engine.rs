//! One object that answers every [`Command`], and one method that does it.
//!
//! This is what a backend *is*. The desktop app holds an `Engine` directly and
//! calls `handle` in-process; the daemon holds one and calls `handle` with
//! whatever came off stdin. Neither knows about the other, and neither can
//! drift from the other, because there is only the one implementation.
//!
//! What is deliberately **not** here: persistence. Recent workspaces, the
//! settings store, window state — all of that belongs to whoever is driving,
//! and a daemon inside a WSL distro has no business writing config files on
//! the user's behalf. The engine is told its settings and forgets them when
//! the process ends.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::agents::claude::ClaudeCode;
use crate::agents::grok::Grok;
use crate::agents::{self, AgentProvider};
use crate::correlate::Correlator;
use crate::protocol::{
    AgentActivity, AgentEvent, AgentKind, AgentPoll, Command, CommandResult, Hello, SessionRef,
    WorkspaceInfo, CAPABILITIES, PROTOCOL_VERSION,
};
use crate::settings::{self, SettingsState};
use crate::snapshots::{self, SessionState};
use crate::watcher::{self, EventSink, WatcherManager};
use crate::workspace::{self, WorkspaceState};
use crate::{browse, gitops, gitstatus, preview, tree};

/// How often the agent poller discovers sessions and tails them.
///
/// One second is a named constant rather than a magic number because the
/// activity indicator and the correlation window both depend on it: faster
/// would thrash disk for no user-visible gain, slower would make the header
/// lag behind a turn that already finished.
const AGENT_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Per-workspace agent providers. They own their read offsets, so they must
/// outlive a single command — a fresh provider each poll would re-tail from
/// the end and never report anything. They must *not* outlive the workspace,
/// hence the reset on open and close.
struct AgentState {
    claude: ClaudeCode,
    grok: Grok,
}

impl AgentState {
    fn new() -> Self {
        AgentState {
            claude: ClaudeCode::new(),
            grok: Grok::new(),
        }
    }

    fn discover(&self, workspace: &Path, roots: &[PathBuf]) -> Vec<SessionRef> {
        let mut sessions = self.claude.discover(workspace, roots);
        sessions.extend(self.grok.discover(workspace, roots));
        sessions.sort_by_key(|session| -session.last_activity);
        sessions
    }

    fn poll(
        &mut self,
        workspace: &Path,
        session: &SessionRef,
        roots: &[PathBuf],
    ) -> (Vec<crate::protocol::AgentEvent>, u64, u64) {
        match session.agent {
            AgentKind::ClaudeCode => {
                let events = self.claude.poll(workspace, session, roots);
                (events, self.claude.stats.records, self.claude.stats.skipped)
            }
            AgentKind::Grok => {
                let events = self.grok.poll(workspace, session, roots);
                (events, self.grok.stats.records, self.grok.stats.skipped)
            }
        }
    }
}

/// The observing half of AgentLens, driven by commands.
pub struct Engine {
    workspace: WorkspaceState,
    watcher: Arc<WatcherManager>,
    settings: SettingsState,
    session: SessionState,
    agents: Arc<Mutex<AgentState>>,
    /// Correlation decorator around the real sink. The watcher and the
    /// poller both talk to this; it forwards to the caller's sink.
    correlator: Arc<Correlator>,
    /// Same object as `correlator`, held as a trait object so `watcher::start`
    /// and git mutations can take `&Arc<dyn EventSink>` without re-wrapping.
    sink: Arc<dyn EventSink>,
    /// Bumped on every poller start and stop. The poller thread remembers
    /// the generation it was spawned with and exits once they differ — the
    /// same pattern as [`WatcherManager`], so reopening a workspace cannot
    /// leave two pollers running.
    poller_generation: Arc<AtomicU64>,
}

impl Engine {
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        let correlator = Arc::new(Correlator::new(sink));
        let sink: Arc<dyn EventSink> = correlator.clone();
        Engine {
            workspace: WorkspaceState::default(),
            watcher: Arc::new(WatcherManager::default()),
            settings: SettingsState::default(),
            session: SessionState::default(),
            agents: Arc::new(Mutex::new(AgentState::new())),
            correlator,
            sink,
            poller_generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Run `command` and return its result as JSON.
    ///
    /// JSON rather than a `Response` enum with one variant per command: the
    /// value is about to be serialized either way (into a webview or into a
    /// pipe), so a second enum would only be a second place for the shapes to
    /// disagree. Each arm still builds a typed value from `protocol` first.
    pub fn handle(&self, command: Command) -> CommandResult<Value> {
        match command {
            Command::Hello { protocol_version } => {
                if protocol_version != PROTOCOL_VERSION {
                    return Err(format!(
                        "protocol version mismatch: this backend speaks {PROTOCOL_VERSION}, the caller speaks {protocol_version}"
                    ));
                }
                ok(Hello {
                    name: env!("CARGO_PKG_NAME").to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    protocol_version: PROTOCOL_VERSION,
                    // Reported so a newer app can ask an older daemon what it
                    // supports without guessing from the package version.
                    capabilities: CAPABILITIES.iter().map(|s| (*s).to_string()).collect(),
                })
            }
            Command::Ping => ok(()),

            Command::OpenWorkspace { path } => self.open_workspace(&path),
            Command::CloseWorkspace => self.close_workspace(),
            Command::CurrentWorkspace => {
                ok(workspace::current_opt(&self.workspace)?.map(|w| w.info()))
            }
            Command::RestartSession => {
                let ws = workspace::restart_session(&self.workspace)?;
                snapshots::restart(&self.session, &ws.root)?;
                ok(ws.info())
            }
            Command::GetWatcherStatus => ok(watcher::status(&self.watcher)),

            Command::ListDir { path } => {
                let ws = workspace::current(&self.workspace)?;
                ok(tree::list_dir(
                    &ws.root,
                    &path,
                    &settings::current_matcher(&self.settings),
                    &settings::current_visibility(&self.settings)?,
                )?)
            }
            Command::ListFiles => {
                let ws = workspace::current(&self.workspace)?;
                ok(tree::list_files(
                    &ws.root,
                    &settings::current_matcher(&self.settings),
                    &settings::current_visibility(&self.settings)?,
                ))
            }
            // No workspace required, unlike every other listing: this is how a
            // workspace gets chosen in the first place.
            Command::BrowseDir { path } => ok(browse::list(path.as_deref())?),
            Command::PinnedEntries => {
                let ws = workspace::current(&self.workspace)?;
                ok(tree::pinned_entries(
                    &ws.root,
                    &settings::current_visibility(&self.settings)?,
                ))
            }
            Command::ReadPreview { path } => {
                let ws = workspace::current(&self.workspace)?;
                ok(preview::read(&ws.root, &path)?)
            }
            Command::ResolveForOpen { path } => {
                let ws = workspace::current(&self.workspace)?;
                let resolved = preview::resolve_for_open(&ws.root, &path)?;
                ok(crate::paths::normalize_absolute(&resolved))
            }
            Command::SessionDiff { path } => {
                let ws = workspace::current(&self.workspace)?;
                ok(snapshots::diff(&self.session, &ws.root, &path)?)
            }
            Command::GitDiff { path, staged } => {
                let ws = workspace::current(&self.workspace)?;
                ok(gitops::file_diff(&ws.root, &path, staged)?)
            }

            Command::GitStatus => {
                let ws = workspace::current(&self.workspace)?;
                ok(gitstatus::status(&ws.root)?)
            }
            Command::GitCapabilities => {
                let ws = workspace::current(&self.workspace)?;
                ok(gitops::capabilities(&ws.root))
            }
            Command::GitStage { paths } => self.mutate(|root| gitops::stage(root, &paths)),
            Command::GitStageAll => self.mutate(gitops::stage_all),
            Command::GitUnstage { paths } => self.mutate(|root| gitops::unstage(root, &paths)),
            Command::GitUnstageAll => self.mutate(gitops::unstage_all),
            Command::GitCommit { message, amend } => {
                self.mutate(|root| gitops::commit(root, &message, amend))
            }
            Command::GitBranches => {
                let ws = workspace::current(&self.workspace)?;
                ok(gitops::branches(&ws.root)?)
            }
            Command::GitSwitchBranch { name } => {
                self.mutate(|root| gitops::switch_branch(root, &name))
            }
            Command::GitCreateBranch { name } => {
                self.mutate(|root| gitops::create_branch(root, &name))
            }
            Command::GitStashPush { message } => {
                self.mutate(|root| gitops::stash_push(root, message.as_deref()))
            }
            Command::GitStashPop => self.mutate(gitops::stash_pop),

            Command::GetWorkspaceSettings => ok(settings::current(&self.settings)?),
            Command::SetWorkspaceSettings { value } => {
                let ws = workspace::current(&self.workspace)?;
                settings::activate(&self.settings, &ws.root, value)?;
                self.restart_watcher(&ws.root)?;
                ok(settings::current(&self.settings)?)
            }
            Command::GetAppSettings => ok(settings::current_app(&self.settings)?),
            Command::SetAppSettings { value } => {
                settings::set_app(&self.settings, value)?;
                // App-level settings outlive the workspace, so unlike the
                // workspace scope this does not require one open — but when
                // there is one, the watcher has to pick up the new visibility
                // rules the same way.
                if let Some(ws) = workspace::current_opt(&self.workspace)? {
                    self.restart_watcher(&ws.root)?;
                }
                ok(settings::current_app(&self.settings)?)
            }

            Command::AgentSessions => {
                let ws = workspace::current(&self.workspace)?;
                let roots =
                    agents::resolve_roots(&settings::current_app(&self.settings)?.agent_roots);
                let guard = self.agents.lock().map_err(|_| "agent state poisoned")?;
                ok(guard.discover(&ws.root, &roots))
            }
            Command::AgentRoots => ok(agents::describe_roots(
                &settings::current_app(&self.settings)?.agent_roots,
            )),
            Command::AgentEvents { session } => {
                let ws = workspace::current(&self.workspace)?;
                let roots =
                    agents::resolve_roots(&settings::current_app(&self.settings)?.agent_roots);
                let mut guard = self.agents.lock().map_err(|_| "agent state poisoned")?;
                let (events, records, skipped) = guard.poll(&ws.root, &session, &roots);
                // On-demand polls still feed the correlator so a UI that
                // drives AgentEvents (instead of waiting for the background
                // poller) still attributes file changes.
                self.correlator.observe_agent_events(&events);
                ok(AgentPoll {
                    events,
                    records,
                    skipped,
                })
            }
        }
    }

    /// The workspace currently open, for callers that need it outside a
    /// command (the daemon logs it, the app labels its window with it).
    pub fn current_workspace(&self) -> Option<WorkspaceInfo> {
        workspace::current_opt(&self.workspace)
            .ok()
            .flatten()
            .map(|w| w.info())
    }

    /// Stop watching and drop every per-workspace fact. Called on shutdown so
    /// a daemon exits without leaving OS watches behind.
    pub fn shutdown(&self) {
        let _ = self.close_workspace();
    }

    /// Current poller generation. Tests assert that close/reopen bumps it so
    /// a superseded thread exits rather than fighting its replacement.
    #[cfg(test)]
    pub fn poller_generation(&self) -> u64 {
        self.poller_generation.load(Ordering::SeqCst)
    }

    fn open_workspace(&self, path: &str) -> CommandResult<Value> {
        let opened = workspace::open(&self.workspace, Path::new(path))?;
        snapshots::restart(&self.session, &opened.root)?;
        // Read offsets and parse tallies belong to the workspace, not the
        // process: without this, reopening resumes from the offset it had on
        // close and the "couldn't parse N records" counter reports another
        // workspace's totals.
        self.reset_agents()?;
        self.start_poller(opened.root.clone());
        // The watcher deliberately does not start here. It filters through
        // settings persisted against the *canonical* root, which the caller
        // only learns from this reply — so it follows with
        // `SetWorkspaceSettings`, and that starts the watch.
        ok(opened.info())
    }

    fn close_workspace(&self) -> CommandResult<Value> {
        self.stop_poller();
        watcher::stop(&self.sink, &self.watcher);
        snapshots::clear(&self.session)?;
        settings::deactivate(&self.settings)?;
        self.reset_agents()?;
        workspace::close(&self.workspace)?;
        ok(())
    }

    fn reset_agents(&self) -> CommandResult<()> {
        let mut guard = self.agents.lock().map_err(|_| "agent state poisoned")?;
        *guard = AgentState::new();
        Ok(())
    }

    /// Start (or replace) the background agent poller for `root`.
    ///
    /// Bumps the generation first so any previous poller exits on its next
    /// tick, then spawns a thread that discovers sessions and polls every
    /// provider on [`AGENT_POLL_INTERVAL`]. No agent roots, no sessions, or
    /// an unreadable directory is quiet idling — never an error, never log
    /// spam — so the app behaves exactly as it does today when no agent is
    /// present.
    fn start_poller(&self, root: PathBuf) {
        let generation = self.poller_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation_handle = Arc::clone(&self.poller_generation);
        let agents = Arc::clone(&self.agents);
        let correlator = Arc::clone(&self.correlator);
        let settings_app = Arc::new(Mutex::new(
            settings::current_app(&self.settings).unwrap_or_default(),
        ));
        // Snapshot agent_roots at start; SetAppSettings mid-session is rare
        // enough that re-reading every tick would buy little and couple the
        // poller to SettingsState's lock. Root detection is snapshotted for
        // the same reason — resolve_roots walks the filesystem — so a profile
        // directory created mid-session is picked up on the next open.
        if let Ok(app) = settings::current_app(&self.settings) {
            if let Ok(mut guard) = settings_app.lock() {
                *guard = app;
            }
        }

        thread::spawn(move || {
            // Live-session keys from the previous tick. Local to this thread
            // so it dies with the poller generation and cannot leak across
            // workspaces.
            let mut prev_live: HashSet<(AgentKind, String)> = HashSet::new();

            // Resolve once per poller generation — the settings snapshot
            // above cannot change under us, so re-walking every second buys
            // nothing (see comment on the settings_app snapshot).
            let agent_roots = settings_app
                .lock()
                .map(|g| g.agent_roots.clone())
                .unwrap_or_default();
            let roots = agents::resolve_roots(&agent_roots);

            // Sleep before the first discovery pass; a workspace with no
            // agents stays silent either way. (Root resolution above already
            // touched disk, but on this thread — open_workspace never waits
            // on any of it.)
            loop {
                thread::sleep(AGENT_POLL_INTERVAL);
                if generation_handle.load(Ordering::SeqCst) != generation {
                    break;
                }

                let Ok(mut guard) = agents.lock() else {
                    continue;
                };
                let sessions = guard.discover(&root, &roots);

                // Lifecycle is owned here, derived from what discovery sees
                // each tick — providers must not emit SessionStarted/Ended.
                let now = workspace::now_millis();
                let mut cur_live: HashSet<(AgentKind, String)> = HashSet::new();
                let mut lifecycle = Vec::new();
                for session in &sessions {
                    if session.activity == AgentActivity::Stale {
                        continue;
                    }
                    let key = (session.agent, session.id.clone());
                    if !prev_live.contains(&key) {
                        let at = if session.last_activity > 0 {
                            session.last_activity
                        } else {
                            now
                        };
                        lifecycle.push(AgentEvent::SessionStarted {
                            session_id: session.id.clone(),
                            agent: session.agent,
                            title: session.title.clone(),
                            at,
                        });
                        // SessionStarted carries no activity, and the store
                        // assumes `working` for one — so say what discovery
                        // actually found. Without this an idle session shows
                        // as working until something else moves it, and a
                        // quiet Grok session emits nothing to correct it.
                        lifecycle.push(AgentEvent::ActivityChanged {
                            session_id: session.id.clone(),
                            agent: Some(session.agent),
                            at,
                            activity: session.activity.clone(),
                        });
                    }
                    cur_live.insert(key);
                }
                for (agent, session_id) in prev_live.difference(&cur_live) {
                    lifecycle.push(AgentEvent::SessionEnded {
                        session_id: session_id.clone(),
                        agent: Some(*agent),
                        at: now,
                    });
                }
                prev_live = cur_live;

                // Lifecycle batch first, its own emit: SessionStarted should
                // arrive before any ToolCall or ActivityChanged for that
                // session. The store also inserts on an unknown id, so a
                // silent provider cannot blank the panel. These are not
                // ToolCalls, so no observe.
                if !lifecycle.is_empty() {
                    correlator.agent_events(&lifecycle);
                }

                // Poll live sessions only. discover re-evaluates liveness
                // every tick, so a session that comes back is a fresh
                // SessionStarted — no need to keep tailing stale ones.
                for session in sessions {
                    if session.activity == AgentActivity::Stale {
                        continue;
                    }
                    if generation_handle.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    let (events, _, _) = guard.poll(&root, &session, &roots);
                    if events.is_empty() {
                        continue;
                    }
                    correlator.observe_agent_events(&events);
                    correlator.agent_events(&events);
                }
            }
        });
    }

    fn stop_poller(&self) {
        // Same as watcher::stop: bumping the generation is enough. The
        // thread notices on its next tick and exits without being joined.
        self.poller_generation.fetch_add(1, Ordering::SeqCst);
    }

    /// (Re)start the watcher against the visibility rules currently in effect.
    /// The watcher holds its filters for the life of the watch, so any
    /// settings change that alters what is visible has to go through here.
    fn restart_watcher(&self, root: &Path) -> CommandResult<()> {
        watcher::start(
            &self.sink,
            &self.watcher,
            root,
            watcher::Filters::new(
                root,
                settings::current_matcher(&self.settings),
                settings::current_visibility(&self.settings)?,
            ),
        );
        Ok(())
    }

    /// Run a git mutation, then re-read status and push it out.
    ///
    /// Every git write goes through here: a mutation whose result isn't
    /// reflected immediately reads as a failure, and `.git`-only changes are
    /// filtered out of the watcher's feed, so nothing else would prompt the
    /// refresh.
    fn mutate(&self, op: impl FnOnce(&Path) -> CommandResult<()>) -> CommandResult<Value> {
        let ws = workspace::current(&self.workspace)?;
        op(&ws.root)?;
        let snapshot = gitstatus::status(&ws.root)?;
        self.sink.git_status(&snapshot);
        ok(snapshot)
    }
}

/// Serialize a command's typed result. Failure here is a bug in a protocol
/// type, not a user-visible condition, so it reports as one.
fn ok<T: Serialize>(value: T) -> CommandResult<Value> {
    serde_json::to_value(value).map_err(|e| format!("failed to encode response: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{
        AgentEvent, AppSettings, FsEvent, GitStatusSnapshot, WatcherStatus, WorkspaceSettings,
    };

    /// Wait up to ~3 s for the sink to hold at least one matching event.
    /// Prefer a bounded poll over a bare sleep so a slow CI host is not flaky.
    fn wait_for(sink: &RecordingSink, pred: impl Fn(&[AgentEvent]) -> bool) -> Vec<AgentEvent> {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            let events = sink.agent_events.lock().unwrap().clone();
            if pred(&events) {
                return events;
            }
            if std::time::Instant::now() >= deadline {
                return events;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    /// Percent-encode a path the way Grok names session cwd directories
    /// (`/` → `%2F`). Only the separators matter for these tests.
    fn percent_encode_path(path: &Path) -> String {
        path.to_string_lossy()
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect()
    }

    /// The workspace root as the engine will see it.
    ///
    /// `workspace::open` canonicalizes what it is given, and Grok's session
    /// directory name is that path percent-encoded — so a fixture built from
    /// the raw temp path is never matched and discovery finds nothing. On
    /// Linux canonicalize is usually the identity, which is exactly why
    /// encoding the raw path passed locally and failed on Windows, where it
    /// also resolves 8.3 short names and prepends `\\?\`.
    fn canonical_root(path: &Path) -> PathBuf {
        std::fs::canonicalize(path).expect("workspace tempdir must exist")
    }

    /// Write a synthetic Grok session under `agent_root` for `workspace` and
    /// return its `events.jsonl`. Fresh mtime → discovery treats it as live
    /// within the recency window.
    fn write_live_grok_session(agent_root: &Path, workspace: &Path, session_id: &str) -> PathBuf {
        let dir = agent_root
            .join("sessions")
            .join(percent_encode_path(&canonical_root(workspace)))
            .join(session_id);
        std::fs::create_dir_all(&dir).unwrap();
        let events = dir.join("events.jsonl");
        std::fs::write(&events, "").unwrap();
        events
    }

    /// Records what the engine pushes, so tests can assert on emissions
    /// without a window or a pipe.
    #[derive(Default)]
    struct RecordingSink {
        git_statuses: Mutex<Vec<GitStatusSnapshot>>,
        agent_events: Mutex<Vec<AgentEvent>>,
    }

    impl EventSink for RecordingSink {
        fn fs_changes(&self, _events: &[FsEvent]) {}
        fn git_status(&self, snapshot: &GitStatusSnapshot) {
            self.git_statuses.lock().unwrap().push(snapshot.clone());
        }
        fn watcher_status(&self, _status: &WatcherStatus) {}
        fn agent_events(&self, events: &[AgentEvent]) {
            self.agent_events
                .lock()
                .unwrap()
                .extend(events.iter().cloned());
        }
    }

    fn engine() -> (Engine, Arc<RecordingSink>) {
        let sink = Arc::new(RecordingSink::default());
        (Engine::new(sink.clone()), sink)
    }

    #[test]
    fn hello_reports_the_protocol_version_and_rejects_a_mismatch() {
        let (engine, _) = engine();

        let hello: Hello = serde_json::from_value(
            engine
                .handle(Command::Hello {
                    protocol_version: PROTOCOL_VERSION,
                })
                .unwrap(),
        )
        .unwrap();
        assert_eq!(hello.protocol_version, PROTOCOL_VERSION);
        assert_eq!(hello.name, "agentlens-core");

        let err = engine
            .handle(Command::Hello {
                protocol_version: PROTOCOL_VERSION + 1,
            })
            .unwrap_err();
        assert!(err.contains("protocol version mismatch"), "{err}");
    }

    #[test]
    fn hello_reports_the_capabilities_this_backend_ships() {
        // A newer app learns what an older daemon can do from this list —
        // empty would mean "unknown", so a live backend must fill it in.
        let (engine, _) = engine();
        let hello: Hello = serde_json::from_value(
            engine
                .handle(Command::Hello {
                    protocol_version: PROTOCOL_VERSION,
                })
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            hello.capabilities,
            CAPABILITIES
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            hello.capabilities,
            vec![
                "agents".to_string(),
                "correlation".to_string(),
                "gitops".to_string(),
                "preview".to_string(),
                "snapshots".to_string(),
            ],
        );
    }

    #[test]
    fn commands_needing_a_workspace_say_so_before_one_is_open() {
        let (engine, _) = engine();
        for command in [
            Command::ListFiles,
            Command::GitStatus,
            Command::ReadPreview { path: "a".into() },
        ] {
            assert_eq!(
                engine.handle(command).unwrap_err(),
                "no workspace is open".to_string()
            );
        }
    }

    #[test]
    fn opening_then_settings_makes_the_tree_readable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        let (engine, _) = engine();

        let info: WorkspaceInfo = serde_json::from_value(
            engine
                .handle(Command::OpenWorkspace {
                    path: dir.path().to_string_lossy().into_owned(),
                })
                .unwrap(),
        )
        .unwrap();
        assert!(info.root.ends_with(&info.name));

        engine
            .handle(Command::SetWorkspaceSettings {
                value: WorkspaceSettings::default(),
            })
            .unwrap();

        let files: Vec<String> =
            serde_json::from_value(engine.handle(Command::ListFiles).unwrap()).unwrap();
        assert_eq!(files, vec!["a.txt".to_string()]);
    }

    #[test]
    fn extra_ignores_take_effect_without_reopening() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        std::fs::write(dir.path().join("b.tmp"), "scratch").unwrap();
        let (engine, _) = engine();
        engine
            .handle(Command::OpenWorkspace {
                path: dir.path().to_string_lossy().into_owned(),
            })
            .unwrap();

        engine
            .handle(Command::SetWorkspaceSettings {
                value: WorkspaceSettings {
                    extra_ignores: vec!["*.tmp".into()],
                    ..Default::default()
                },
            })
            .unwrap();

        let files: Vec<String> =
            serde_json::from_value(engine.handle(Command::ListFiles).unwrap()).unwrap();
        assert_eq!(files, vec!["a.txt".to_string()]);
    }

    #[test]
    fn closing_forgets_the_workspace_but_keeps_app_settings() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine();
        engine
            .handle(Command::SetAppSettings {
                value: AppSettings {
                    show_agent_context: false,
                    agent_roots: vec!["/tmp/roots".into()],
                    ..Default::default()
                },
            })
            .unwrap();
        engine
            .handle(Command::OpenWorkspace {
                path: dir.path().to_string_lossy().into_owned(),
            })
            .unwrap();

        engine.handle(Command::CloseWorkspace).unwrap();

        assert!(engine.handle(Command::CurrentWorkspace).unwrap().is_null());
        let app: AppSettings =
            serde_json::from_value(engine.handle(Command::GetAppSettings).unwrap()).unwrap();
        assert_eq!(app.agent_roots, vec!["/tmp/roots".to_string()]);
    }

    #[test]
    fn a_git_mutation_pushes_the_refreshed_status() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        drop(repo);
        let (engine, sink) = engine();
        engine
            .handle(Command::OpenWorkspace {
                path: dir.path().to_string_lossy().into_owned(),
            })
            .unwrap();

        let snapshot: GitStatusSnapshot =
            serde_json::from_value(engine.handle(Command::GitStageAll).unwrap()).unwrap();

        assert!(snapshot.files.iter().any(|f| f.path == "a.txt" && f.staged));
        assert_eq!(sink.git_statuses.lock().unwrap().len(), 1);
    }

    #[test]
    fn resolve_for_open_stays_inside_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        let (engine, _) = engine();
        engine
            .handle(Command::OpenWorkspace {
                path: dir.path().to_string_lossy().into_owned(),
            })
            .unwrap();

        let resolved: String = serde_json::from_value(
            engine
                .handle(Command::ResolveForOpen {
                    path: "a.txt".into(),
                })
                .unwrap(),
        )
        .unwrap();
        assert!(resolved.ends_with("/a.txt"), "{resolved}");

        assert!(engine
            .handle(Command::ResolveForOpen {
                path: "../escape".into()
            })
            .is_err());
    }

    #[test]
    fn opening_a_workspace_with_no_agent_roots_does_not_error_or_emit_agent_events() {
        // The common case: no Claude, no Grok, just a directory. The poller
        // must idle quietly — no error, no event storm — so the app looks
        // exactly like phase 1.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        let (engine, sink) = engine();

        engine
            .handle(Command::OpenWorkspace {
                path: dir.path().to_string_lossy().into_owned(),
            })
            .unwrap();

        // Give the poller a chance to tick once; with no agent roots it
        // discovers nothing and emits nothing.
        thread::sleep(Duration::from_millis(1_200));
        assert!(
            sink.agent_events.lock().unwrap().is_empty(),
            "no agent present → no agent events"
        );
    }

    #[test]
    fn closing_a_workspace_stops_the_poller_via_the_generation_counter() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine();

        engine
            .handle(Command::OpenWorkspace {
                path: dir.path().to_string_lossy().into_owned(),
            })
            .unwrap();
        let after_open = engine.poller_generation();
        assert!(
            after_open > 0,
            "open must start a poller and bump generation"
        );

        engine.handle(Command::CloseWorkspace).unwrap();
        let after_close = engine.poller_generation();
        assert!(
            after_close > after_open,
            "close must bump generation so the poller thread exits \
             (open={after_open}, close={after_close})"
        );

        // Reopen must bump again — two pollers must never share a generation.
        engine
            .handle(Command::OpenWorkspace {
                path: dir.path().to_string_lossy().into_owned(),
            })
            .unwrap();
        assert!(engine.poller_generation() > after_close);
        engine.handle(Command::CloseWorkspace).unwrap();
    }

    #[test]
    fn discovery_of_a_live_session_emits_exactly_one_session_started() {
        let workspace = tempfile::tempdir().unwrap();
        let agent_root = tempfile::tempdir().unwrap();
        let _ = write_live_grok_session(agent_root.path(), workspace.path(), "sess-appear");

        let (engine, sink) = engine();
        engine
            .handle(Command::SetAppSettings {
                value: AppSettings {
                    agent_roots: vec![agent_root.path().to_string_lossy().into_owned()],
                    ..Default::default()
                },
            })
            .unwrap();
        engine
            .handle(Command::OpenWorkspace {
                path: workspace.path().to_string_lossy().into_owned(),
            })
            .unwrap();

        let is_appear_start = |e: &AgentEvent| {
            matches!(
                e,
                AgentEvent::SessionStarted { session_id, .. }
                    if session_id == "sess-appear"
            )
        };
        let events = wait_for(&sink, |ev| ev.iter().any(is_appear_start));
        let starts: Vec<_> = events.iter().filter(|e| is_appear_start(e)).collect();
        assert_eq!(
            starts.len(),
            1,
            "exactly one SessionStarted on first sight: {events:?}"
        );

        // SessionStarted carries no activity and the store assumes `working`,
        // so the announcement has to be followed by what discovery actually
        // found — otherwise a quiet idle session renders as working forever.
        assert!(
            events.iter().any(|e| matches!(
                e,
                AgentEvent::ActivityChanged {
                    session_id,
                    activity: AgentActivity::Idle,
                    ..
                } if session_id == "sess-appear"
            )),
            "a new session must announce its real activity: {events:?}"
        );

        // A second tick must not re-announce the same live session.
        thread::sleep(Duration::from_millis(1_200));
        let events = sink.agent_events.lock().unwrap().clone();
        let starts: Vec<_> = events.iter().filter(|e| is_appear_start(e)).collect();
        assert_eq!(
            starts.len(),
            1,
            "no second SessionStarted on the next tick: {events:?}"
        );

        engine.handle(Command::CloseWorkspace).unwrap();
    }

    #[test]
    fn session_turning_stale_emits_exactly_one_session_ended() {
        let workspace = tempfile::tempdir().unwrap();
        let agent_root = tempfile::tempdir().unwrap();
        let events_file = write_live_grok_session(agent_root.path(), workspace.path(), "sess-end");

        let (engine, sink) = engine();
        engine
            .handle(Command::SetAppSettings {
                value: AppSettings {
                    agent_roots: vec![agent_root.path().to_string_lossy().into_owned()],
                    ..Default::default()
                },
            })
            .unwrap();
        engine
            .handle(Command::OpenWorkspace {
                path: workspace.path().to_string_lossy().into_owned(),
            })
            .unwrap();

        let is_end_start = |e: &AgentEvent| {
            matches!(
                e,
                AgentEvent::SessionStarted { session_id, .. } if session_id == "sess-end"
            )
        };
        let is_end_ended = |e: &AgentEvent| {
            matches!(
                e,
                AgentEvent::SessionEnded { session_id, .. } if session_id == "sess-end"
            )
        };
        let events = wait_for(&sink, |ev| ev.iter().any(is_end_start));
        assert!(
            events.iter().any(is_end_start),
            "session must appear first: {events:?}"
        );

        // Age the session past Grok's freshness window so discover marks it
        // Stale — that is the same path as "disappeared" for lifecycle.
        // Bare epoch-second ts → normalize_ts scales to ms far in the past.
        std::fs::write(
            &events_file,
            r#"{"type":"phase_changed","ts":1,"phase":"streaming_text"}
"#,
        )
        .unwrap();

        let events = wait_for(&sink, |ev| ev.iter().any(is_end_ended));
        let ends: Vec<_> = events.iter().filter(|e| is_end_ended(e)).collect();
        assert_eq!(
            ends.len(),
            1,
            "exactly one SessionEnded when session goes stale: {events:?}"
        );

        engine.handle(Command::CloseWorkspace).unwrap();
    }
}
