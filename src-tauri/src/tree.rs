//! Lazy, gitignore-aware directory listing, plus the flat file index behind
//! the `Ctrl+P` jump.

use std::path::Path;

use ignore::gitignore::Gitignore;
use ignore::WalkBuilder;

use crate::paths::{resolve_in_workspace, to_workspace_relative};
use crate::protocol::DirEntryNode;
use crate::settings::is_extra_ignored;

/// Upper bound on the `Ctrl+P` index. A repo larger than this still works;
/// the tail is simply not jumpable, which beats stalling the UI.
const MAX_INDEXED_FILES: usize = 50_000;

/// Directory names filtered out regardless of `.gitignore` contents, on top
/// of which each workspace can add its own globs (see `settings.rs`). Shared
/// with `watcher.rs` so the tree and the watcher can never disagree about
/// what's ignored.
pub(crate) const BUILTIN_IGNORED_DIRS: [&str; 3] = [".git", "node_modules", "target"];

/// List the immediate children of `relative` (workspace-relative) inside
/// `root`, honouring `.gitignore`, the built-in ignore list, and the
/// workspace's extra ignore globs. Directories sort before files; both are
/// sorted by name case-insensitively.
pub fn list_dir(
    root: &Path,
    relative: &str,
    extra: &Gitignore,
) -> Result<Vec<DirEntryNode>, String> {
    let target = resolve_in_workspace(root, relative)?;

    let mut entries: Vec<DirEntryNode> = WalkBuilder::new(&target)
        .max_depth(Some(1))
        .hidden(false)
        .git_ignore(true)
        // `ignore` only applies `.gitignore` inside a real repository by
        // default. The watcher reads it either way, so requiring git here
        // would make the tree and the feed disagree in a plain directory.
        .require_git(false)
        .build()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path() != target)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .map(|name| !BUILTIN_IGNORED_DIRS.contains(&name))
                .unwrap_or(true)
        })
        .filter_map(|entry| {
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = to_workspace_relative(root, entry.path())?;
            Some(DirEntryNode { name, path, is_dir })
        })
        .filter(|entry| !is_extra_ignored(extra, &entry.path, entry.is_dir))
        .collect();

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

/// Every non-ignored file in the workspace, workspace-relative, for the
/// `Ctrl+P` jump. Directories are excluded — you jump to a file.
pub fn list_files(root: &Path, extra: &Gitignore) -> Vec<String> {
    WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .require_git(false)
        .filter_entry(|entry| {
            entry.depth() == 0
                || entry
                    .file_name()
                    .to_str()
                    .map(|name| !BUILTIN_IGNORED_DIRS.contains(&name))
                    .unwrap_or(true)
        })
        .build()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|entry| to_workspace_relative(root, entry.path()))
        .filter(|path| !is_extra_ignored(extra, path, false))
        .take(MAX_INDEXED_FILES)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::WorkspaceSettings;
    use crate::settings::build_matcher;
    use std::fs;

    fn no_extra() -> Gitignore {
        Gitignore::empty()
    }

    fn extra(globs: &[&str]) -> WorkspaceSettings {
        WorkspaceSettings {
            extra_ignores: globs.iter().map(|g| g.to_string()).collect(),
        }
    }

    #[test]
    fn lists_single_level_honouring_gitignore_and_builtin_ignores() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(root.join("ignored.txt"), "").unwrap();
        fs::write(root.join("visible.txt"), "").unwrap();
        fs::create_dir(root.join("node_modules")).unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::create_dir(root.join("subdir")).unwrap();
        fs::write(root.join("subdir").join("nested.txt"), "").unwrap();

        let entries = list_dir(root, "", &no_extra()).unwrap();

        let got: Vec<(String, bool)> = entries.iter().map(|e| (e.name.clone(), e.is_dir)).collect();
        assert_eq!(
            got,
            vec![
                ("subdir".to_string(), true),
                (".gitignore".to_string(), false),
                ("visible.txt".to_string(), false),
            ]
        );

        for entry in &entries {
            assert!(!entry.path.contains('\\'));
        }
    }

    #[test]
    fn lists_nested_directory_by_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("subdir")).unwrap();
        fs::write(root.join("subdir").join("nested.txt"), "").unwrap();

        let entries = list_dir(root, "subdir", &no_extra()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "nested.txt");
        assert_eq!(entries[0].path, "subdir/nested.txt");
    }

    #[test]
    fn extra_ignore_globs_hide_entries_from_the_listing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("keep.txt"), "").unwrap();
        fs::write(root.join("scratch.tmp"), "").unwrap();
        fs::create_dir(root.join("build")).unwrap();

        let matcher = build_matcher(root, &extra(&["*.tmp", "build/"]));
        let names: Vec<String> = list_dir(root, "", &matcher)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();

        assert_eq!(names, vec!["keep.txt".to_string()]);
    }

    #[test]
    fn list_files_indexes_recursively_and_honours_every_ignore_source() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::create_dir_all(root.join("src/lib")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::write(root.join("src/lib/deep.rs"), "").unwrap();
        fs::write(root.join("ignored.txt"), "").unwrap();
        fs::write(root.join("scratch.tmp"), "").unwrap();
        fs::write(root.join("node_modules/pkg/index.js"), "").unwrap();

        let matcher = build_matcher(root, &extra(&["*.tmp"]));
        let mut files = list_files(root, &matcher);
        files.sort();

        assert_eq!(
            files,
            vec![".gitignore".to_string(), "src/lib/deep.rs".to_string()]
        );
    }

    #[test]
    fn rejects_traversal_outside_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(list_dir(root, "../secret", &no_extra()).is_err());
    }
}
