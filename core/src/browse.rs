//! Walking a machine's directories to choose a workspace on it.
//!
//! Deliberately separate from `tree`. That module answers "what is in the
//! workspace", in workspace-relative paths, with the user's ignore rules
//! applied — none of which exists yet at the moment somebody is deciding
//! *which* directory to open. So this one deals in absolute paths, has no
//! workspace to be relative to, and hides nothing but the entries it cannot
//! read.
//!
//! It is also the one place a backend looks outside a workspace root. That is
//! the point of it, not a hole: choosing a folder means seeing folders you
//! have not chosen. Reads of file *contents* stay confined to the workspace
//! (see `preview::resolve_for_open`), and this returns names only.

use std::path::{Path, PathBuf};

use crate::paths::normalize_absolute;
use crate::protocol::{BrowseEntry, BrowseListing, CommandResult};

/// Most directories to return for one listing.
///
/// A home directory with ten thousand entries is a real thing, and the picker
/// showing the first thousand of them is no less useful than a picker that
/// takes a second to render all of them.
const MAX_ENTRIES: usize = 1_000;

/// List the directories inside `path`, or inside the home directory when no
/// path is given.
///
/// Directories only: this is a folder picker, and a repository's files are of
/// no help in choosing it. What *is* helpful is knowing which of these
/// candidates are repositories at all, so each entry says.
pub fn list(path: Option<&str>) -> CommandResult<BrowseListing> {
    let start = match path.map(str::trim).filter(|path| !path.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => home_dir().ok_or("this machine has no home directory to start from")?,
    };

    let root = start
        .canonicalize()
        .map_err(|e| format!("cannot open {}: {e}", start.display()))?;
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }

    let mut entries: Vec<BrowseEntry> = std::fs::read_dir(&root)
        .map_err(|e| format!("cannot read {}: {e}", root.display()))?
        // An unreadable entry is skipped rather than failing the listing: one
        // permission-denied directory must not hide its siblings.
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .map(|entry| {
            let path = entry.path();
            BrowseEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_repository: path.join(".git").exists(),
                path: normalize_absolute(&path),
            }
        })
        .collect();

    // Case-insensitively, and with dot-directories after the rest: they are
    // rarely what someone is looking for, but hiding them outright would make
    // a workspace that happens to live in one unreachable.
    entries.sort_by(|a, b| {
        let hidden = a.name.starts_with('.').cmp(&b.name.starts_with('.'));
        hidden.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    let truncated = entries.len() > MAX_ENTRIES;
    entries.truncate(MAX_ENTRIES);

    Ok(BrowseListing {
        path: normalize_absolute(&root),
        parent: root.parent().map(normalize_absolute),
        entries,
        truncated,
    })
}

/// The directory a listing starts from when nothing else is asked for.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|home| home.is_dir())
        .or_else(|| Some(Path::new("/").to_path_buf()).filter(|root| root.is_dir()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_directories_and_leaves_files_out() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("project")).unwrap();
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();

        let listing = list(Some(&dir.path().to_string_lossy())).unwrap();

        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["project"], "a folder picker shows folders");
    }

    #[test]
    fn marks_which_candidates_are_repositories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("repo/.git")).unwrap();
        std::fs::create_dir(dir.path().join("plain")).unwrap();

        let listing = list(Some(&dir.path().to_string_lossy())).unwrap();

        let repo = listing.entries.iter().find(|e| e.name == "repo").unwrap();
        let plain = listing.entries.iter().find(|e| e.name == "plain").unwrap();
        assert!(repo.is_repository);
        assert!(!plain.is_repository);
    }

    #[test]
    fn sorts_case_insensitively_with_dot_directories_last() {
        let dir = tempfile::tempdir().unwrap();
        for name in [".config", "Zebra", "apple", ".cache", "Banana"] {
            std::fs::create_dir(dir.path().join(name)).unwrap();
        }

        let listing = list(Some(&dir.path().to_string_lossy())).unwrap();

        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["apple", "Banana", "Zebra", ".cache", ".config"],
            "hidden directories stay reachable, just not first"
        );
    }

    #[test]
    fn reports_the_parent_so_a_picker_can_go_up() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("child");
        std::fs::create_dir(&child).unwrap();

        let listing = list(Some(&child.to_string_lossy())).unwrap();

        assert_eq!(listing.parent, Some(normalize_absolute(dir.path())));
        assert_eq!(listing.path, normalize_absolute(&child));
    }

    #[test]
    fn paths_are_absolute_so_they_can_be_opened_directly() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("project")).unwrap();

        let listing = list(Some(&dir.path().to_string_lossy())).unwrap();

        let entry = &listing.entries[0];
        assert!(entry.path.ends_with("/project"), "{}", entry.path);
        assert!(entry.path.starts_with('/') || entry.path.contains(':'));
    }

    #[test]
    fn a_missing_or_non_directory_path_explains_itself() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, "x").unwrap();

        assert!(list(Some(&dir.path().join("nope").to_string_lossy())).is_err());
        assert!(list(Some(&file.to_string_lossy())).is_err());
    }

    #[test]
    fn no_path_starts_somewhere_usable() {
        // Whatever the machine's home is — the picker has to open on
        // *something*, and refusing to start is not an option.
        let listing = list(None).unwrap();
        assert!(!listing.path.is_empty());
    }

    #[test]
    fn an_empty_path_is_treated_as_no_path() {
        assert_eq!(list(Some("   ")).unwrap().path, list(None).unwrap().path);
    }
}
