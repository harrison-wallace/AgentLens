mod protocol;

use protocol::AppInfo;
use tauri::Manager;

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        name: "AgentLens".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_app_info])
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
