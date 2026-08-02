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
        let Some(path) = to_workspace_relative(root, &root.join(rel_path)) else {
            continue;
        };
        let flags = entry.status();

        // A conflicted path has no meaningful two-sided split — it is one
        // problem to resolve, not staged work plus unstaged work.
        if flags.is_conflicted() {
            files.push(GitFileStatus {
                path,
                status: GitStatusKind::Conflicted,
                staged: false,
            });
            continue;
        }

        // Both sides, when both differ. A file staged and then edited again
        // (`MM`) genuinely has staged work *and* unstaged work, and collapsing
        // it to one entry hides whichever side loses — which made the commit
        // box report nothing to commit while `git commit` would have
        // committed the staged version.
        if let Some(status) = classify_index(flags) {
            files.push(GitFileStatus {
                path: path.clone(),
                status,
                staged: true,
            });
        }
        if let Some(status) = classify_worktree(flags) {
            files.push(GitFileStatus {
                path,
                status,
                staged: false,
            });
        }
    }

    Ok(GitStatusSnapshot {
        is_repository: true,
        branch,
        files,
    })
}

/// How the index differs from `HEAD`, if it does.
fn classify_index(flags: Status) -> Option<GitStatusKind> {
    if flags.is_index_new() {
        Some(GitStatusKind::Added)
    } else if flags.is_index_modified() || flags.is_index_typechange() {
        Some(GitStatusKind::Modified)
    } else if flags.is_index_deleted() {
        Some(GitStatusKind::Deleted)
    } else if flags.is_index_renamed() {
        Some(GitStatusKind::Renamed)
    } else {
        None
    }
}

/// How the working tree differs from the index, if it does.
fn classify_worktree(flags: Status) -> Option<GitStatusKind> {
    if flags.is_wt_new() {
        Some(GitStatusKind::Untracked)
    } else if flags.is_wt_modified() || flags.is_wt_typechange() {
        Some(GitStatusKind::Modified)
    } else if flags.is_wt_deleted() {
        Some(GitStatusKind::Deleted)
    } else if flags.is_wt_renamed() {
        Some(GitStatusKind::Renamed)
    } else {
        None
    }
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

    #[test]
    fn a_file_staged_then_edited_again_reports_both_sides() {
        // Regression: this used to collapse to the working-tree side alone,
        // so the commit box saw nothing staged and disabled itself while
        // `git commit` would happily have committed the staged version. An
        // agent editing a file it already staged hits this constantly.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = Repository::init(root).unwrap();
        fs::write(root.join("file.txt"), "one\n").unwrap();
        commit_all(&repo, "initial");

        fs::write(root.join("file.txt"), "two\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("file.txt")).unwrap();
        index.write().unwrap();
        fs::write(root.join("file.txt"), "three\n").unwrap();

        let snapshot = status(root).unwrap();
        let sides: Vec<(&str, bool)> = snapshot
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.staged))
            .collect();

        assert_eq!(sides, vec![("file.txt", true), ("file.txt", false)]);
    }

    #[test]
    fn a_wholly_staged_file_reports_only_the_staged_side() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = Repository::init(root).unwrap();
        fs::write(root.join("kept.txt"), "x\n").unwrap();
        commit_all(&repo, "initial");

        fs::write(root.join("added.txt"), "new\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("added.txt")).unwrap();
        index.write().unwrap();

        let snapshot = status(root).unwrap();
        assert_eq!(snapshot.files.len(), 1, "no phantom unstaged entry");
        assert_eq!(snapshot.files[0].status, GitStatusKind::Added);
        assert!(snapshot.files[0].staged);
    }
}
