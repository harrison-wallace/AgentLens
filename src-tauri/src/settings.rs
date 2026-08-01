//! Per-workspace settings, persisted in the shared store.
//!
//! Today that's just extra ignore globs. They're gitignore syntax, compiled
//! into the same kind of matcher `.gitignore` produces, so the tree, the file
//! index, and the watcher can all apply them the same way.

use std::path::Path;
use std::sync::Mutex;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::paths::normalize_absolute;
use crate::protocol::WorkspaceSettings;

const SETTINGS_STORE_FILE: &str = "settings.json";
/// Object keyed by normalized workspace root, so settings follow the
/// workspace rather than the app.
const WORKSPACE_SETTINGS_KEY: &str = "workspaceSettings";

/// Tauri-managed state holding the open workspace's settings and the matcher
/// compiled from them. Kept in memory because `list_dir` consults it on every
/// call and re-reading the store each time would be silly.
#[derive(Default)]
pub struct SettingsState(pub Mutex<Active>);

/// The current settings alongside their compiled matcher.
pub struct Active {
    pub settings: WorkspaceSettings,
    pub matcher: Gitignore,
}

impl Default for Active {
    fn default() -> Self {
        Active {
            settings: WorkspaceSettings::default(),
            matcher: Gitignore::empty(),
        }
    }
}

/// Compile `settings` into a matcher rooted at `root`. Invalid globs are
/// skipped rather than failing the whole set — a typo in one line shouldn't
/// silently disable the others.
pub fn build_matcher(root: &Path, settings: &WorkspaceSettings) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    for glob in &settings.extra_ignores {
        let trimmed = glob.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let _ = builder.add_line(None, trimmed);
    }
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// True if `relative` (workspace-relative, forward slashes) is covered by the
/// extra globs.
pub fn is_extra_ignored(matcher: &Gitignore, relative: &str, is_dir: bool) -> bool {
    if relative.is_empty() {
        return false;
    }
    matcher
        .matched_path_or_any_parents(relative, is_dir)
        .is_ignore()
}

/// Read the persisted settings for `root`.
pub fn load(app: &AppHandle, root: &Path) -> WorkspaceSettings {
    let Ok(store) = app.store(SETTINGS_STORE_FILE) else {
        return WorkspaceSettings::default();
    };
    store
        .get(WORKSPACE_SETTINGS_KEY)
        .and_then(|value| {
            value
                .get(normalize_absolute(root))
                .and_then(|entry| serde_json::from_value(entry.clone()).ok())
        })
        .unwrap_or_default()
}

/// Persist `settings` for `root`, leaving other workspaces' entries alone.
pub fn save(app: &AppHandle, root: &Path, settings: &WorkspaceSettings) -> Result<(), String> {
    let store = app
        .store(SETTINGS_STORE_FILE)
        .map_err(|e| format!("failed to open settings store: {e}"))?;

    let mut all = match store.get(WORKSPACE_SETTINGS_KEY) {
        Some(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    all.insert(normalize_absolute(root), json!(settings));

    store.set(WORKSPACE_SETTINGS_KEY, serde_json::Value::Object(all));
    store
        .save()
        .map_err(|e| format!("failed to save settings store: {e}"))?;
    Ok(())
}

/// Replace the in-memory settings and matcher for `root`.
pub fn activate(
    state: &SettingsState,
    root: &Path,
    settings: WorkspaceSettings,
) -> Result<(), String> {
    let matcher = build_matcher(root, &settings);
    let mut guard = state.0.lock().map_err(|_| "settings state poisoned")?;
    *guard = Active { settings, matcher };
    Ok(())
}

/// Reset to defaults (workspace closed).
pub fn deactivate(state: &SettingsState) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|_| "settings state poisoned")?;
    *guard = Active::default();
    Ok(())
}

/// The settings currently in effect.
pub fn current(state: &SettingsState) -> Result<WorkspaceSettings, String> {
    let guard = state.0.lock().map_err(|_| "settings state poisoned")?;
    Ok(guard.settings.clone())
}

/// A clone of the matcher currently in effect, for callers that need to hold
/// it without keeping the lock (the watcher keeps one for its whole run).
pub fn current_matcher(state: &SettingsState) -> Gitignore {
    state
        .0
        .lock()
        .map(|guard| guard.matcher.clone())
        .unwrap_or_else(|_| Gitignore::empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(globs: &[&str]) -> WorkspaceSettings {
        WorkspaceSettings {
            extra_ignores: globs.iter().map(|g| g.to_string()).collect(),
        }
    }

    #[test]
    fn matches_extra_globs_including_parents() {
        let root = Path::new("/workspace");
        let matcher = build_matcher(root, &settings(&["*.tmp", "build/"]));

        assert!(is_extra_ignored(&matcher, "scratch.tmp", false));
        assert!(is_extra_ignored(&matcher, "build", true));
        assert!(is_extra_ignored(&matcher, "build/out.js", false));
        assert!(!is_extra_ignored(&matcher, "src/main.rs", false));
    }

    #[test]
    fn skips_blank_lines_and_comments() {
        let root = Path::new("/workspace");
        let matcher = build_matcher(root, &settings(&["", "   ", "# a comment", "*.tmp"]));

        assert!(is_extra_ignored(&matcher, "a.tmp", false));
        assert!(!is_extra_ignored(&matcher, "# a comment", false));
    }

    #[test]
    fn one_bad_glob_does_not_disable_the_rest() {
        let root = Path::new("/workspace");
        let matcher = build_matcher(root, &settings(&["[unclosed", "*.tmp"]));

        assert!(is_extra_ignored(&matcher, "a.tmp", false));
    }

    #[test]
    fn empty_settings_ignore_nothing() {
        let matcher = build_matcher(Path::new("/workspace"), &WorkspaceSettings::default());
        assert!(!is_extra_ignored(&matcher, "src/main.rs", false));
        assert!(!is_extra_ignored(&matcher, "", true));
    }
}
