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

use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;

use crate::agents::claude::ClaudeCode;
use crate::agents::{self, AgentProvider};
use crate::protocol::{AgentPoll, Command, CommandResult, Hello, WorkspaceInfo, PROTOCOL_VERSION};
use crate::settings::{self, SettingsState};
use crate::snapshots::{self, SessionState};
use crate::watcher::{self, EventSink, WatcherManager};
use crate::workspace::{self, WorkspaceState};
use crate::{browse, gitops, gitstatus, preview, tree};

/// The observing half of AgentLens, driven by commands.
pub struct Engine {
    workspace: WorkspaceState,
    watcher: Arc<WatcherManager>,
    settings: SettingsState,
    session: SessionState,
    /// The agent providers own their read offsets, so they outlive a single
    /// command — a fresh provider each poll would re-tail from the end and
    /// never report anything. They must *not* outlive the workspace, hence
    /// the reset on open and close.
    agents: Mutex<ClaudeCode>,
    sink: Arc<dyn EventSink>,
}

impl Engine {
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        Engine {
            workspace: WorkspaceState::default(),
            watcher: Arc::new(WatcherManager::default()),
            settings: SettingsState::default(),
            session: SessionState::default(),
            agents: Mutex::new(ClaudeCode::new()),
            sink,
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
                    capabilities: Vec::new(),
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
                ok(ClaudeCode::new().discover(&ws.root, &roots))
            }
            Command::AgentRoots => ok(agents::describe_roots(
                &settings::current_app(&self.settings)?.agent_roots,
            )),
            Command::AgentEvents { session } => {
                let ws = workspace::current(&self.workspace)?;
                let roots =
                    agents::resolve_roots(&settings::current_app(&self.settings)?.agent_roots);
                let mut provider = self.agents.lock().map_err(|_| "agent state poisoned")?;
                let events = provider.poll(&ws.root, &session, &roots);
                ok(AgentPoll {
                    events,
                    records: provider.stats.records,
                    skipped: provider.stats.skipped,
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

    fn open_workspace(&self, path: &str) -> CommandResult<Value> {
        let opened = workspace::open(&self.workspace, Path::new(path))?;
        snapshots::restart(&self.session, &opened.root)?;
        // Read offsets and parse tallies belong to the workspace, not the
        // process: without this, reopening resumes from the offset it had on
        // close and the "couldn't parse N records" counter reports another
        // workspace's totals.
        self.reset_agents()?;
        // The watcher deliberately does not start here. It filters through
        // settings persisted against the *canonical* root, which the caller
        // only learns from this reply — so it follows with
        // `SetWorkspaceSettings`, and that starts the watch.
        ok(opened.info())
    }

    fn close_workspace(&self) -> CommandResult<Value> {
        watcher::stop(&self.sink, &self.watcher);
        snapshots::clear(&self.session)?;
        settings::deactivate(&self.settings)?;
        self.reset_agents()?;
        workspace::close(&self.workspace)?;
        ok(())
    }

    fn reset_agents(&self) -> CommandResult<()> {
        let mut guard = self.agents.lock().map_err(|_| "agent state poisoned")?;
        *guard = ClaudeCode::new();
        Ok(())
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
        AppSettings, FsEvent, GitStatusSnapshot, WatcherStatus, WorkspaceSettings,
    };

    /// Records what the engine pushes, so tests can assert on emissions
    /// without a window or a pipe.
    #[derive(Default)]
    struct RecordingSink {
        git_statuses: Mutex<Vec<GitStatusSnapshot>>,
    }

    impl EventSink for RecordingSink {
        fn fs_changes(&self, _events: &[FsEvent]) {}
        fn git_status(&self, snapshot: &GitStatusSnapshot) {
            self.git_statuses.lock().unwrap().push(snapshot.clone());
        }
        fn watcher_status(&self, _status: &WatcherStatus) {}
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
}
