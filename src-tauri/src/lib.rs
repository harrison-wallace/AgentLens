//! The desktop app: Tauri commands, persistence, and choosing a backend.
//!
//! Every command here is a thin wrapper that hands a `Command` to whichever
//! backend is connected and passes the JSON back to the webview. What the
//! *app* still owns is the part a machine holding files has no business
//! owning: the settings store, the recent-workspaces list, which connection is
//! in use, and handing a file to another application.

mod backend;
mod events;
mod remote;
mod settings;
mod workspace;

use std::sync::Arc;

use agentlens_core::protocol::{
    self, AppSettings, Command, CommandResult, ConnectionInfo, ConnectionTarget, SessionRef,
    WorkspaceInfo, WorkspaceSettings, EVENT_CONNECTION,
};
use backend::child::ChildProcess;
use backend::{Backend, BackendState, InProcess};
use events::TauriEvents;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

/// Run `command` on the connected backend.
fn send(state: &State<BackendState>, command: Command) -> CommandResult<Value> {
    state.current()?.send(command)
}

/// Run `command` and read the reply as `T`, for the few places the app needs
/// the value rather than just forwarding it.
fn ask<T: serde::de::DeserializeOwned>(
    backend: &Arc<dyn Backend>,
    command: Command,
) -> CommandResult<T> {
    serde_json::from_value(backend.send(command)?)
        .map_err(|e| format!("unexpected reply from the backend: {e}"))
}

/// How the open workspace is written down in the store: bare path when local,
/// scheme-qualified when not. Two SSH hosts with the same directory layout
/// must not share one settings entry.
fn location_of(backend: &Arc<dyn Backend>) -> CommandResult<String> {
    let info: Option<WorkspaceInfo> = ask(backend, Command::CurrentWorkspace)?;
    let info = info.ok_or("no workspace is open")?;
    Ok(remote::format_location(&backend.info().target, &info.root))
}

#[tauri::command]
fn get_app_info() -> protocol::AppInfo {
    protocol::AppInfo {
        name: "AgentLens".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

/// Open a workspace, connecting first if the location names another machine.
///
/// Two commands rather than one, because the backend canonicalizes the root
/// and only then can the app look up the settings persisted against it — a
/// daemon has no store to look in.
#[tauri::command]
fn open_workspace(
    path: String,
    state: State<BackendState>,
    app: AppHandle,
) -> CommandResult<WorkspaceInfo> {
    let (target, path) = remote::parse_location(&path);
    if target != state.current()?.info().target {
        connect_to(target, &state, &app)?;
    }

    let backend = state.current()?;
    let info: WorkspaceInfo = ask(&backend, Command::OpenWorkspace { path })?;
    let location = remote::format_location(&backend.info().target, &info.root);

    // Persisting the recent list is best-effort: a read-only or unwritable
    // config dir must not stop the user opening a workspace.
    if let Err(err) = workspace::record_recent(&app, &location) {
        eprintln!("agentlens: {err}");
    }

    // Settings second, and they are what starts the watcher — the tree, the
    // file index and the feed all filter through them, so nothing may read
    // before they are in effect.
    backend.send(Command::SetWorkspaceSettings {
        value: settings::load(&app, &location),
    })?;
    Ok(info)
}

#[tauri::command]
fn close_workspace(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::CloseWorkspace)
}

#[tauri::command]
fn current_workspace(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::CurrentWorkspace)
}

#[tauri::command]
fn watcher_status(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GetWatcherStatus)
}

#[tauri::command]
fn list_dir(path: String, state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::ListDir { path })
}

#[tauri::command]
fn list_files(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::ListFiles)
}

#[tauri::command]
fn git_status(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitStatus)
}

#[tauri::command]
fn git_capabilities(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitCapabilities)
}

#[tauri::command]
fn git_stage(paths: Vec<String>, state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitStage { paths })
}

#[tauri::command]
fn git_stage_all(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitStageAll)
}

#[tauri::command]
fn git_unstage(paths: Vec<String>, state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitUnstage { paths })
}

#[tauri::command]
fn git_unstage_all(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitUnstageAll)
}

#[tauri::command]
fn git_commit(message: String, amend: bool, state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitCommit { message, amend })
}

#[tauri::command]
fn git_branches(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitBranches)
}

#[tauri::command]
fn git_switch_branch(name: String, state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitSwitchBranch { name })
}

#[tauri::command]
fn git_create_branch(name: String, state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitCreateBranch { name })
}

#[tauri::command]
fn git_stash_push(message: Option<String>, state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitStashPush { message })
}

#[tauri::command]
fn git_stash_pop(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitStashPop)
}

#[tauri::command]
fn read_preview(path: String, state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::ReadPreview { path })
}

/// Hand the file to the OS default application.
///
/// The backend resolves the path (and refuses anything outside the
/// workspace); expressing it in terms *this* machine can open is local work,
/// and for an SSH host there is no such expression — the user is told that
/// rather than watching nothing happen.
#[tauri::command]
fn open_externally(path: String, state: State<BackendState>, app: AppHandle) -> CommandResult<()> {
    let backend = state.current()?;
    let resolved: String = ask(&backend, Command::ResolveForOpen { path })?;
    let target = remote::to_local_path(&backend.info().target, &resolved)?;
    app.opener()
        .open_path(target, None::<&str>)
        .map_err(|e| format!("failed to open file: {e}"))
}

#[tauri::command]
fn session_diff(path: String, state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::SessionDiff { path })
}

