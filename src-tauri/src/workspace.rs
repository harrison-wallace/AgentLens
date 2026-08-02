//! The recent-workspaces list.
//!
//! Which workspace is *open* is the backend's business (see
//! `agentlens_core::workspace`). Which ones were opened *recently* is a fact
//! about the person sitting in front of the app, so it stays here — and
//! entries are locations, not paths, so a recent SSH workspace reopens on the
//! host it came from.

use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const RECENT_STORE_FILE: &str = "settings.json";
const RECENT_KEY: &str = "recentWorkspaces";
const MAX_RECENT: usize = 10;

/// Record `location` as the most-recently opened workspace, deduplicated and
/// capped at `MAX_RECENT`.
pub fn record_recent(app: &AppHandle, location: &str) -> Result<(), String> {
    let store = app
        .store(RECENT_STORE_FILE)
        .map_err(|e| format!("failed to open settings store: {e}"))?;

    let mut recent = read_recent_list(store.get(RECENT_KEY));
    recent.retain(|entry| entry != location);
    recent.insert(0, location.to_string());
    recent.truncate(MAX_RECENT);

    store.set(RECENT_KEY, json!(recent));
    store
        .save()
        .map_err(|e| format!("failed to save settings store: {e}"))?;

    Ok(())
}

/// Return the persisted list of recent workspace locations, most-recent-first.
pub fn recent_workspaces(app: &AppHandle) -> Result<Vec<String>, String> {
    let store = app
        .store(RECENT_STORE_FILE)
        .map_err(|e| format!("failed to open settings store: {e}"))?;
    Ok(read_recent_list(store.get(RECENT_KEY)))
}

fn read_recent_list(value: Option<serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
        .unwrap_or_default()
}
