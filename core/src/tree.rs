//! Lazy, gitignore-aware directory listing, plus the flat file index behind
//! the `Ctrl+P` jump.

use std::collections::HashSet;
use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;

use crate::ignores::is_extra_ignored;
use crate::paths::{resolve_in_workspace, to_workspace_relative};
use crate::protocol::{DirEntryNode, PinnedEntry};
use crate::visibility::{is_agent_context, Visibility};

/// Upper bound on the `Ctrl+P` index. A repo larger than this still works;
/// the tail is simply not jumpable, which beats stalling the UI.
const MAX_INDEXED_FILES: usize = 50_000;

/// Directory names filtered out regardless of `.gitignore` contents, on top
/// of which each workspace can add its own globs (see `settings.rs`). Shared
/// with `watcher.rs` so the tree and the watcher can never disagree about
/// what's ignored.
pub(crate) const BUILTIN_IGNORED_DIRS: [&str; 3] = [".git", "node_modules", "target"];

/// One directory listing pass. `honour_gitignore` off means every entry is
/// returned; the built-in ignore list still applies either way.
fn collect_entries(root: &Path, target: &Path, honour_gitignore: bool) -> Vec<DirEntryNode> {
    WalkBuilder::new(target)
        .max_depth(Some(1))
        .hidden(false)
        .git_ignore(honour_gitignore)
        .git_exclude(honour_gitignore)
        .git_global(honour_gitignore)
        .ignore(honour_gitignore)
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
            Some(DirEntryNode {
                name,
                path,
                is_dir,
                ignored: false,
                agent_context: false,
            })
        })
        .collect()
}

/// True if git ignores the directory `relative` itself.
///
/// The two-pass listing below can't see this: a walk started *inside* an
/// ignored directory never evaluates that directory's own rule, so every child
/// comes back looking tracked. Only the `.gitignore` files on the path from
/// the root down to `relative` can decide it — which is exactly what git
/// consults — so those are the only ones read.
///
/// That is one file-open attempt per ancestor, on every expand. Measured at
/// ~60µs for a five-deep path against ~810µs for a single `collect_entries`,
/// of which `list_dir` does two — so this is a few percent of work the call
/// was already doing, not a cost worth caching away. It cannot be skipped when
/// nothing is pinned either: `reloadLoaded` re-lists directories that were
/// expanded before show-ignored was switched off, and those still have to come
/// back correctly flagged.
fn ancestors_ignore(root: &Path, relative: &str) -> bool {
    if relative.is_empty() {
        return false;
    }
    let mut builder = GitignoreBuilder::new(root);
    let _ = builder.add(root.join(".gitignore"));
    let _ = builder.add(root.join(".git").join("info").join("exclude"));
    let mut dir = root.to_path_buf();
    // The target's own `.gitignore` is skipped: a directory can't ignore
    // itself, and its rules are already covered by the walk.
    let parts: Vec<&str> = relative.split('/').collect();
    for part in &parts[..parts.len() - 1] {
        dir.push(part);
        let _ = builder.add(dir.join(".gitignore"));
    }
    builder
        .build()
        .map(|matcher| {
            matcher
                .matched_path_or_any_parents(relative, true)
                .is_ignore()
        })
        .unwrap_or(false)
}

