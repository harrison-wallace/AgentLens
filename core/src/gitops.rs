//! Git mutations, via the `git` CLI.
//!
//! Reads stay on `git2` (see `gitstatus.rs`); writes go through the porcelain
//! commands the user would have typed. That asymmetry is deliberate: `git add`
//! and `git commit` run the repository's hooks, honour its config, and produce
//! exactly the messages the terminal does. For a mutation, matching what the
//! user expects matters more than the speed libgit2 would buy — and
//! reimplementing hook execution to get there would be worse than shelling
//! out.
//!
//! **Nothing here goes through a shell.** Every call is a direct process spawn
//! with an argument array and an explicit working directory, so a path
//! containing a space, a quote, or a `;` is data rather than syntax. Paths are
//! resolved through `resolve_in_workspace` first and passed after `--`, so a
//! file named `-f` can't become a flag either.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::paths::resolve_in_workspace;
use crate::protocol::{BranchList, DiffUnavailable, GitCapabilities, SessionDiff};

/// Ceiling for either side of a git diff, matching the session snapshot cap
/// in `snapshots.rs`. Past this the pane says so rather than shipping a
/// multi-megabyte string down a pipe to render two thousand rows of it.
const MAX_DIFF_BYTES: u64 = 1024 * 1024;

/// Output of one `git` invocation that exited zero.
struct GitOutput {
    stdout: String,
}

/// Run `git` in `root` with `args`.
///
/// The error is git's own stderr, trimmed — the UI shows it in a copyable
/// detail drawer, and paraphrasing it would only lose information the user
/// needs to act on.
fn run(root: &Path, args: &[&str]) -> Result<GitOutput, String> {
    let output = Command::new("git")
        .args(args)
        // Explicit, rather than inheriting the app's cwd: the app's process
        // directory has nothing to do with which repository this is.
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                "git is not installed, or not on PATH — install it to enable git actions".into()
            }
            _ => format!("failed to run git: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Some porcelain reports failure detail on stdout (`git stash pop`
        // with a conflict, for one), so fall back rather than showing nothing.
        return Err(if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("git {} failed", args.join(" "))
        });
    }

    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
    })
}

/// Whether git mutations can be offered at all.
///
/// Checked when a workspace opens so the UI can degrade to read-only with a
/// hint, rather than presenting buttons that fail the moment they're pressed.
pub fn capabilities(root: &Path) -> GitCapabilities {
    match run(root, &["--version"]) {
        Ok(out) => GitCapabilities {
            can_mutate: true,
            version: Some(out.stdout.trim().to_string()),
            reason: None,
        },
        Err(reason) => GitCapabilities {
            can_mutate: false,
            version: None,
            reason: Some(reason),
        },
    }
}

/// Turn workspace-relative protocol paths into arguments git will accept.
///
/// Each one goes through `resolve_in_workspace`, so a path that tries to
/// escape the workspace is rejected here rather than handed to git. They stay
/// relative afterwards: git resolves them against `current_dir`, and a
/// relative path can't accidentally address another repository.
fn checked_paths(root: &Path, paths: &[String]) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Err("no files given".into());
    }
    for path in paths {
        resolve_in_workspace(root, path)?;
    }
    Ok(paths.to_vec())
}

/// Stage `paths`. Handles deletions as well as edits, so a removed file can be
/// staged like any other change.
pub fn stage(root: &Path, paths: &[String]) -> Result<(), String> {
    let paths = checked_paths(root, paths)?;
    let mut args = vec!["add", "--all", "--"];
    args.extend(paths.iter().map(String::as_str));
    run(root, &args).map(|_| ())
}

/// Stage every change in the workspace.
pub fn stage_all(root: &Path) -> Result<(), String> {
    run(root, &["add", "--all"]).map(|_| ())
}

/// Unstage `paths`, leaving the working tree untouched.
pub fn unstage(root: &Path, paths: &[String]) -> Result<(), String> {
    let paths = checked_paths(root, paths)?;
    let mut args = vec!["restore", "--staged", "--"];
    args.extend(paths.iter().map(String::as_str));
    run(root, &args).map(|_| ())
}

