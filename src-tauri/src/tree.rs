//! Lazy, gitignore-aware single-level directory listing.

use std::path::Path;

use ignore::WalkBuilder;

use crate::paths::{resolve_in_workspace, to_workspace_relative};
use crate::protocol::DirEntryNode;

/// Directory names filtered out regardless of `.gitignore` contents.
/// Configurable extra ignores are a later task in the phase plan.
const BUILTIN_IGNORED_DIRS: [&str; 3] = [".git", "node_modules", "target"];

/// List the immediate children of `relative` (workspace-relative) inside
/// `root`, honouring `.gitignore` and the built-in ignore list. Directories
/// sort before files; both are sorted by name case-insensitively.
pub fn list_dir(root: &Path, relative: &str) -> Result<Vec<DirEntryNode>, String> {
    let target = resolve_in_workspace(root, relative)?;

    let mut entries: Vec<DirEntryNode> = WalkBuilder::new(&target)
        .max_depth(Some(1))
        .hidden(false)
        .git_ignore(true)
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
        .collect();

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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

        let entries = list_dir(root, "").unwrap();

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

        let entries = list_dir(root, "subdir").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "nested.txt");
        assert_eq!(entries[0].path, "subdir/nested.txt");
    }

    #[test]
    fn rejects_traversal_outside_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(list_dir(root, "../secret").is_err());
    }
}
