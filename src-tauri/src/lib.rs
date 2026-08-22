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
    UpdateCheck, WorkspaceInfo, WorkspaceSettings, EVENT_CONNECTION,
};
use backend::child::ChildProcess;
use backend::{Backend, BackendState, InProcess};
use events::TauriEvents;
use serde_json::Value;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
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

/// Notify-only GitHub release check. Every failure is a quiet non-event —
/// offline, rate-limited, or unparseable all look the same as "already current".
#[tauri::command]
fn check_for_update() -> UpdateCheck {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let quiet = || UpdateCheck {
        current: current.clone(),
        latest: None,
        url: None,
        newer: false,
    };

    let mut response =
        match ureq::get("https://api.github.com/repos/harrison-wallace/AgentLens/releases/latest")
            .header("User-Agent", &format!("AgentLens/{current}"))
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(std::time::Duration::from_secs(5)))
            .build()
            .call()
        {
            Ok(r) => r,
            Err(_) => return quiet(),
        };

    if response.status() != 200 {
        return quiet();
    }

    let body: serde_json::Value = match response.body_mut().read_json() {
        Ok(v) => v,
        Err(_) => return quiet(),
    };

    // Drafts and prereleases are not something to nag about.
    if body.get("draft").and_then(|v| v.as_bool()) == Some(true)
        || body.get("prerelease").and_then(|v| v.as_bool()) == Some(true)
    {
        return quiet();
    }

    let tag = match body.get("tag_name").and_then(|v| v.as_str()) {
        Some(t) => t.trim().trim_start_matches('v').to_string(),
        None => return quiet(),
    };
    if tag.is_empty() {
        return quiet();
    }

    let url = body
        .get("html_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let newer = is_newer(&current, &tag);
    UpdateCheck {
        current,
        latest: Some(tag),
        url,
        newer,
    }
}

/// Numeric `x.y.z` comparison after stripping a leading `v`. Anything that
/// does not parse three components is not considered newer.
fn is_newer(current: &str, latest: &str) -> bool {
    fn parts(v: &str) -> Option<[u64; 3]> {
        let trimmed = v.trim().trim_start_matches('v');
        let mut nums = [0u64; 3];
        let segs: Vec<&str> = trimmed.split('.').collect();
        if segs.len() != 3 {
            return None;
        }
        for (i, seg) in segs.iter().enumerate() {
            nums[i] = seg.parse().ok()?;
        }
        Some(nums)
    }
    match (parts(current), parts(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

#[cfg(test)]
mod update_check_tests {
    use super::is_newer;

    #[test]
    fn newer_minor() {
        assert!(is_newer("0.5.1", "0.6.0"));
    }

    #[test]
    fn same_version_is_not_newer() {
        assert!(!is_newer("0.5.1", "0.5.1"));
    }

    #[test]
    fn older_is_not_newer_numerically() {
        // String compare would call "0.5.9" > "0.6.0" — numeric must not.
        assert!(!is_newer("0.6.0", "0.5.9"));
    }

    #[test]
    fn garbage_tag_is_not_newer() {
        assert!(!is_newer("0.5.1", "not-a-version"));
        assert!(!is_newer("0.5.1", "v1.2"));
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

/// Directories on the connected machine, for choosing a workspace on it.
///
/// The only listing that needs no workspace open, because it is what happens
/// before there is one — and the only reason the app can offer a folder picker
/// for a machine it isn't sitting at.
#[tauri::command]
fn browse_dir(path: Option<String>, state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::BrowseDir { path })
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
fn git_diff(path: String, staged: bool, state: State<BackendState>) -> CommandResult<Value> {
    send(&state, Command::GitDiff { path, staged })
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
    let target = state.current()?.info().target;
    settings::save_app_for(&app, &target, &value)?;
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

/// Update the tray tooltip. Errors (no tray, Linux) are ignored.
#[tauri::command]
fn set_tray_status(app: AppHandle, text: String) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(text.as_str()));
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn install_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show AgentLens", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("AgentLens")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    // Keep the handle so drop does not remove the icon.
    let tray = builder.build(app)?;
    app.manage(tray);
    Ok(())
}

/// Point the app at another machine without opening anything yet.
///
/// Needed because the folder picker has to *browse* that machine before a
/// workspace on it can be chosen. `open_workspace` still connects on its own
/// when handed a location, so typing or clicking a recent entry skips this.
#[tauri::command]
fn connect(
    target: ConnectionTarget,
    state: State<BackendState>,
    app: AppHandle,
) -> CommandResult<ConnectionInfo> {
    connect_to(target, &state, &app)
}

/// Back to observing this machine.
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
    let stored = settings::load_app_for(app, &target);

    let backend: Arc<dyn Backend> = match &target {
        ConnectionTarget::Local => Arc::new(InProcess::new(events)),
        _ => Arc::new(ChildProcess::connect(
            target.clone(),
            stored.daemon_command.clone(),
            stored.auto_install_daemon,
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
        // Window geometry first so the saved size/position lands before show.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            check_for_update,
            open_workspace,
            close_workspace,
            current_workspace,
            list_dir,
            list_files,
            browse_dir,
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
            git_diff,
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
            connect,
            disconnect,
            wsl_distros,
            set_tray_status,
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
                    value: settings::load_app_for(app.handle(), &ConnectionTarget::Local),
                })
                .map_err(std::io::Error::other)?;
            app.manage(BackendState::new(local));
            install_tray(app)?;
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
