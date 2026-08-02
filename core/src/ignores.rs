//! Extra ignore globs, compiled.
//!
//! A workspace can hide paths beyond what `.gitignore` says. Those globs are
//! gitignore syntax, compiled into the same kind of matcher `.gitignore`
//! produces, so the tree, the file index, and the watcher all apply them the
//! same way.
//!
//! Only the compiling lives here. Where the globs are *stored* is the app's
//! problem, and a daemon has no settings store of its own — it is told the
//! rules by whoever is driving it.

use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::protocol::WorkspaceSettings;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(globs: &[&str]) -> WorkspaceSettings {
        WorkspaceSettings {
            extra_ignores: globs.iter().map(|g| g.to_string()).collect(),
            ..Default::default()
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