/// Unstage everything.
pub fn unstage_all(root: &Path) -> Result<(), String> {
    run(root, &["reset"]).map(|_| ())
}

/// Commit what's staged.
///
/// `amend` rewrites the previous commit. Sign-off and GPG signing are left to
/// the repository's own config rather than exposed as options: they're
/// per-repo policy, and duplicating them in the UI invites the two disagreeing.
pub fn commit(root: &Path, message: &str, amend: bool) -> Result<(), String> {
    if message.trim().is_empty() {
        return Err("commit message is empty".into());
    }
    // `--message` with the text as its own argument: a message containing
    // newlines, quotes or leading dashes is data, never parsed as flags.
    let mut args = vec!["commit", "--message", message];
    if amend {
        args.push("--amend");
    }
    run(root, &args).map(|_| ())
}

/// Local branches, and which one is checked out.
pub fn branches(root: &Path) -> Result<BranchList, String> {
    // `--format` keeps this independent of `git branch`'s decorated output.
    let out = run(
        root,
        &["branch", "--list", "--format=%(refname:short)", "--"],
    )?;
    let branches: Vec<String> = out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    // Empty on a detached HEAD or an unborn branch; both are "nothing checked
    // out to report", not failures.
    let current = run(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .ok()
        .map(|out| out.stdout.trim().to_string())
        .filter(|name| !name.is_empty());

    Ok(BranchList { current, branches })
}

/// Check out an existing branch.
pub fn switch_branch(root: &Path, name: &str) -> Result<(), String> {
    run(root, &["switch", "--", name]).map(|_| ())
}

/// Create a branch from the current HEAD and check it out.
pub fn create_branch(root: &Path, name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("branch name is empty".into());
    }
    run(root, &["switch", "--create", name]).map(|_| ())
}

/// Stash the working tree, including untracked files.
///
/// Untracked are included because the alternative surprises: stashing to
/// switch branches, then finding new files left behind, is worse than having
/// them travel with the stash.
pub fn stash_push(root: &Path, message: Option<&str>) -> Result<(), String> {
    let mut args = vec!["stash", "push", "--include-untracked"];
    if let Some(message) = message.map(str::trim).filter(|m| !m.is_empty()) {
        args.push("--message");
        args.push(message);
    }
    run(root, &args).map(|_| ())
}

/// Restore the most recent stash and drop it.
pub fn stash_pop(root: &Path) -> Result<(), String> {
    run(root, &["stash", "pop"]).map(|_| ())
}

/// `HEAD` versus the working tree (`staged: false`) or the index (`staged: true`).
///
/// Returns a `SessionDiff` so the UI reuses the same line-diff path as the
/// session comparison. Failures that are about the *file* (not a repo, not
/// tracked, not text) land in `unavailable` rather than as an error, so the
/// pane always has something to render.
pub fn file_diff(root: &Path, path: &str, staged: bool) -> Result<SessionDiff, String> {
    // Reject escapes before any git invocation sees the path.
    let abs = resolve_in_workspace(root, path)?;

    if run(root, &["rev-parse", "--git-dir"]).is_err() {
        return Ok(unavailable(path, DiffUnavailable::NotARepository));
    }

    let baseline = match head_text(root, path)? {
        Side::Missing => None,
        Side::Text(s) => Some(s),
        Side::NotText => return Ok(unavailable(path, DiffUnavailable::NotText)),
        Side::TooLarge => return Ok(unavailable(path, DiffUnavailable::TooLarge)),
    };

    let current = if staged {
        match index_text(root, path)? {
            Side::Missing => None,
            Side::Text(s) => Some(s),
            Side::NotText => return Ok(unavailable(path, DiffUnavailable::NotText)),
            Side::TooLarge => return Ok(unavailable(path, DiffUnavailable::TooLarge)),
        }
    } else {
        match working_tree_text(&abs)? {
            Side::Missing => None,
            Side::Text(s) => Some(s),
            Side::NotText => return Ok(unavailable(path, DiffUnavailable::NotText)),
            Side::TooLarge => return Ok(unavailable(path, DiffUnavailable::TooLarge)),
        }
    };

    if baseline.is_none() && current.is_none() {
        return Ok(unavailable(path, DiffUnavailable::NotTracked));
    }

    // Working-tree mode can still surface disk content for a path git has
    // never heard of. `git diff HEAD` skips those; refuse rather than paint
    // an untracked file as a full add against empty.
    if !staged && baseline.is_none() {
        match index_text(root, path)? {
            Side::Missing => return Ok(unavailable(path, DiffUnavailable::NotTracked)),
            Side::NotText => return Ok(unavailable(path, DiffUnavailable::NotText)),
            Side::TooLarge => return Ok(unavailable(path, DiffUnavailable::TooLarge)),
            Side::Text(_) => {}
        }
    }

    Ok(SessionDiff {
        path: path.to_string(),
        baseline,
        current,
        unavailable: None,
    })
}

