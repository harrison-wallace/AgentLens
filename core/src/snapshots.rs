//! Session baselines for "diff since session".
//!
//! A session's baseline for a file is its content at the moment watching
//! started. Deriving that cheaply relies on one observation: at session start
//! every *clean* tracked file already equals its `HEAD` blob, so only the
//! files git reports as dirty need capturing eagerly — normally a handful.
//! Everything else is read from `HEAD` on demand, and files that existed in
//! neither have no baseline (the diff shows them as wholly added).
//!
//! This deviates from the phase plan, which captures a copy the first time a
//! file is modified during the session. That is a beat too late: by the time
//! the watcher reports a write, the pre-session content is already gone. The
//! approach here reconstructs the true session-start content instead.
//!
//! Baselines come from git, so a workspace that isn't a repository reports
//! the feature as unavailable rather than inventing a comparison.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use git2::Repository;

use crate::gitstatus;
use crate::paths::{resolve_existing_in_workspace, resolve_in_workspace};
use crate::protocol::{DiffUnavailable, SessionDiff};

/// Per-file cap for both baseline and current content, matching the plan.
const MAX_SNAPSHOT_BYTES: u64 = 1024 * 1024;

/// Cap on eagerly captured baselines, so opening a workspace with a huge
/// dirty tree can't stall on reads.
const MAX_CAPTURED_FILES: usize = 2_000;

/// Baselines for the current session.
#[derive(Debug, Default, Clone)]
pub struct Session {
    /// False when the workspace isn't a git repository.
    pub is_repository: bool,
    /// Session-start text of files that were already dirty at that moment.
    /// Absent means "was clean, so `HEAD` is the baseline".
    captured: HashMap<String, Option<String>>,
}

/// Tauri-managed state holding the current session's baselines.
#[derive(Default)]
pub struct SessionState(pub Mutex<Session>);

/// Capture session-start baselines for `root`. Runs on workspace open and on
/// an explicit session restart.
pub fn capture(root: &Path) -> Session {
    let Ok(snapshot) = gitstatus::status(root) else {
        return Session::default();
    };
    if !snapshot.is_repository {
        return Session::default();
    }

    let mut captured = HashMap::new();
    for file in snapshot.files.iter().take(MAX_CAPTURED_FILES) {
        if captured.contains_key(&file.path) {
            continue;
        }
        captured.insert(file.path.clone(), read_text_capped(&root.join(&file.path)));
    }

    Session {
        is_repository: true,
        captured,
    }
}

/// Replace the stored session with freshly captured baselines.
pub fn restart(state: &SessionState, root: &Path) -> Result<(), String> {
    let session = capture(root);
    let mut guard = state.0.lock().map_err(|_| "session state poisoned")?;
    *guard = session;
    Ok(())
}

/// Forget the current session (workspace closed).
pub fn clear(state: &SessionState) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|_| "session state poisoned")?;
    *guard = Session::default();
    Ok(())
}

/// Baseline and current content of `relative`, for the diff tab.
pub fn diff(state: &SessionState, root: &Path, relative: &str) -> Result<SessionDiff, String> {
    let joined = resolve_in_workspace(root, relative)?;
    // A file deleted during the session has no real path to canonicalize, and
    // that is a legitimate "no current content" — but a file that *does*
    // exist has to survive the symlink check before it's read.
    let current_path = if joined.exists() {
        Some(resolve_existing_in_workspace(root, relative)?)
    } else {
        None
    };
    let session = {
        let guard = state.0.lock().map_err(|_| "session state poisoned")?;
        guard.clone()
    };

    if !session.is_repository {
        return Ok(unavailable(relative, DiffUnavailable::NotARepository));
    }
    // An ignored file has no HEAD blob and never appears in git's status, so
    // both baseline sources come up empty for it — which would render as a
    // brand-new file rather than as "git can't tell you".
    if is_git_ignored(root, relative) {
        return Ok(unavailable(relative, DiffUnavailable::NotTracked));
    }

    let baseline = match session.captured.get(relative) {
        Some(captured) => captured.clone(),
        None => head_text(root, relative),
    };

    Ok(SessionDiff {
        path: relative.to_string(),
        baseline,
        current: current_path.as_deref().and_then(read_text_capped),
        unavailable: None,
    })
}

fn unavailable(relative: &str, why: DiffUnavailable) -> SessionDiff {
    SessionDiff {
        path: relative.to_string(),
        baseline: None,
        current: None,
        unavailable: Some(why),
    }
}

/// True if git ignores `relative`. Best-effort: an unreadable repo just means
/// we fall through to the normal baseline lookup.
fn is_git_ignored(root: &Path, relative: &str) -> bool {
    Repository::open(root)
        .and_then(|repo| repo.is_path_ignored(Path::new(relative)))
        .unwrap_or(false)
}

/// Read a file as UTF-8, or `None` if it's missing, oversized, or not text.
/// Every "can't show this" case collapses to `None` on purpose: the diff has
/// no way to render a partial or lossy side.
fn read_text_capped(path: &Path) -> Option<String> {
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SNAPSHOT_BYTES {
        return None;
    }
    String::from_utf8(std::fs::read(path).ok()?).ok()
}