/// List the immediate children of `relative` (workspace-relative) inside
/// `root`, honouring `.gitignore`, the built-in ignore list, and the
/// workspace's extra ignore globs. Directories sort before files; both are
/// sorted by name case-insensitively.
///
/// Entries git ignores are flagged `ignored` and kept only when `show_ignored`
/// is on or `visibility` forces them (a pin, or an agent context file). Which
/// entries those are is worked out by listing the directory twice — once with
/// `.gitignore` applied and once without — rather than re-implementing
/// gitignore matching, which would drift from the walker over nested ignore
/// files. Both passes are one shallow directory read, so the second is cheap.
pub fn list_dir(
    root: &Path,
    relative: &str,
    extra: &Gitignore,
    visibility: &Visibility,
) -> Result<Vec<DirEntryNode>, String> {
    let target = resolve_in_workspace(root, relative)?;

    let visible: HashSet<String> = collect_entries(root, &target, true)
        .into_iter()
        .map(|entry| entry.path)
        .collect();
    // Everything inside an ignored directory is ignored too, whatever the
    // walk started at `target` thinks.
    let inherited = ancestors_ignore(root, relative);
    let mut entries = collect_entries(root, &target, false);
    for entry in &mut entries {
        entry.ignored = inherited || !visible.contains(&entry.path);
        entry.agent_context = visibility.show_agent_context && is_agent_context(&entry.path);
    }
    entries.retain(|entry| {
        !entry.ignored || visibility.show_ignored || visibility.forced(&entry.path)
    });

    // The workspace's own globs are an explicit "hide this", distinct from
    // git's tracking, so they keep applying even when ignored files are shown.
    entries.retain(|entry| !is_extra_ignored(extra, &entry.path, entry.is_dir));

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

/// Every file the jump should reach, workspace-relative. Directories are
/// excluded — you jump to a file.
///
/// The main walk prunes ignored subtrees, which is what keeps this fast on a
/// large repo — and also what would hide a pinned path buried inside one. So
/// the forced roots are collected separately, by walking each one directly:
/// exact, and it costs nothing when nothing is pinned. That is the whole
/// reason `AGENT_CONTEXT_FILES` holds fixed paths rather than basenames.
pub fn list_files(root: &Path, extra: &Gitignore, visibility: &Visibility) -> Vec<String> {
    let mut files: Vec<String> = walk_files(root, root, extra, visibility.show_ignored)
        .take(MAX_INDEXED_FILES)
        .collect();

    // With everything shown already, there is nothing left to force.
    if visibility.show_ignored {
        return files;
    }

    let mut seen: HashSet<String> = files.iter().cloned().collect();
    for forced in visibility.forced_roots() {
        let Ok(target) = resolve_in_workspace(root, forced) else {
            continue;
        };
        if target.is_dir() {
            for path in walk_files(root, &target, extra, true) {
                if seen.insert(path.clone()) {
                    files.push(path);
                }
            }
        } else if target.is_file() && seen.insert(forced.to_string()) {
            files.push(forced.to_string());
        }
    }

    files.truncate(MAX_INDEXED_FILES);
    files
}

/// Files under `from`, workspace-relative to `root`, with the built-in
/// ignores and the workspace's extra globs applied.
fn walk_files<'a>(
    root: &Path,
    from: &Path,
    extra: &'a Gitignore,
    show_ignored: bool,
) -> impl Iterator<Item = String> + 'a {
    let root = root.to_path_buf();
    WalkBuilder::new(from)
        .hidden(false)
        .git_ignore(!show_ignored)
        .git_exclude(!show_ignored)
        .git_global(!show_ignored)
        .ignore(!show_ignored)
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
        .filter_map(move |entry| to_workspace_relative(&root, entry.path()))
        .filter(|path| !is_extra_ignored(extra, path, false))
}

