//! Path normalization at the UI <-> backend boundary.
//!
//! Protocol paths are always workspace-relative with forward slashes (see
//! `AGENTS.md`); this module is the single place that crosses between that
//! representation and real OS paths. `resolve_in_workspace` is the guard
//! that stops a malicious or buggy frontend from reading outside the
//! workspace root — treat changes here as security-sensitive.

use std::path::{Path, PathBuf};

/// Absolute path with forward slashes, for display (e.g. `WorkspaceInfo.root`).
///
/// Windows `canonicalize` returns extended-length paths (`\\?\C:\...`), which
/// are correct for filesystem calls but wrong to show a user, so the prefix is
/// stripped here.
/// The current user's home directory on this machine.
///
/// Used as the starting point whenever a path was not given — the folder
/// picker's first listing, and a workspace opened with a blank path. Falling
/// back to the process's *working* directory instead was the obvious shortcut
/// and the wrong one: a daemon's cwd is whatever the thing that spawned it
/// happened to be in, which for an SSH session is a promise nobody made.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|home| home.is_dir())
}

pub fn normalize_absolute(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if let Some(rest) = text.strip_prefix("//?/UNC/") {
        return format!("//{rest}");
    }
    text.strip_prefix("//?/").unwrap_or(&text).to_string()
}

/// Strip `root` from `path` and return a forward-slash relative path.
/// `None` if `path` is not under `root`. The workspace root itself maps to
/// `""`.
pub fn to_workspace_relative(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}

/// Join a frontend-supplied relative path onto `root`, rejecting anything
/// that could escape the workspace: absolute inputs, `..` components,
/// Windows drive/UNC prefixes. An empty string resolves to the root itself.
pub fn resolve_in_workspace(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() {
        return Ok(root.to_path_buf());
    }

    // UNC prefix, e.g. `\\server\share`.
    if relative.starts_with("\\\\") {
        return Err(format!("invalid path: {relative}"));
    }
    // Drive prefix, e.g. `C:\foo` or `C:foo`.
    let bytes = relative.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(format!("invalid path: {relative}"));
    }

    // Split on both separators: the protocol convention is forward slash,
    // but a backslash must not be silently reinterpreted as a separator by
    // the OS later on (Windows) while looking harmless here (Linux).
    let parts: Vec<&str> = relative.split(['/', '\\']).collect();

    if parts.iter().any(|p| p.is_empty()) {
        // Leading/trailing/doubled separators, e.g. "/etc/passwd" or "a//b".
        return Err(format!("invalid path: {relative}"));
    }
    if parts.contains(&"..") {
        return Err(format!("path escapes workspace: {relative}"));
    }

    let mut resolved = root.to_path_buf();
    for part in parts {
        resolved.push(part);
    }

    if !resolved.starts_with(root) {
        return Err(format!("path escapes workspace: {relative}"));
    }

    Ok(resolved)
}