/// Content of `relative` at `HEAD`, or `None` if it isn't there (or isn't
/// text). Used for files that were clean when the session started.
fn head_text(root: &Path, relative: &str) -> Option<String> {
    let repo = Repository::open(root).ok()?;
    let tree = repo.head().ok()?.peel_to_tree().ok()?;
    let entry = tree.get_path(Path::new(relative)).ok()?;
    let blob = repo.find_blob(entry.id()).ok()?;
    if blob.size() as u64 > MAX_SNAPSHOT_BYTES {
        return None;
    }
    String::from_utf8(blob.content().to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Signature;
    use std::fs;

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

    fn state_for(root: &Path) -> SessionState {
        SessionState(Mutex::new(capture(root)))
    }

    #[test]
    fn clean_file_edited_during_the_session_diffs_against_head() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = Repository::init(root).unwrap();
        fs::write(root.join("a.txt"), "one\n").unwrap();
        commit_all(&repo, "initial");

        // Session starts with a clean tree, then the agent edits the file.
        let state = state_for(root);
        fs::write(root.join("a.txt"), "one\ntwo\n").unwrap();

        let diff = diff(&state, root, "a.txt").unwrap();
        assert_eq!(diff.unavailable, None);
        assert_eq!(diff.baseline.as_deref(), Some("one\n"));
        assert_eq!(diff.current.as_deref(), Some("one\ntwo\n"));
    }

    #[test]
    fn file_already_dirty_at_session_start_diffs_against_that_content() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = Repository::init(root).unwrap();
        fs::write(root.join("a.txt"), "committed\n").unwrap();
        commit_all(&repo, "initial");

        // The user had uncommitted work before AgentLens started; that, not
        // HEAD, is what "since session" has to mean.
        fs::write(root.join("a.txt"), "my own edit\n").unwrap();
        let state = state_for(root);
        fs::write(root.join("a.txt"), "my own edit\nagent edit\n").unwrap();

        let diff = diff(&state, root, "a.txt").unwrap();
        assert_eq!(diff.baseline.as_deref(), Some("my own edit\n"));
        assert_eq!(diff.current.as_deref(), Some("my own edit\nagent edit\n"));
    }

    #[test]
    fn file_created_during_the_session_has_no_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = Repository::init(root).unwrap();
        fs::write(root.join("a.txt"), "one\n").unwrap();
        commit_all(&repo, "initial");

        let state = state_for(root);
        fs::write(root.join("new.txt"), "brand new\n").unwrap();

        let diff = diff(&state, root, "new.txt").unwrap();
        assert_eq!(diff.baseline, None);
        assert_eq!(diff.current.as_deref(), Some("brand new\n"));
    }

    #[test]
    fn file_deleted_during_the_session_keeps_its_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = Repository::init(root).unwrap();
        fs::write(root.join("a.txt"), "one\n").unwrap();
        commit_all(&repo, "initial");

        let state = state_for(root);
        fs::remove_file(root.join("a.txt")).unwrap();

        let diff = diff(&state, root, "a.txt").unwrap();
        assert_eq!(diff.baseline.as_deref(), Some("one\n"));
        assert_eq!(diff.current, None);
    }

    #[test]
    fn restart_rebaselines_on_the_current_content() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = Repository::init(root).unwrap();
        fs::write(root.join("a.txt"), "one\n").unwrap();
        commit_all(&repo, "initial");

        let state = state_for(root);
        fs::write(root.join("a.txt"), "one\ntwo\n").unwrap();
        restart(&state, root).unwrap();

        let diff = diff(&state, root, "a.txt").unwrap();
        assert_eq!(diff.baseline.as_deref(), Some("one\ntwo\n"));
        assert_eq!(diff.current.as_deref(), Some("one\ntwo\n"));
    }

    #[test]
    fn non_repository_reports_the_feature_as_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.txt"), "one\n").unwrap();

        let state = state_for(root);
        let diff = diff(&state, root, "a.txt").unwrap();
        assert_eq!(diff.unavailable, Some(DiffUnavailable::NotARepository));
        assert_eq!(diff.baseline, None);
        assert_eq!(diff.current, None);
    }

    #[test]
    fn gitignored_file_reports_not_tracked_rather_than_wholly_added() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let repo = Repository::init(root).unwrap();
        fs::write(root.join(".gitignore"), "dist/\n").unwrap();
        fs::create_dir(root.join("dist")).unwrap();
        fs::write(root.join("dist/out.js"), "before\n").unwrap();
        commit_all(&repo, "initial");

        let state = state_for(root);
        fs::write(root.join("dist/out.js"), "before\nafter\n").unwrap();

        // It existed with different content, so "wholly added" would be a lie.
        let diff = diff(&state, root, "dist/out.js").unwrap();
        assert_eq!(diff.unavailable, Some(DiffUnavailable::NotTracked));
        assert_eq!(diff.baseline, None);
        assert_eq!(diff.current, None);
    }

    #[test]
    fn refuses_paths_outside_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let state = state_for(dir.path());
        assert!(diff(&state, dir.path(), "../secret").is_err());
    }
}
