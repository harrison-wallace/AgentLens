mod gitstatus;
mod paths;
mod protocol;
mod tree;
mod workspace;

use protocol::{CommandResult, DirEntryNode, GitStatusSnapshot, WorkspaceInfo};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
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
    app: AppHandle,
) -> CommandResult<WorkspaceInfo> {
    let opened = workspace::open(&state, &PathBuf::from(path))?;
    // Persisting the recent list is best-effort: a read-only or unwritable
    // config dir must not stop the user opening a workspace.
    if let Err(err) = workspace::record_recent(&app, &opened.root) {
        eprintln!("agentlens: {err}");
    }
    Ok(to_workspace_info(&opened))
}

#[tauri::command]
fn close_workspace(state: State<WorkspaceState>) -> CommandResult<()> {
    workspace::close(&state)
}

#[tauri::command]
fn current_workspace(state: State<WorkspaceState>) -> CommandResult<Option<WorkspaceInfo>> {
    Ok(workspace::current_opt(&state)?.map(|w| to_workspace_info(&w)))
}

#[tauri::command]
fn list_dir(path: String, state: State<WorkspaceState>) -> CommandResult<Vec<DirEntryNode>> {
    let ws = workspace::current(&state)?;
    tree::list_dir(&ws.root, &path)
}

#[tauri::command]
fn git_status(state: State<WorkspaceState>) -> CommandResult<GitStatusSnapshot> {
    let ws = workspace::current(&state)?;
    gitstatus::status(&ws.root)
}

#[tauri::command]
fn recent_workspaces(app: AppHandle) -> CommandResult<Vec<String>> {
    workspace::recent_workspaces(&app)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(WorkspaceState::default())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            open_workspace,
            close_workspace,
            current_workspace,
            list_dir,
            git_status,
            recent_workspaces,
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
