mod gitstatus;
mod paths;
mod preview;
mod protocol;
mod settings;
mod snapshots;
mod tree;
mod watcher;
mod workspace;

use protocol::{
    CommandResult, DirEntryNode, GitStatusSnapshot, PreviewPayload, SessionDiff, WatcherStatus,
    WorkspaceInfo, WorkspaceSettings,
};
use settings::SettingsState;
use snapshots::SessionState;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;
use watcher::WatcherManager;
use workspace::WorkspaceState;

fn to_workspace_info(w: &workspace::Workspace) -> WorkspaceInfo {
    WorkspaceInfo {
        root: paths::normalize_absolute(&w.root),
        name: w.name.clone(),
        watching_since: w.watching_since,
    }
}

#[tauri::command]
fn get_app_info() -> protocol::AppInfo {
    protocol::AppInfo {
        name: "AgentLens".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

#[tauri::command]
fn open_workspace(
    path: String,
    state: State<WorkspaceState>,
    watcher_state: State<WatcherManager>,
    settings_state: State<SettingsState>,
    session_state: State<SessionState>,
    app: AppHandle,
) -> CommandResult<WorkspaceInfo> {
    let opened = workspace::open(&state, &PathBuf::from(path))?;
    // Persisting the recent list is best-effort: a read-only or unwritable
    // config dir must not stop the user opening a workspace.
    if let Err(err) = workspace::record_recent(&app, &opened.root) {
        eprintln!("agentlens: {err}");
    }

    // Settings first — the tree, the file index, and the watcher all filter
    // through them, so they have to be in effect before anything reads.
    let loaded = settings::load(&app, &opened.root);
    settings::activate(&settings_state, &opened.root, loaded)?;
    snapshots::restart(&session_state, &opened.root)?;
    watcher::start(
        &app,
        &watcher_state,
        &opened.root,
        settings::current_matcher(&settings_state),
    );
    Ok(to_workspace_info(&opened))
}

#[tauri::command]
fn close_workspace(
    state: State<WorkspaceState>,
    watcher_state: State<WatcherManager>,
    settings_state: State<SettingsState>,
    session_state: State<SessionState>,
    app: AppHandle,
) -> CommandResult<()> {
    watcher::stop(&app, &watcher_state);
    snapshots::clear(&session_state)?;
    settings::deactivate(&settings_state)?;
    workspace::close(&state)
}

#[tauri::command]
fn watcher_status(watcher_state: State<WatcherManager>) -> CommandResult<WatcherStatus> {
    Ok(watcher::status(&watcher_state))
}

#[tauri::command]
fn current_workspace(state: State<WorkspaceState>) -> CommandResult<Option<WorkspaceInfo>> {
    Ok(workspace::current_opt(&state)?.map(|w| to_workspace_info(&w)))
}

#[tauri::command]
fn list_dir(
    path: String,
    state: State<WorkspaceState>,
    settings_state: State<SettingsState>,
) -> CommandResult<Vec<DirEntryNode>> {
    let ws = workspace::current(&state)?;
    tree::list_dir(&ws.root, &path, &settings::current_matcher(&settings_state))
}

#[tauri::command]
fn list_files(
    state: State<WorkspaceState>,
    settings_state: State<SettingsState>,
) -> CommandResult<Vec<String>> {
    let ws = workspace::current(&state)?;
    Ok(tree::list_files(
        &ws.root,
        &settings::current_matcher(&settings_state),
    ))
}

#[tauri::command]
fn git_status(state: State<WorkspaceState>) -> CommandResult<GitStatusSnapshot> {
    let ws = workspace::current(&state)?;
    gitstatus::status(&ws.root)
}

#[tauri::command]
fn read_preview(path: String, state: State<WorkspaceState>) -> CommandResult<PreviewPayload> {
    let ws = workspace::current(&state)?;
    preview::read(&ws.root, &path)
}

#[tauri::command]
fn open_externally(
    path: String,
    state: State<WorkspaceState>,
    app: AppHandle,
) -> CommandResult<()> {
    let ws = workspace::current(&state)?;
    let target = preview::resolve_for_open(&ws.root, &path)?;
    app.opener()
        .open_path(target.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("failed to open file: {e}"))
}

#[tauri::command]
fn session_diff(
    path: String,
    state: State<WorkspaceState>,
    session_state: State<SessionState>,
) -> CommandResult<SessionDiff> {
    let ws = workspace::current(&state)?;
    snapshots::diff(&session_state, &ws.root, &path)
}

#[tauri::command]
fn restart_session(
    state: State<WorkspaceState>,
    session_state: State<SessionState>,
) -> CommandResult<WorkspaceInfo> {
    let ws = workspace::restart_session(&state)?;
    snapshots::restart(&session_state, &ws.root)?;
    Ok(to_workspace_info(&ws))
}

#[tauri::command]
fn workspace_settings(settings_state: State<SettingsState>) -> CommandResult<WorkspaceSettings> {
    settings::current(&settings_state)
}

#[tauri::command]
fn set_workspace_settings(
    value: WorkspaceSettings,
    state: State<WorkspaceState>,
    settings_state: State<SettingsState>,
    watcher_state: State<WatcherManager>,
    app: AppHandle,
) -> CommandResult<WorkspaceSettings> {
    let ws = workspace::current(&state)?;
    settings::save(&app, &ws.root, &value)?;
    settings::activate(&settings_state, &ws.root, value)?;
    // The watcher holds its matcher for the life of the watch, so new globs
    // only take effect once it's restarted.
    watcher::start(
        &app,
        &watcher_state,
        &ws.root,
        settings::current_matcher(&settings_state),
    );
    settings::current(&settings_state)
}

#[tauri::command]
fn recent_workspaces(app: AppHandle) -> CommandResult<Vec<String>> {
    workspace::recent_workspaces(&app)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(WorkspaceState::default())
        .manage(WatcherManager::default())
        .manage(SettingsState::default())
        .manage(SessionState::default())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            open_workspace,
            close_workspace,
            current_workspace,
            list_dir,
            list_files,
            git_status,
            read_preview,
            open_externally,
            session_diff,
            restart_session,
            workspace_settings,
            set_workspace_settings,
            recent_workspaces,
            watcher_status,
        ])
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .expect("main window must exist");
            window
                .set_title(&format!("AgentLens v{}", env!("CARGO_PKG_VERSION")))
                .expect("failed to set window title");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
