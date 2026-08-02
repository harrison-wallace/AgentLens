//! Settings persistence, in two scopes.
//!
//! *Per workspace*, keyed by location (see `remote::format_location`): extra
//! ignore globs and pinned paths. Keying by location rather than by path is
//! what keeps `/home/h/proj` on two different SSH hosts from sharing one
//! entry.
//!
//! *Per app*: settings that describe how you work rather than what one repo
//! contains. Both scopes live in the same store file under separate keys.
//!
//! Only storage lives here. The settings *in effect*, and the matcher compiled
//! from them, belong to whichever backend is doing the observing — see
//! `agentlens_core::settings`. A daemon has no store of its own; it is told.

use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use agentlens_core::protocol::{AppSettings, WorkspaceSettings};

const SETTINGS_STORE_FILE: &str = "settings.json";
/// Object keyed by workspace location, so settings follow the workspace
/// rather than the app.
const WORKSPACE_SETTINGS_KEY: &str = "workspaceSettings";
/// The app-level scope — one object, not keyed by anything.
const APP_SETTINGS_KEY: &str = "appSettings";

/// Read the persisted app-level settings. Missing or unreadable means
/// defaults, which for this scope is not "everything off".
pub fn load_app(app: &AppHandle) -> AppSettings {
    let Ok(store) = app.store(SETTINGS_STORE_FILE) else {
        return AppSettings::default();
    };
    store
        .get(APP_SETTINGS_KEY)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

/// Persist the app-level settings.
pub fn save_app(app: &AppHandle, value: &AppSettings) -> Result<(), String> {
    let store = app
        .store(SETTINGS_STORE_FILE)
        .map_err(|e| format!("failed to open settings store: {e}"))?;
    store.set(APP_SETTINGS_KEY, json!(value));
    store
        .save()
        .map_err(|e| format!("failed to save settings store: {e}"))
}

/// Read the persisted settings for `location`.
pub fn load(app: &AppHandle, location: &str) -> WorkspaceSettings {
    let Ok(store) = app.store(SETTINGS_STORE_FILE) else {
        return WorkspaceSettings::default();
    };
    store
        .get(WORKSPACE_SETTINGS_KEY)
        .and_then(|value| {
            value
                .get(location)
                .and_then(|entry| serde_json::from_value(entry.clone()).ok())
        })
        .unwrap_or_default()
}

/// Persist `settings` for `location`, leaving other workspaces' entries alone.
pub fn save(app: &AppHandle, location: &str, settings: &WorkspaceSettings) -> Result<(), String> {
    let store = app
        .store(SETTINGS_STORE_FILE)
        .map_err(|e| format!("failed to open settings store: {e}"))?;

    let mut all = match store.get(WORKSPACE_SETTINGS_KEY) {
        Some(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    all.insert(location.to_string(), json!(settings));

    store.set(WORKSPACE_SETTINGS_KEY, serde_json::Value::Object(all));
    store
        .save()
        .map_err(|e| format!("failed to save settings store: {e}"))
}
