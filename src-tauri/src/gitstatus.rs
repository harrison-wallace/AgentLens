//! Git status snapshot via `git2`.

use std::path::Path;

use git2::{Repository, Status, StatusOptions};

use crate::paths::to_workspace_relative;
use crate::protocol::{GitFileStatus, GitStatusKind, GitStatusSnapshot};

/// Read the working-tree git status of `root`. Returns
/// `is_repository: false` (not an error) when `root` isn't a git repository.
pub fn status(root: &Path) -> Result<GitStatusSnapshot, String> {
    let repo = match Repository::open(root) {
        Ok(repo) => repo,
        Err(_) => {
            return Ok(GitStatusSnapshot {
                is_repository: false,
                branch: None,
                files: vec![],
            });
        }
    };

    // `head()` errors on a detached HEAD's underlying ref lookup in some
    // cases and always errors on an unborn branch (fresh repo, no commits);
    // both are "no branch to report", not a failure.
    let branch = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(str::to_string));

    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);

    let statuses = repo
        .statuses(Some(&mut opts))
        .map_err(|e| format!("failed to read git status: {e}"))?;

    let mut files = Vec::new();
    for entry in statuses.iter() {
        let Some(rel_path) = entry.path() else {
            continue;
        };
        let Some((kind, staged)) = classify(entry.status()) else {
            continue;
        };
        let Some(path) = to_workspace_relative(root, &root.join(rel_path)) else {
            continue;
        };
        files.push(GitFileStatus {
            path,
            status: kind,
            staged,
        });
    }

    Ok(GitStatusSnapshot {
        is_repository: true,
        branch,
        files,
    })
}

/// Map git2's status flags to one `(GitStatusKind, staged)` per path.
/// Worktree flags win over index flags when both are set, since that's
/// what the tree badge should show.
fn classify(flags: Status) -> Option<(GitStatusKind, bool)> {
    if flags.is_conflicted() {
        return Some((GitStatusKind::Conflicted, false));
    }
    if flags.is_wt_new() {
        return Some((GitStatusKind::Untracked, false));
    }
    if flags.is_wt_modified() || flags.is_wt_typechange() {
        return Some((GitStatusKind::Modified, false));
    }
    if flags.is_wt_deleted() {
        return Some((GitStatusKind::Deleted, false));
    }
    if flags.is_wt_renamed() {
        return Some((GitStatusKind::Renamed, false));
    }
    if flags.is_index_new() {
        return Some((GitStatusKind::Added, true));
    }
    if flags.is_index_modified() || flags.is_index_typechange() {
        return Some((GitStatusKind::Modified, true));
    }
    if flags.is_index_deleted() {
        return Some((GitStatusKind::Deleted, true));
    }
    if flags.is_index_renamed() {
        return Some((GitStatusKind::Renamed, true));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Signature;
    use std::fs;

    /// Stage everything and commit with an explicit signature — CI has no
    /// user.name/user.email configured, so we never read it.
    fn commit_all(repo: &Repository, message: &str) {
        let sig = Signature::now("Test User", "test@example.com").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();

        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();

        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap();
    }

    #[test]
    fn non_repository_reports_is_repository_false() {
        let dir = tempfile::tempdir().unwrap();
        let snapshot = status(dir.path()).unwrap();
        assert!(!snapshot.is_repository);
        assert_eq!(snapshot.branch, None);
        assert!(snapshot.files.is_empty());
    }

    #[test]
    fn fresh_repo_has_no_branch_and_no_error() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        Repository::init(root).unwrap();

        let snapshot = status(root).unwrap();
        assert!(snapshot.is_repository);
        assert_eq!(snapshot.branch, None);
    }

    #[test]
    fn untracked_file_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        Repository::init(root).unwrap();
        fs::write(root.join("new.txt"), "hello").unwrap();

        let snapshot = status(root).unwrap();
        assert!(snapshot.is_repository);
        assert_eq!(snapshot.files.len(), 1);
        assert_eq!(snapshot.files[0].path, "new.txt");
        assert_eq!(snapshot.files[0].status, GitStatusKind::Untracked);
        assert!(!snapshot.files[0].staged);
    }

    #[test]
    fn staged_new_file_is_added() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = Repository::init(root).unwrap();
        fs::write(root.join("new.txt"), "hello").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("new.txt")).unwrap();
        index.write().unwrap();

        let snapshot = status(root).unwrap();
        assert_eq!(snapshot.files.len(), 1);
        assert_eq!(snapshot.files[0].status, GitStatusKind::Added);
        assert!(snapshot.files[0].staged);
    }

    #[test]
    fn modified_file_after_commit_is_unstaged_modified() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = Repository::init(root).unwrap();
        fs::write(root.join("file.txt"), "hello").unwrap();
        commit_all(&repo, "initial commit");

        fs::write(root.join("file.txt"), "changed").unwrap();

        let snapshot = status(root).unwrap();
        assert!(snapshot.branch.is_some());
        assert_eq!(snapshot.files.len(), 1);
        assert_eq!(snapshot.files[0].path, "file.txt");
        assert_eq!(snapshot.files[0].status, GitStatusKind::Modified);
        assert!(!snapshot.files[0].staged);
    }
}
