//! What stays visible regardless of `.gitignore`.
//!
//! `.gitignore` encodes two unrelated things through one mechanism: "this is
//! noise" (`node_modules/`, `target/`) and "this is mine, not the team's"
//! (planning docs, agent context files). Treating both as uninteresting is
//! right for the first and backwards for the second, so this module carries
//! the exceptions: the agent context files, and the workspace's pins.
//!
//! The tree, the file index, and the watcher all consult the same `Visibility`
//! so they can't disagree about what is on screen.

use crate::protocol::{AppSettings, WorkspaceSettings};

/// Files that instruct a coding agent, matched by exact workspace-relative
/// path (case-insensitively — Windows filesystems are, and these names are
/// conventionally capitalised).
///
/// Only root-level paths: a nested `notes/AGENTS.md` is pinnable, and keeping
/// the match exact means the file index never has to walk ignored subtrees
/// hunting for a basename. This list will need occasional maintenance as new
/// agents appear — that is the standing cost of the feature, and it is why it
/// lives in exactly one place.
pub const AGENT_CONTEXT_FILES: [&str; 6] = [
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    ".cursorrules",
    ".clinerules",
    ".github/copilot-instructions.md",
];

/// True if `relative` is one of the recognised agent context files.
pub fn is_agent_context(relative: &str) -> bool {
    AGENT_CONTEXT_FILES
        .iter()
        .any(|name| name.eq_ignore_ascii_case(relative))
}

/// True if `child` sits under directory `parent` (neither may be empty).
fn is_under(parent: &str, child: &str) -> bool {
    child.len() > parent.len()
        && child.as_bytes()[parent.len()] == b'/'
        && child.starts_with(parent)
}

/// Same, ignoring ASCII case, for matching against the agent context list.
fn is_under_insensitive(parent: &str, child: &str) -> bool {
    child.len() > parent.len()
        && child.as_bytes()[parent.len()] == b'/'
        && child[..parent.len()].eq_ignore_ascii_case(parent)
}

/// The visibility rules in effect for the open workspace.
#[derive(Debug, Clone, Default)]
pub struct Visibility {
    /// Show everything git ignores. The escape hatch of last resort — the
    /// rules below should mean it is rarely needed.
    pub show_ignored: bool,
    pub show_agent_context: bool,
    /// Workspace-relative, forward slashes.
    pub pinned: Vec<String>,
}

impl Visibility {
    pub fn new(settings: &WorkspaceSettings, app: &AppSettings) -> Self {
        Visibility {
            show_ignored: settings.show_ignored,
            show_agent_context: app.show_agent_context,
            pinned: settings
                .pinned
                .iter()
                .map(|path| path.trim().trim_matches('/').to_string())
                .filter(|path| !path.is_empty())
                .collect(),
        }
    }

    /// Paths that are visible in their own right: the pins, plus the agent
    /// context files when that setting is on.
    pub fn forced_roots(&self) -> impl Iterator<Item = &str> {
        let agent = if self.show_agent_context {
            &AGENT_CONTEXT_FILES[..]
        } else {
            &[][..]
        };
        self.pinned
            .iter()
            .map(String::as_str)
            .chain(agent.iter().copied())
    }

    /// True if `relative` must be shown whatever `.gitignore` says.
    ///
    /// A forced path drags its whole line with it: everything under a pinned
    /// directory (that is what pinning a directory means) and every ancestor
    /// of a forced path (otherwise there is no way to reach it in the tree —
    /// a gitignored `notes/` would hide a pinned `notes/drafts/`).
    pub fn forced(&self, relative: &str) -> bool {
        if relative.is_empty() {
            return false;
        }
        if self.show_agent_context
            && AGENT_CONTEXT_FILES.iter().any(|name| {
                name.eq_ignore_ascii_case(relative) || is_under_insensitive(relative, name)
            })
        {
            return true;
        }
        self.pinned
            .iter()
            .any(|pin| pin == relative || is_under(pin, relative) || is_under(relative, pin))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vis(pinned: &[&str], show_agent_context: bool) -> Visibility {
        Visibility::new(
            &WorkspaceSettings {
                pinned: pinned.iter().map(|p| p.to_string()).collect(),
                ..Default::default()
            },
            &AppSettings { show_agent_context },
        )
    }

    #[test]
    fn recognises_the_agent_context_files_case_insensitively() {
        assert!(is_agent_context("AGENTS.md"));
        assert!(is_agent_context("claude.md"));
        assert!(is_agent_context(".github/copilot-instructions.md"));
        assert!(!is_agent_context("notes/AGENTS.md"));
        assert!(!is_agent_context("README.md"));
    }

    #[test]
    fn agent_context_files_and_their_ancestors_are_forced() {
        let v = vis(&[], true);
        assert!(v.forced("CLAUDE.md"));
        // `.github/` has to be reachable for the file inside it to be.
        assert!(v.forced(".github"));
        assert!(!v.forced(".github/workflows"));
        assert!(!v.forced("src/lib.rs"));
    }

    #[test]
    fn the_app_setting_turns_agent_context_forcing_off() {
        let v = vis(&[], false);
        assert!(!v.forced("AGENTS.md"));
        assert!(!v.forced(".github"));
    }

    #[test]
    fn a_pinned_directory_forces_itself_its_contents_and_its_ancestors() {
        let v = vis(&["notes/drafts"], false);
        assert!(v.forced("notes/drafts"));
        assert!(v.forced("notes/drafts/spec.md"));
        assert!(v.forced("notes"));
        assert!(!v.forced("notes/other"));
        assert!(!v.forced("notesx"));
    }

    #[test]
    fn pins_are_normalised_and_blank_entries_dropped() {
        let v = vis(&["  notes/drafts/  ", "", "   "], false);
        assert_eq!(v.pinned, vec!["notes/drafts".to_string()]);
    }

    #[test]
    fn forced_roots_covers_pins_and_the_agent_list() {
        let with = vis(&["notes.md"], true);
        let roots: Vec<&str> = with.forced_roots().collect();
        assert!(roots.contains(&"notes.md"));
        assert!(roots.contains(&"AGENTS.md"));

        let pins_only = vis(&["notes.md"], false);
        let without: Vec<&str> = pins_only.forced_roots().collect();
        assert_eq!(without, vec!["notes.md"]);
    }
}