fn unavailable(path: &str, why: DiffUnavailable) -> SessionDiff {
    SessionDiff {
        path: path.to_string(),
        baseline: None,
        current: None,
        unavailable: Some(why),
    }
}

/// One side of a git file comparison.
enum Side {
    Missing,
    Text(String),
    NotText,
    TooLarge,
}

/// Bytes of a successful `git` invocation, without lossy UTF-8 conversion.
///
/// `run` is fine for porcelain messages; blob contents must stay exact so a
/// binary file is detected rather than rendered as mojibake.
fn run_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                "git is not installed, or not on PATH — install it to enable git actions".into()
            }
            _ => format!("failed to run git: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("git {} failed", args.join(" "))
        });
    }

    Ok(output.stdout)
}

/// Decode blob bytes: missing → `Missing`, non-UTF-8 → `NotText`.
fn decode_blob(bytes: Vec<u8>) -> Side {
    match String::from_utf8(bytes) {
        Ok(text) => Side::Text(text),
        Err(_) => Side::NotText,
    }
}

/// Content of `path` at `HEAD`, or `Missing` when unborn / not in the tree.
fn head_text(root: &Path, path: &str) -> Result<Side, String> {
    // Unborn HEAD has no commits; treat like a brand-new file.
    if run(root, &["rev-parse", "--verify", "HEAD"]).is_err() {
        return Ok(Side::Missing);
    }
    // `rev:path` does not accept `--`. The path is already workspace-validated
    // and cannot start with `-` as a free-standing flag because it is part of
    // a single `HEAD:<path>` argument.
    let rev_path = format!("HEAD:{path}");
    match run(root, &["cat-file", "-s", &rev_path]) {
        Ok(out) => {
            let size: u64 = out.stdout.trim().parse().unwrap_or(0);
            if size > MAX_DIFF_BYTES {
                return Ok(Side::TooLarge);
            }
        }
        Err(_) => return Ok(Side::Missing),
    }
    match run_bytes(root, &["show", &rev_path]) {
        Ok(bytes) => Ok(decode_blob(bytes)),
        Err(_) => Ok(Side::Missing),
    }
}

/// Content of `path` in the index, or `Missing` when not staged.
fn index_text(root: &Path, path: &str) -> Result<Side, String> {
    // Same `rev:path` form as `head_text` — no `--`, path already validated.
    let rev_path = format!(":{path}");
    match run(root, &["cat-file", "-s", &rev_path]) {
        Ok(out) => {
            let size: u64 = out.stdout.trim().parse().unwrap_or(0);
            if size > MAX_DIFF_BYTES {
                return Ok(Side::TooLarge);
            }
        }
        Err(_) => return Ok(Side::Missing),
    }
    match run_bytes(root, &["show", &rev_path]) {
        Ok(bytes) => Ok(decode_blob(bytes)),
        Err(_) => Ok(Side::Missing),
    }
}