/// Resolve the workspace's pinned paths against the disk, for the tree's
/// Pinned group. Order follows the stored list, and pins whose target is gone
/// are returned marked rather than dropped.
pub fn pinned_entries(root: &Path, visibility: &Visibility) -> Vec<PinnedEntry> {
    visibility
        .pinned
        .iter()
        .map(|path| {
            let resolved = resolve_in_workspace(root, path).ok();
            PinnedEntry {
                path: path.clone(),
                name: path.rsplit('/').next().unwrap_or(path).to_string(),
                is_dir: resolved.as_ref().is_some_and(|p| p.is_dir()),
                exists: resolved.as_ref().is_some_and(|p| p.exists()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ignores::build_matcher;
    use crate::protocol::{AppSettings, WorkspaceSettings};
    use std::fs;

    fn no_extra() -> Gitignore {
        Gitignore::empty()
    }

    fn extra(globs: &[&str]) -> WorkspaceSettings {
        WorkspaceSettings {
            extra_ignores: globs.iter().map(|g| g.to_string()).collect(),
            ..Default::default()
        }
    }

    /// Visibility with nothing forced — plain gitignore behaviour.
    fn plain(show_ignored: bool) -> Visibility {
        Visibility {
            show_ignored,
            show_agent_context: false,
            pinned: Vec::new(),
        }
    }

    fn with_pins(pins: &[&str]) -> Visibility {
        Visibility::new(
            &WorkspaceSettings {
                pinned: pins.iter().map(|p| p.to_string()).collect(),
                ..Default::default()
            },
            &AppSettings {
                show_agent_context: false,
                ..Default::default()
            },
        )
    }

    fn with_agent_context() -> Visibility {
        Visibility::new(&WorkspaceSettings::default(), &AppSettings::default())
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

        let entries = list_dir(root, "", &no_extra(), &plain(false)).unwrap();

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

        let entries = list_dir(root, "subdir", &no_extra(), &plain(false)).unwrap();
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
        let names: Vec<String> = list_dir(root, "", &matcher, &plain(false))
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
        let mut files = list_files(root, &matcher, &plain(false));
        files.sort();

        assert_eq!(
            files,
            vec![".gitignore".to_string(), "src/lib/deep.rs".to_string()]
        );
    }

    #[test]
    fn show_ignored_reveals_gitignored_entries_and_flags_them() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".gitignore"), "notes/\nsecret.env\n").unwrap();
        fs::create_dir(root.join("notes")).unwrap();
        fs::create_dir(root.join("node_modules")).unwrap();
        fs::write(root.join("secret.env"), "").unwrap();
        fs::write(root.join("visible.txt"), "").unwrap();

        let hidden: Vec<String> = list_dir(root, "", &no_extra(), &plain(false))
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(
            hidden,
            vec![".gitignore".to_string(), "visible.txt".to_string()]
        );

        let shown = list_dir(root, "", &no_extra(), &plain(true)).unwrap();
        let flagged: Vec<(String, bool)> =
            shown.iter().map(|e| (e.name.clone(), e.ignored)).collect();
        assert_eq!(
            flagged,
            vec![
                ("notes".to_string(), true),
                (".gitignore".to_string(), false),
                ("secret.env".to_string(), true),
                ("visible.txt".to_string(), false),
            ],
            "node_modules must stay hidden even with show_ignored on"
        );
    }

    #[test]
    fn extra_globs_still_hide_entries_when_show_ignored_is_on() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".gitignore"), "dist/\n").unwrap();
        fs::create_dir(root.join("dist")).unwrap();
        fs::write(root.join("scratch.tmp"), "").unwrap();
        fs::write(root.join("keep.txt"), "").unwrap();

        let matcher = build_matcher(root, &extra(&["*.tmp"]));
        let names: Vec<String> = list_dir(root, "", &matcher, &plain(true))
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();

        // `dist/` is git-ignored so it reappears; `*.tmp` is the user's own
        // explicit hide, so it stays gone.
        assert!(names.contains(&"dist".to_string()));
        assert!(!names.contains(&"scratch.tmp".to_string()));
        assert!(names.contains(&"keep.txt".to_string()));
    }

    #[test]
    fn list_files_includes_ignored_paths_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".gitignore"), "notes/\n").unwrap();
        fs::create_dir(root.join("notes")).unwrap();
        fs::write(root.join("notes/scratch.md"), "").unwrap();
        fs::write(root.join("main.rs"), "").unwrap();

        assert!(
            !list_files(root, &no_extra(), &plain(false)).contains(&"notes/scratch.md".to_string())
        );
        assert!(
            list_files(root, &no_extra(), &plain(true)).contains(&"notes/scratch.md".to_string())
        );
    }

    #[test]
    fn rejects_traversal_outside_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(list_dir(root, "../secret", &no_extra(), &plain(false)).is_err());
    }

    /// A workspace whose agent context files and personal notes are both
    /// gitignored — the case the visibility rules exist for.
    fn agent_workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".gitignore"), "AGENTS.md\nCLAUDE.md\nnotes/\n").unwrap();
        fs::write(root.join("AGENTS.md"), "").unwrap();
        fs::write(root.join("CLAUDE.md"), "").unwrap();
        fs::write(root.join("README.md"), "").unwrap();
        fs::create_dir_all(root.join("notes/drafts")).unwrap();
        fs::write(root.join("notes/scratch.md"), "").unwrap();
        fs::write(root.join("notes/drafts/spec.md"), "").unwrap();
        dir
    }

    #[test]
    fn agent_context_files_survive_gitignore_and_are_marked() {
        let dir = agent_workspace();
        let root = dir.path();

        let entries = list_dir(root, "", &no_extra(), &with_agent_context()).unwrap();
        let shown: Vec<(String, bool, bool)> = entries
            .iter()
            .map(|e| (e.name.clone(), e.ignored, e.agent_context))
            .collect();

        assert!(shown.contains(&("AGENTS.md".to_string(), true, true)));
        assert!(shown.contains(&("CLAUDE.md".to_string(), true, true)));
        assert!(shown.contains(&("README.md".to_string(), false, false)));
        // Only the context files are forced; `notes/` stays hidden.
        assert!(!shown.iter().any(|(name, _, _)| name == "notes"));
    }

    #[test]
    fn turning_off_agent_context_hides_them_again() {
        let dir = agent_workspace();
        let names: Vec<String> = list_dir(dir.path(), "", &no_extra(), &plain(false))
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();

        assert!(!names.contains(&"AGENTS.md".to_string()));
        assert!(names.contains(&"README.md".to_string()));
    }

    #[test]
    fn a_pin_beats_gitignore_for_the_path_its_contents_and_its_ancestors() {
        let dir = agent_workspace();
        let root = dir.path();
        let vis = with_pins(&["notes/drafts"]);

        // The ancestor has to come back, or there is no way to expand to it.
        let top: Vec<String> = list_dir(root, "", &no_extra(), &vis)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(top.contains(&"notes".to_string()));

        // ...but only the pinned branch of it.
        let notes: Vec<String> = list_dir(root, "notes", &no_extra(), &vis)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(notes, vec!["drafts".to_string()]);

        let contents: Vec<String> = list_dir(root, "notes/drafts", &no_extra(), &vis)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(contents, vec!["spec.md".to_string()]);
    }

    #[test]
    fn a_pin_works_under_an_anchored_ignore_inside_a_visible_parent() {
        // An anchored rule inside an otherwise tracked parent: `notes/` is
        // visible, `/notes/drafts/` is not, and the pin targets the latter.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join(".gitignore"), "/notes/drafts/\n").unwrap();
        fs::create_dir_all(root.join("notes/drafts")).unwrap();
        fs::write(root.join("notes/README.md"), "").unwrap();
        fs::write(root.join("notes/drafts/spec.md"), "").unwrap();

        let vis = with_pins(&["notes/drafts"]);
        let notes = list_dir(root, "notes", &no_extra(), &vis).unwrap();
        let flagged: Vec<(String, bool)> =
            notes.iter().map(|e| (e.name.clone(), e.ignored)).collect();
        assert_eq!(
            flagged,
            vec![
                ("drafts".to_string(), true),
                ("README.md".to_string(), false),
            ]
        );

        let drafts: Vec<String> = list_dir(root, "notes/drafts", &no_extra(), &vis)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(drafts, vec!["spec.md".to_string()]);

        assert!(list_files(root, &no_extra(), &vis).contains(&"notes/drafts/spec.md".to_string()));
    }

    #[test]
    fn contents_of_an_ignored_directory_stay_flagged_ignored() {
        // A walk started inside `notes/` never evaluates the `notes/` rule, so
        // without the ancestor check every file in there looks tracked — and
        // would then survive the retain that a pin is supposed to gate.
        let dir = agent_workspace();
        let entries = list_dir(dir.path(), "notes", &no_extra(), &plain(true)).unwrap();

        assert!(!entries.is_empty());
        assert!(entries.iter().all(|entry| entry.ignored));
    }

    #[test]
    fn the_file_index_reaches_pinned_and_agent_context_paths() {
        let dir = agent_workspace();
        let root = dir.path();
        let vis = Visibility::new(
            &WorkspaceSettings {
                pinned: vec!["notes/drafts".to_string()],
                ..Default::default()
            },
            &AppSettings::default(),
        );

        let files = list_files(root, &no_extra(), &vis);
        assert!(files.contains(&"AGENTS.md".to_string()));
        assert!(files.contains(&"notes/drafts/spec.md".to_string()));
        assert!(files.contains(&"README.md".to_string()));
        // Still pruned: nothing forced it.
        assert!(!files.contains(&"notes/scratch.md".to_string()));
        // One entry each, despite the pinned walk overlapping the main one.
        assert_eq!(files.len(), files.iter().collect::<HashSet<_>>().len());
    }

    #[test]
    fn pinned_entries_resolve_against_disk_and_keep_dead_pins() {
        let dir = agent_workspace();
        let entries = pinned_entries(
            dir.path(),
            &with_pins(&["notes/drafts", "README.md", "notes/gone.md"]),
        );

        let got: Vec<(&str, &str, bool, bool)> = entries
            .iter()
            .map(|e| (e.path.as_str(), e.name.as_str(), e.is_dir, e.exists))
            .collect();
        assert_eq!(
            got,
            vec![
                ("notes/drafts", "drafts", true, true),
                ("README.md", "README.md", false, true),
                ("notes/gone.md", "gone.md", false, false),
            ]
        );
    }
}