/// Like `resolve_in_workspace`, but for paths whose *contents* are about to
/// be read or handed to the OS. The component checks alone can't see through
/// a symlink: `workspace/link` is a legitimate relative path even when it
/// points at `/etc/shadow`. Resolving the real path and re-checking it
/// against the root closes that, at the cost of requiring the file to exist.
///
/// Both sides are canonicalized before comparing. Canonicalizing only the
/// target would make the check depend on the caller having handed us an
/// already-canonical root: on Windows a short (`RUNNER~1`) or non-verbatim
/// root never prefix-matches a `\\?\`-prefixed target, and every read would
/// be rejected as an escape.
pub fn resolve_existing_in_workspace(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let joined = resolve_in_workspace(root, relative)?;
    let canonical = joined
        .canonicalize()
        .map_err(|e| format!("failed to resolve path: {e}"))?;
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !canonical.starts_with(&canonical_root) {
        return Err(format!("path escapes workspace: {relative}"));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_absolute_uses_forward_slashes() {
        let path = Path::new("/home/user/project");
        assert_eq!(normalize_absolute(path), "/home/user/project");
    }

    #[test]
    fn normalize_absolute_converts_windows_style_backslashes() {
        // Simulate a Windows-style path string; on any platform the output
        // must use forward slashes.
        let path = PathBuf::from("C:\\Users\\dev\\project");
        assert_eq!(normalize_absolute(&path), "C:/Users/dev/project");
    }

    #[test]
    fn normalize_absolute_strips_windows_extended_length_prefix() {
        assert_eq!(
            normalize_absolute(&PathBuf::from("\\\\?\\C:\\Users\\dev\\project")),
            "C:/Users/dev/project"
        );
        assert_eq!(
            normalize_absolute(&PathBuf::from("\\\\?\\UNC\\server\\share\\project")),
            "//server/share/project"
        );
    }

    #[test]
    fn to_workspace_relative_strips_root_with_unix_separators() {
        let root = Path::new("/home/user/project");
        let path = Path::new("/home/user/project/src/main.rs");
        assert_eq!(
            to_workspace_relative(root, path),
            Some("src/main.rs".to_string())
        );
    }

    #[test]
    fn to_workspace_relative_root_itself_is_empty_string() {
        let root = Path::new("/home/user/project");
        assert_eq!(to_workspace_relative(root, root), Some(String::new()));
    }

    #[test]
    fn to_workspace_relative_converts_windows_style_backslashes() {
        let root = Path::new("/home/user/project");
        let path = PathBuf::from("/home/user/project/src\\main.rs");
        assert_eq!(
            to_workspace_relative(root, &path),
            Some("src/main.rs".to_string())
        );
    }

    #[test]
    fn to_workspace_relative_none_when_not_under_root() {
        let root = Path::new("/home/user/project");
        let path = Path::new("/etc/passwd");
        assert_eq!(to_workspace_relative(root, path), None);
    }

    #[test]
    fn resolve_in_workspace_empty_string_is_root() {
        let root = Path::new("/home/user/project");
        assert_eq!(resolve_in_workspace(root, "").unwrap(), root);
    }

    #[test]
    fn resolve_in_workspace_joins_unix_relative_path() {
        let root = Path::new("/home/user/project");
        assert_eq!(
            resolve_in_workspace(root, "src/main.rs").unwrap(),
            root.join("src").join("main.rs")
        );
    }

    #[test]
    fn resolve_in_workspace_joins_windows_style_relative_path() {
        let root = Path::new("/home/user/project");
        assert_eq!(
            resolve_in_workspace(root, "src\\main.rs").unwrap(),
            root.join("src").join("main.rs")
        );
    }

    #[test]
    fn resolve_in_workspace_rejects_dotdot_traversal() {
        let root = Path::new("/home/user/project");
        assert!(resolve_in_workspace(root, "../secret").is_err());
        assert!(resolve_in_workspace(root, "a/../../secret").is_err());
    }

    #[test]
    fn resolve_in_workspace_rejects_dotdot_via_backslash() {
        let root = Path::new("/home/user/project");
        assert!(resolve_in_workspace(root, "a\\..\\..\\secret").is_err());
    }

    #[test]
    fn resolve_in_workspace_rejects_absolute_input() {
        let root = Path::new("/home/user/project");
        assert!(resolve_in_workspace(root, "/etc/passwd").is_err());
    }

    #[test]
    fn resolve_in_workspace_rejects_windows_drive_prefix() {
        let root = Path::new("/home/user/project");
        assert!(resolve_in_workspace(root, "C:\\secret").is_err());
    }

    #[test]
    fn resolve_existing_rejects_a_symlink_escaping_the_workspace() {
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, "classified").unwrap();

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&secret, root.join("link.txt")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&secret, root.join("link.txt")).unwrap();

        // The component checks pass — it's an ordinary relative path.
        assert!(resolve_in_workspace(&root, "link.txt").is_ok());
        // Following it does not.
        assert!(resolve_existing_in_workspace(&root, "link.txt").is_err());
    }

    #[test]
    fn resolve_existing_accepts_a_real_file_inside_the_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

        let resolved = resolve_existing_in_workspace(&root, "src/main.rs").unwrap();
        assert!(resolved.starts_with(&root));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_existing_accepts_a_non_canonical_root() {
        // Reaching the workspace through a symlinked path is the portable
        // stand-in for Windows' short/verbatim path forms: the root the
        // caller holds is not the one `canonicalize` returns, and the check
        // must not read that as an escape.
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().canonicalize().unwrap();
        std::fs::write(real.join("file.txt"), "hi").unwrap();

        let link_root = dir
            .path()
            .parent()
            .unwrap()
            .join(format!("agentlens-link-{}", std::process::id()));
        let _ = std::fs::remove_file(&link_root);
        std::os::unix::fs::symlink(&real, &link_root).unwrap();

        let resolved = resolve_existing_in_workspace(&link_root, "file.txt");
        std::fs::remove_file(&link_root).unwrap();
        assert!(
            resolved.is_ok(),
            "non-canonical root rejected: {resolved:?}"
        );
    }

    #[test]
    fn resolve_existing_errors_on_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        assert!(resolve_existing_in_workspace(&root, "nope.txt").is_err());
    }

    #[test]
    fn resolve_in_workspace_rejects_unc_prefix() {
        let root = Path::new("/home/user/project");
        assert!(resolve_in_workspace(root, "\\\\server\\share\\secret").is_err());
    }
}