#[tauri::command]
fn restart_session(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::RestartSession)
}

#[tauri::command]
fn workspace_settings(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GetWorkspaceSettings)
}

#[tauri::command]
fn set_workspace_settings(
    value: WorkspaceSettings,
    state: State<BackendState>,
    app: AppHandle,
) -> CommandResult<Value> {
    let backend = state.current()?;
    settings::save(&app, &location_of(&backend)?, &value)?;
    backend.send(Command::SetWorkspaceSettings { value })
}

#[tauri::command]
fn app_settings(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GetAppSettings)
}

/// App-level settings outlive the workspace, so unlike `set_workspace_settings`
/// this doesn't need one open. They are stored here and pushed to the backend,
/// which is also what a fresh daemon is told first when a connection is made.
#[tauri::command]
fn set_app_settings(
    value: AppSettings,
    state: State<BackendState>,
    app: AppHandle,
) -> CommandResult<Value> {
    settings::save_app(&app, &value)?;
    send(&state, Command::SetAppSettings { value })
}

#[tauri::command]
fn pinned_entries(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::PinnedEntries)
}

/// Agent sessions found for the open workspace, most recently active first.
/// An empty list is the normal answer when no agent is running — never an
/// error, since most workspaces have no session at all.
#[tauri::command]
fn agent_sessions(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::AgentSessions)
}

/// Where the backend is looking for agent sessions, and where each entry came
/// from. Surfaced in settings so "no agent detected" is diagnosable instead of
/// a dead end — and, on a remote connection, so it is obvious that the roots
/// being searched are the *remote* machine's.
#[tauri::command]
fn agent_roots(state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::AgentRoots)
}

/// Records appended to `session` since the last call. The backend keeps the
/// read offset, so this returns only what is new — and nothing at all the
/// first time, since a workspace opened mid-task must not replay history into
/// the feed.
#[tauri::command]
fn agent_events(session: SessionRef, state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::AgentEvents { session })
}

#[tauri::command]
fn recent_workspaces(app: AppHandle) -> CommandResult<Vec<String>> {
    workspace::recent_workspaces(&app)
}

/// The WSL distros installed on this machine, for the "Open in WSL…" picker.
/// Empty everywhere except a Windows box with WSL, which is not an error.
#[tauri::command]
fn wsl_distros() -> Vec<String> {
    remote::wsl_distros()
}

#[tauri::command]
fn connection(state: State<BackendState>) -> CommandResult<ConnectionInfo> {
    Ok(state.current()?.info())
}

/// Back to observing this machine.
///
/// There is no matching `connect` command: `open_workspace` connects wherever
/// the location it is given points, which is the only time pointing the app at
/// another machine is useful. Coming back has no location to hang off, so it
/// gets one of its own.
#[tauri::command]
fn disconnect(state: State<BackendState>, app: AppHandle) -> CommandResult<ConnectionInfo> {
    connect_to(ConnectionTarget::Local, &state, &app)
}

/// Build a backend for `target`, install it, and give it the app-level
/// settings it has no way to load for itself.
fn connect_to(
    target: ConnectionTarget,
    state: &State<BackendState>,
    app: &AppHandle,
) -> CommandResult<ConnectionInfo> {
    let events = Arc::new(TauriEvents(app.clone()));
    let stored = settings::load_app(app);

    let backend: Arc<dyn Backend> = match &target {
        ConnectionTarget::Local => Arc::new(InProcess::new(events)),
        _ => Arc::new(ChildProcess::connect(
            target.clone(),
            stored.daemon_command.clone(),
            events,
        )?),
    };

    let info = backend.info();
    backend.send(Command::SetAppSettings { value: stored })?;
    state.replace(backend)?;
    let _ = app.emit(EVENT_CONNECTION, &info);
    Ok(info)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            open_workspace,
            close_workspace,
            current_workspace,
            list_dir,
            list_files,
            git_status,
            git_capabilities,
            git_stage,
            git_stage_all,
            git_unstage,
            git_unstage_all,
            git_commit,
            git_branches,
            git_switch_branch,
            git_create_branch,
            git_stash_push,
            git_stash_pop,
            read_preview,
            open_externally,
            session_diff,
            restart_session,
            workspace_settings,
            set_workspace_settings,
            app_settings,
            set_app_settings,
            pinned_entries,
            agent_sessions,
            agent_events,
            agent_roots,
            recent_workspaces,
            watcher_status,
            connection,
            disconnect,
            wsl_distros,
        ])
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .expect("main window must exist");
            window
                .set_title(&format!("AgentLens v{}", env!("CARGO_PKG_VERSION")))
                .expect("failed to set window title");

            // The app starts local. App-level settings outlive any workspace,
            // so they load once here rather than on open; an unreadable store
            // means defaults, not a refusal to start.
            let events = Arc::new(TauriEvents(app.handle().clone()));
            let local: Arc<dyn Backend> = Arc::new(InProcess::new(events));
            local
                .send(Command::SetAppSettings {
                    value: settings::load_app(app.handle()),
                })
                .map_err(std::io::Error::other)?;
            app.manage(BackendState::new(local));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            // A remote connection is a child process holding an OS watch and,
            // for SSH, a network session. Closing the window has to take them
            // with it — otherwise `ssh` outlives the app that spawned it.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app.try_state::<BackendState>() {
                    if let Ok(backend) = state.current() {
                        backend.shutdown();
                    }
                }
            }
        });
}
