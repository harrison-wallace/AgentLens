//! The settings currently *in effect*, and the matcher compiled from them.
//!
//! Storage is deliberately absent. A daemon has no settings store of its own —
//! it is told the rules by whoever is driving it, and that driver (the desktop
//! app) persists them locally. So this module holds the live values and the
//! compiled `Gitignore` that `list_dir` consults on every call, and nothing
//! more.

use std::sync::Mutex;

use ignore::gitignore::Gitignore;

use crate::ignores::build_matcher;
use crate::protocol::{AppSettings, WorkspaceSettings};
use crate::visibility::Visibility;

/// The settings in effect, in both scopes.
#[derive(Default)]
pub struct SettingsState {
    pub workspace: Mutex<Active>,
    pub app: Mutex<AppSettings>,
}

/// The current workspace settings alongside their compiled matcher.
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

/// Replace the in-memory app-level settings.
pub fn set_app(state: &SettingsState, value: AppSettings) -> Result<(), String> {
    let mut guard = state.app.lock().map_err(|_| "settings state poisoned")?;
    *guard = value;
    Ok(())
}

/// The app-level settings currently in effect.
pub fn current_app(state: &SettingsState) -> Result<AppSettings, String> {
    let guard = state.app.lock().map_err(|_| "settings state poisoned")?;
    Ok(guard.clone())
}

/// The visibility rules both scopes add up to — what the tree, the file index,
/// and the watcher all filter through.
pub fn current_visibility(state: &SettingsState) -> Result<Visibility, String> {
    Ok(Visibility::new(&current(state)?, &current_app(state)?))
}

/// Replace the in-memory workspace settings and recompile their matcher
/// against `root`.
pub fn activate(
    state: &SettingsState,
    root: &std::path::Path,
    settings: WorkspaceSettings,
) -> Result<(), String> {
    let matcher = build_matcher(root, &settings);
    let mut guard = state
        .workspace
        .lock()
        .map_err(|_| "settings state poisoned")?;
    *guard = Active { settings, matcher };
    Ok(())
}

/// Reset to defaults (workspace closed). App-level settings are deliberately
/// left alone — they outlive the workspace.
pub fn deactivate(state: &SettingsState) -> Result<(), String> {
    let mut guard = state
        .workspace
        .lock()
        .map_err(|_| "settings state poisoned")?;
    *guard = Active::default();
    Ok(())
}

/// The workspace settings currently in effect.
pub fn current(state: &SettingsState) -> Result<WorkspaceSettings, String> {
    let guard = state
        .workspace
        .lock()
        .map_err(|_| "settings state poisoned")?;
    Ok(guard.settings.clone())
}

/// A clone of the matcher currently in effect, for callers that need to hold
/// it without keeping the lock (the watcher keeps one for its whole run).
pub fn current_matcher(state: &SettingsState) -> Gitignore {
    state
        .workspace
        .lock()
        .map(|guard| guard.matcher.clone())
        .unwrap_or_else(|_| Gitignore::empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn activating_compiles_the_extra_globs() {
        let state = SettingsState::default();
        activate(
            &state,
            Path::new("/workspace"),
            WorkspaceSettings {
                extra_ignores: vec!["*.tmp".into()],
                ..Default::default()
            },
        )
        .unwrap();

        let matcher = current_matcher(&state);
        assert!(crate::ignores::is_extra_ignored(&matcher, "a.tmp", false));
        assert_eq!(current(&state).unwrap().extra_ignores, vec!["*.tmp"]);
    }

    #[test]
    fn deactivating_clears_the_workspace_scope_but_not_the_app_scope() {
        let state = SettingsState::default();
        set_app(
            &state,
            AppSettings {
                show_agent_context: false,
                agent_roots: vec!["/tmp/roots".into()],
                ..Default::default()
            },
        )
        .unwrap();
        activate(
            &state,
            Path::new("/workspace"),
            WorkspaceSettings {
                show_ignored: true,
                ..Default::default()
            },
        )
        .unwrap();

        deactivate(&state).unwrap();

        assert_eq!(current(&state).unwrap(), WorkspaceSettings::default());
        assert_eq!(current_app(&state).unwrap().agent_roots, vec!["/tmp/roots"]);
    }

    #[test]
    fn visibility_combines_both_scopes() {
        let state = SettingsState::default();
        set_app(
            &state,
            AppSettings {
                show_agent_context: true,
                ..Default::default()
            },
        )
        .unwrap();
        activate(
            &state,
            Path::new("/workspace"),
            WorkspaceSettings {
                pinned: vec!["notes".into()],
                ..Default::default()
            },
        )
        .unwrap();

        let visibility = current_visibility(&state).unwrap();
        assert!(visibility.forced("notes/a.md"));
        assert!(visibility.forced("AGENTS.md"));
    }
}
