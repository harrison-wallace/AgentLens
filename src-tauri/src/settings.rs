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

use std::collections::BTreeMap;

use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use agentlens_core::protocol::{AppSettings, ConnectionTarget, WorkspaceSettings};

const SETTINGS_STORE_FILE: &str = "settings.json";
/// Object keyed by workspace location, so settings follow the workspace
/// rather than the app.
const WORKSPACE_SETTINGS_KEY: &str = "workspaceSettings";
/// The app-level scope — one object, not keyed by anything.
const APP_SETTINGS_KEY: &str = "appSettings";
/// Extra agent-session folders keyed by [`ConnectionTarget::host_key`].
/// Separate from `appSettings` so a local path is not pushed to a remote
/// daemon (and the other way around).
const AGENT_ROOTS_BY_HOST_KEY: &str = "agentRootsByHost";

/// App-level settings with `agent_roots` filled for `target` only.
///
/// Extra session folders are stored per host. The daemon sees a flat list
/// because it only ever searches the machine it is running on.
pub fn load_app_for(app: &AppHandle, target: &ConnectionTarget) -> AppSettings {
    let mut settings = raw_app(app);
    let by_host = load_roots_by_host(app);
    settings.agent_roots = roots_for_host(&by_host, &target.host_key(), &settings.agent_roots);
    settings
}

fn raw_app(app: &AppHandle) -> AppSettings {
    let Ok(store) = app.store(SETTINGS_STORE_FILE) else {
        return AppSettings::default();
    };
    store
        .get(APP_SETTINGS_KEY)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn load_roots_by_host(app: &AppHandle) -> BTreeMap<String, Vec<String>> {
    let Ok(store) = app.store(SETTINGS_STORE_FILE) else {
        return BTreeMap::new();
    };
    store
        .get(AGENT_ROOTS_BY_HOST_KEY)
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

/// Persist app-level settings and record `value.agent_roots` under `target`.
///
/// Other hosts' extra folders stay in the map. Older stores that only had
/// a single `agentRoots` list are treated as this machine (`local`).
pub fn save_app_for(
    app: &AppHandle,
    target: &ConnectionTarget,
    value: &AppSettings,
) -> Result<(), String> {
    let store = app
        .store(SETTINGS_STORE_FILE)
        .map_err(|e| format!("failed to open settings store: {e}"))?;

    let stored = raw_app(app);
    let mut by_host = load_roots_by_host(app);
    store_roots_for_host(
        &mut by_host,
        target.host_key(),
        value.agent_roots.clone(),
        &stored.agent_roots,
    );

    store.set(APP_SETTINGS_KEY, json!(value));
    store.set(AGENT_ROOTS_BY_HOST_KEY, json!(by_host));
    store
        .save()
        .map_err(|e| format!("failed to save settings store: {e}"))
}

/// Extra folders for `host`, falling back to a pre-map `legacy` list only
/// for this machine so an existing setting is not dropped on upgrade.
pub fn roots_for_host(
    by_host: &BTreeMap<String, Vec<String>>,
    host: &str,
    legacy: &[String],
) -> Vec<String> {
    if let Some(roots) = by_host.get(host) {
        return roots.clone();
    }
    if by_host.is_empty() && host == "local" {
        return legacy.to_vec();
    }
    Vec::new()
}

/// Write `roots` for `host`. If the map is empty and a legacy list exists,
/// keep that list on `local` first so a first remote save does not lose it.
pub fn store_roots_for_host(
    by_host: &mut BTreeMap<String, Vec<String>>,
    host: String,
    roots: Vec<String>,
    legacy: &[String],
) {
    if by_host.is_empty() && !legacy.is_empty() && host != "local" {
        by_host.insert("local".to_string(), legacy.to_vec());
    }
    by_host.insert(host, roots);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_roots_apply_only_to_this_machine() {
        let legacy = vec!["/home/h/.claude".to_string()];
        let empty = BTreeMap::new();
        assert_eq!(roots_for_host(&empty, "local", &legacy), legacy);
        assert!(roots_for_host(&empty, "ssh:box", &legacy).is_empty());
    }

    #[test]
    fn a_host_reads_only_its_own_list() {
        let mut by_host = BTreeMap::new();
        by_host.insert("local".into(), vec!["/local".into()]);
        by_host.insert("ssh:box".into(), vec!["/remote".into()]);
        assert_eq!(
            roots_for_host(&by_host, "local", &["/legacy".into()]),
            vec!["/local".to_string()]
        );
        assert_eq!(
            roots_for_host(&by_host, "ssh:box", &[]),
            vec!["/remote".to_string()]
        );
        assert!(roots_for_host(&by_host, "ssh:other", &[]).is_empty());
    }

    #[test]
    fn first_remote_save_keeps_the_legacy_local_list() {
        let mut by_host = BTreeMap::new();
        store_roots_for_host(
            &mut by_host,
            "ssh:box".into(),
            vec!["/remote".into()],
            &["/legacy".into()],
        );
        assert_eq!(by_host.get("local"), Some(&vec!["/legacy".to_string()]));
        assert_eq!(by_host.get("ssh:box"), Some(&vec!["/remote".to_string()]));
    }

    #[test]
    fn local_save_does_not_invent_a_second_copy() {
        let mut by_host = BTreeMap::new();
        store_roots_for_host(
            &mut by_host,
            "local".into(),
            vec!["/new".into()],
            &["/legacy".into()],
        );
        assert_eq!(by_host.len(), 1);
        assert_eq!(by_host.get("local"), Some(&vec!["/new".to_string()]));
    }
}