/// Working-tree bytes for an absolute path already resolved into the workspace.
fn working_tree_text(abs: &Path) -> Result<Side, String> {
    match abs.metadata() {
        Ok(metadata) if metadata.len() > MAX_DIFF_BYTES => return Ok(Side::TooLarge),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Side::Missing),
        // A path that exists but isn't a readable file (directory, permission)
        // is a real error — not a "deleted" side of a diff.
        Err(e) => return Err(format!("failed to read file: {e}")),
    }
    // Same arms again rather than an `unwrap`: the file can go between the
    // stat above and the read here.
    match std::fs::read(abs) {
        Ok(bytes) => Ok(decode_blob(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Side::Missing),
        Err(e) => Err(format!("failed to read file: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A repository with one commit, deterministic identity, and no reliance
    /// on the developer's global git config.
    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for args in [
            vec!["init", "--initial-branch=main"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            run(root, &args).unwrap();
        }
        fs::write(root.join("first.txt"), "one\n").unwrap();
        stage_all(root).unwrap();
        commit(root, "initial", false).unwrap();
        dir
    }

    /// Porcelain status, as the terminal would report it.
    fn porcelain(root: &Path) -> String {
        run(root, &["status", "--porcelain"]).unwrap().stdout
    }

    #[test]
    fn reports_capabilities_when_git_is_available() {
        let dir = repo();
        let caps = capabilities(dir.path());
        assert!(caps.can_mutate);
        assert!(caps.version.is_some_and(|v| v.starts_with("git version")));
        assert!(caps.reason.is_none());
    }

    #[test]
    fn stages_and_unstages_individual_files() {
        let dir = repo();
        let root = dir.path();
        fs::write(root.join("a.txt"), "a\n").unwrap();
        fs::write(root.join("b.txt"), "b\n").unwrap();

        stage(root, &["a.txt".to_string()]).unwrap();
        let status = porcelain(root);
        assert!(status.contains("A  a.txt"), "{status}");
        assert!(status.contains("?? b.txt"), "{status}");

        unstage(root, &["a.txt".to_string()]).unwrap();
        assert!(porcelain(root).contains("?? a.txt"));
    }

    #[test]
    fn stages_a_deletion() {
        // `git add <path>` without `--all` won't record a removal, so a
        // deleted file could never be committed from the UI.
        let dir = repo();
        let root = dir.path();
        fs::remove_file(root.join("first.txt")).unwrap();

        stage(root, &["first.txt".to_string()]).unwrap();
        assert!(porcelain(root).contains("D  first.txt"));
    }

    #[test]
    fn stage_all_then_unstage_all_round_trips() {
        let dir = repo();
        let root = dir.path();
        fs::write(root.join("a.txt"), "a\n").unwrap();

        stage_all(root).unwrap();
        assert!(porcelain(root).contains("A  a.txt"));
        unstage_all(root).unwrap();
        assert!(porcelain(root).contains("?? a.txt"));
    }

    #[test]
    fn commits_staged_work_and_leaves_the_rest_alone() {
        let dir = repo();
        let root = dir.path();
        fs::write(root.join("staged.txt"), "s\n").unwrap();
        fs::write(root.join("loose.txt"), "l\n").unwrap();

        stage(root, &["staged.txt".to_string()]).unwrap();
        commit(root, "add staged", false).unwrap();

        let log = run(root, &["log", "--oneline"]).unwrap().stdout;
        assert!(log.contains("add staged"), "{log}");
        assert!(porcelain(root).contains("?? loose.txt"));
    }

    #[test]
    fn a_message_with_quotes_and_newlines_survives_intact() {
        // The argument-array spawn is what makes this safe; a shell would
        // have mangled or executed part of it.
        let dir = repo();
        let root = dir.path();
        fs::write(root.join("a.txt"), "a\n").unwrap();
        stage_all(root).unwrap();

        let message = "fix: don't \"quote\" me; rm -rf /\n\nBody line";
        commit(root, message, false).unwrap();

        let stored = run(root, &["log", "-1", "--format=%B"]).unwrap().stdout;
        assert!(stored.contains("don't \"quote\" me; rm -rf /"), "{stored}");
        assert!(stored.contains("Body line"), "{stored}");
    }

    #[test]
    fn refuses_an_empty_commit_message_without_invoking_git() {
        let dir = repo();
        assert_eq!(
            commit(dir.path(), "   ", false),
            Err("commit message is empty".into())
        );
    }

    #[test]
    fn amend_rewrites_rather_than_adding_a_commit() {
        let dir = repo();
        let root = dir.path();
        let before = run(root, &["rev-list", "--count", "HEAD"]).unwrap().stdout;

        commit(root, "reworded", true).unwrap();

        let after = run(root, &["rev-list", "--count", "HEAD"]).unwrap().stdout;
        assert_eq!(before.trim(), after.trim(), "commit count must not grow");
        assert!(run(root, &["log", "-1", "--format=%s"])
            .unwrap()
            .stdout
            .contains("reworded"));
    }

    #[test]
    fn lists_creates_and_switches_branches() {
        let dir = repo();
        let root = dir.path();

        let listed = branches(root).unwrap();
        assert_eq!(listed.current.as_deref(), Some("main"));
        assert_eq!(listed.branches, vec!["main".to_string()]);

        create_branch(root, "feature/x").unwrap();
        let listed = branches(root).unwrap();
        assert_eq!(listed.current.as_deref(), Some("feature/x"));
        assert!(listed.branches.contains(&"main".to_string()));

        switch_branch(root, "main").unwrap();
        assert_eq!(branches(root).unwrap().current.as_deref(), Some("main"));
    }

    #[test]
    fn a_detached_head_has_no_current_branch_but_still_lists() {
        let dir = repo();
        let root = dir.path();
        let head = run(root, &["rev-parse", "HEAD"]).unwrap().stdout;
        run(root, &["checkout", "--detach", head.trim()]).unwrap();

        let listed = branches(root).unwrap();
        assert_eq!(listed.current, None, "detached HEAD has no branch");
        assert!(listed.branches.contains(&"main".to_string()));
    }

    #[test]
    fn stash_round_trips_including_untracked_files() {
        let dir = repo();
        let root = dir.path();
        fs::write(root.join("first.txt"), "changed\n").unwrap();
        fs::write(root.join("brand-new.txt"), "new\n").unwrap();

        stash_push(root, Some("wip")).unwrap();
        assert_eq!(porcelain(root).trim(), "", "working tree should be clean");
        assert!(!root.join("brand-new.txt").exists());

        stash_pop(root).unwrap();
        let status = porcelain(root);
        assert!(status.contains("first.txt"), "{status}");
        assert!(
            root.join("brand-new.txt").exists(),
            "an untracked file must come back too, not be left behind"
        );
    }

    #[test]
    fn a_failing_operation_returns_gits_own_message() {
        let dir = repo();
        let error = switch_branch(dir.path(), "does-not-exist").unwrap_err();
        assert!(
            error.contains("does-not-exist"),
            "the toast needs git's detail, not a paraphrase: {error}"
        );
    }

    #[test]
    fn paths_that_escape_the_workspace_are_rejected_before_reaching_git() {
        let dir = repo();
        for bad in ["../outside.txt", "/etc/passwd"] {
            assert!(
                stage(dir.path(), &[bad.to_string()]).is_err(),
                "{bad} should not reach git"
            );
        }
    }

    #[test]
    fn a_file_named_like_a_flag_is_treated_as_a_path() {
        // Without the `--` separator git would read this as an option.
        let dir = repo();
        let root = dir.path();
        fs::write(root.join("-f"), "tricky\n").unwrap();

        stage(root, &["-f".to_string()]).unwrap();
        assert!(porcelain(root).contains("-f"));
    }

    #[test]
    fn file_diff_shows_unstaged_working_tree_against_head() {
        let dir = repo();
        let root = dir.path();
        fs::write(root.join("first.txt"), "edited\n").unwrap();

        let diff = file_diff(root, "first.txt", false).unwrap();
        assert_eq!(diff.baseline.as_deref(), Some("one\n"));
        assert_eq!(diff.current.as_deref(), Some("edited\n"));
        assert_eq!(diff.unavailable, None);
    }

    #[test]
    fn file_diff_staged_shows_index_content_as_current() {
        let dir = repo();
        let root = dir.path();
        // Stage one edit, then change the working tree again so the three
        // sides disagree — staged mode must report the index, not the disk.
        fs::write(root.join("first.txt"), "staged\n").unwrap();
        stage(root, &["first.txt".to_string()]).unwrap();
        fs::write(root.join("first.txt"), "working\n").unwrap();

        let diff = file_diff(root, "first.txt", true).unwrap();
        assert_eq!(diff.baseline.as_deref(), Some("one\n"));
        assert_eq!(diff.current.as_deref(), Some("staged\n"));
        assert_eq!(diff.unavailable, None);
    }

    #[test]
    fn file_diff_added_never_committed_has_no_baseline() {
        let dir = repo();
        let root = dir.path();
        fs::write(root.join("brand-new.txt"), "new\n").unwrap();
        stage(root, &["brand-new.txt".to_string()]).unwrap();

        let diff = file_diff(root, "brand-new.txt", true).unwrap();
        assert_eq!(diff.baseline, None);
        assert_eq!(diff.current.as_deref(), Some("new\n"));
        assert_eq!(diff.unavailable, None);
    }

    #[test]
    fn file_diff_deleted_from_working_tree_has_no_current() {
        let dir = repo();
        let root = dir.path();
        fs::remove_file(root.join("first.txt")).unwrap();

        let diff = file_diff(root, "first.txt", false).unwrap();
        assert_eq!(diff.baseline.as_deref(), Some("one\n"));
        assert_eq!(diff.current, None);
        assert_eq!(diff.unavailable, None);
    }

    #[test]
    fn file_diff_untracked_is_not_tracked() {
        let dir = repo();
        let root = dir.path();
        fs::write(root.join("loose.txt"), "loose\n").unwrap();

        let diff = file_diff(root, "loose.txt", false).unwrap();
        assert_eq!(diff.unavailable, Some(DiffUnavailable::NotTracked));
    }

    #[test]
    fn file_diff_non_repository_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "x\n").unwrap();

        let diff = file_diff(dir.path(), "a.txt", false).unwrap();
        assert_eq!(diff.unavailable, Some(DiffUnavailable::NotARepository));
    }

    #[test]
    fn file_diff_binary_working_tree_is_not_text() {
        let dir = repo();
        let root = dir.path();
        fs::write(root.join("first.txt"), [0x00, 0xff, 0xfe]).unwrap();

        let diff = file_diff(root, "first.txt", false).unwrap();
        assert_eq!(diff.unavailable, Some(DiffUnavailable::NotText));
    }

    #[test]
    fn file_diff_oversized_working_tree_is_too_large() {
        let dir = repo();
        let root = dir.path();
        fs::write(
            root.join("first.txt"),
            "x".repeat(MAX_DIFF_BYTES as usize + 1),
        )
        .unwrap();

        let diff = file_diff(root, "first.txt", false).unwrap();
        assert_eq!(diff.unavailable, Some(DiffUnavailable::TooLarge));
    }

    #[test]
    fn file_diff_oversized_head_blob_is_too_large() {
        // Commit an oversized blob, then shrink the working tree so only the
        // HEAD side is over the cap — proves `cat-file -s` gates the blob.
        let dir = repo();
        let root = dir.path();
        fs::write(
            root.join("first.txt"),
            "x".repeat(MAX_DIFF_BYTES as usize + 1),
        )
        .unwrap();
        stage(root, &["first.txt".to_string()]).unwrap();
        commit(root, "oversized blob", false).unwrap();
        fs::write(root.join("first.txt"), "small\n").unwrap();

        let diff = file_diff(root, "first.txt", false).unwrap();
        assert_eq!(diff.unavailable, Some(DiffUnavailable::TooLarge));
    }
}
