//! Filesystem watcher: notify + debounce, ignore filtering, event fan-out.
//!
//! Every non-ignored directory gets its own non-recursive
//! `notify-debouncer-full` watch. Watching per directory rather than
//! recursively is what keeps `node_modules` and `target` from consuming an OS
//! watch each, and it costs two things: directories appearing later have to be
//! adopted explicitly (`adopt_new_dirs`), and the initial registration needs a
//! walk of the workspace.
//!
//! That walk is why `start` only registers the root before returning and hands
//! the rest to the background thread (`register_descendant_watches`) — on a
//! 100k-file tree it takes seconds cold, and `open_workspace` has under a
//! second to render.
//!
//! Each debounced batch is turned into `FsEvent`s by pure functions
//! (`classify_kind`, `to_fs_event`, `is_ignored`, `coalesce`, `touches_git_dir`)
//! so the interesting logic is testable without a running watcher — only the
//! walking, stat-ing, and emitting functions touch the OS.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::gitstatus;
use crate::ignores::is_extra_ignored;
use crate::paths::{resolve_in_workspace, to_workspace_relative};
use crate::protocol::{
    AgentEvent, AttributedEvent, FsEvent, FsEventKind, GitStatusSnapshot, WatcherState,
    WatcherStatus,
};
use crate::tree::BUILTIN_IGNORED_DIRS;
use crate::visibility::Visibility;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;
use notify::event::ModifyKind;
use notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    new_debouncer, DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache,
};

/// Maximum number of distinct paths emitted per batch; excess is dropped
/// (by first-seen order) rather than flooding the UI.
const MAX_BATCH: usize = 500;

/// Where the watcher's output goes.
///
/// The watcher does not know what a window is. In the desktop app this
/// forwards to Tauri events; running headless it will write protocol messages
/// to stdout instead. Keeping it a trait is what lets the same watcher serve
/// both without a line of conditional code.
///
/// Optional methods default to no-ops so older sinks (and tests) keep
/// compiling when a new event kind is added; the correlator and agent poller
/// only need implementations that care about them.
pub trait EventSink: Send + Sync + 'static {
    fn fs_changes(&self, events: &[FsEvent]);
    fn git_status(&self, snapshot: &GitStatusSnapshot);
    fn watcher_status(&self, status: &WatcherStatus);

    /// Filesystem changes with optional agent attribution. Default is empty
    /// so sinks that only care about the raw feed compile unchanged.
    fn attributed_changes(&self, _events: &[AttributedEvent]) {}

    /// Agent session activity from the background poller. Default is empty
    /// for the same reason: not every sink needs live session state.
    fn agent_events(&self, _events: &[AgentEvent]) {}
}

/// Everything that decides whether a path reaches the feed, kept together so
/// the watch registration and the event filter can't disagree about it.
pub struct Filters {
    /// The workspace's `.gitignore` and `.git/info/exclude`.
    gitignore: Gitignore,
    /// The workspace's own extra globs.
    extra: Gitignore,
    /// What is watched and surfaced despite `.gitignore` — the same rules the
    /// tree renders, so a file the tree shows also glows when it changes. The
    /// built-in list still applies on top, which is what keeps `node_modules`
    /// churn out of the feed regardless.
    visibility: Visibility,
}

impl Filters {
    pub fn new(root: &Path, extra: Gitignore, visibility: Visibility) -> Self {
        Filters {
            gitignore: build_gitignore(root),
            extra,
            visibility,
        }
    }

    /// True if `relative` should be dropped from the feed.
    fn is_ignored(&self, relative: &str, is_dir: bool) -> bool {
        if relative
            .split('/')
            .any(|part| BUILTIN_IGNORED_DIRS.contains(&part))
        {
            return true;
        }
        // The empty path is the workspace root itself. Some platforms report a
        // modify on the parent directory alongside the real change; surfacing
        // it would put a blank row in the feed.
        if relative.is_empty() {
            return true;
        }
        if is_extra_ignored(&self.extra, relative, is_dir) {
            return true;
        }
        if self.visibility.show_ignored || self.visibility.forced(relative) {
            return false;
        }
        self.gitignore
            .matched_path_or_any_parents(relative, is_dir)
            .is_ignore()
    }
}

/// Holds the (at most one) active watch and the last known status. Dropping
/// the `Debouncer` stops the underlying OS watch and its background thread,
/// so replacing or clearing `debouncer` is enough to stop watching.
pub struct WatcherManager {
    debouncer: Mutex<Option<Debouncer<RecommendedWatcher, RecommendedCache>>>,
    status: Mutex<WatcherStatus>,
    /// Bumped on every start and stop. The batch-handling thread remembers
    /// the generation it was spawned with and exits once they differ, so a
    /// batch queued just before a workspace switch can't be emitted against
    /// the workspace that replaced it.
    generation: Arc<AtomicU64>,
}

impl Default for WatcherManager {
    fn default() -> Self {
        WatcherManager {
            debouncer: Mutex::new(None),
            status: Mutex::new(WatcherStatus {
                state: WatcherState::Off,
                message: None,
            }),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl WatcherManager {
    fn set_status(&self, status: WatcherStatus) {
        if let Ok(mut guard) = self.status.lock() {
            *guard = status;
        }
    }
}

/// Current watcher status, for the `watcher_status` command (lets the UI
/// render correctly after a reload without waiting for the next event).
pub fn status(manager: &WatcherManager) -> WatcherStatus {
    manager
        .status
        .lock()
        .map(|g| g.clone())
        .unwrap_or(WatcherStatus {
            state: WatcherState::Error,
            message: Some("watcher state poisoned".to_string()),
        })
}

/// Start watching `root`, replacing (and thus stopping) any previous watch.
/// A workspace that can't be watched must still open and browse, so setup
/// failure here only downgrades the watcher status to `Error` rather than
/// erroring the caller.
///
/// Only the root and `.git` are watched before this returns — two syscalls.
/// Watching every descendant directory means walking the workspace, which on
/// a large repo takes seconds cold, so it happens on the background thread
/// instead; `open_workspace` must not wait for it.
pub fn start(
    sink: &Arc<dyn EventSink>,
    manager: &Arc<WatcherManager>,
    root: &Path,
    filters: Filters,
) {
    let generation = manager.generation.fetch_add(1, Ordering::SeqCst) + 1;
    stop_internal(manager);

    let (tx, rx) = mpsc::channel::<DebounceEventResult>();

    let mut debouncer = match new_debouncer(Duration::from_millis(300), None, tx) {
        Ok(d) => d,
        Err(e) => {
            fail(sink, manager, format!("failed to start watcher: {e}"));
            return;
        }
    };

    // Watch each directory individually instead of asking for one recursive
    // watch on the root. A recursive watch registers an OS watch for every
    // descendant directory including `node_modules` and `target`, which on a
    // large repo can exhaust the per-user inotify limit and take the whole
    // watcher down — and every write inside those directories would wake this
    // process just to be filtered out again.
    if let Err(e) = debouncer.watch(root, RecursiveMode::NonRecursive) {
        fail(sink, manager, format!("failed to watch workspace: {e}"));
        return;
    }
    watch_git_dir(&mut debouncer, root);

    if let Ok(mut guard) = manager.debouncer.lock() {
        *guard = Some(debouncer);
    }

    let sink_for_thread = Arc::clone(sink);
    let manager_for_thread = Arc::clone(manager);
    let root_for_thread = root.to_path_buf();
    let generation_handle = Arc::clone(&manager.generation);
    std::thread::spawn(move || {
        // Everything below the root gets its watch here rather than on the
        // command thread. Until this returns only the root is covered, so a
        // write deep in the tree during the first moments of a session can be
        // missed; root events are not lost, they queue in `rx`.
        register_descendant_watches(&manager_for_thread, &root_for_thread, generation, &filters);

        while let Ok(result) = rx.recv() {
            if generation_handle.load(Ordering::SeqCst) != generation {
                break;
            }
            match result {
                Ok(events) => handle_batch(
                    &sink_for_thread,
                    &manager_for_thread,
                    &root_for_thread,
                    &filters,
                    generation,
                    events,
                ),
                Err(errors) => {
                    for err in errors {
                        eprintln!("agentlens: watcher error: {err}");
                    }
                }
            }
        }
    });

    let running = WatcherStatus {
        state: WatcherState::Running,
        message: None,
    };
    manager.set_status(running.clone());
    sink.watcher_status(&running);
}

/// Stop watching, if a watch is active.
pub fn stop(sink: &Arc<dyn EventSink>, manager: &WatcherManager) {
    manager.generation.fetch_add(1, Ordering::SeqCst);
    stop_internal(manager);
    let off = WatcherStatus {
        state: WatcherState::Off,
        message: None,
    };
    manager.set_status(off.clone());
    sink.watcher_status(&off);
}

fn stop_internal(manager: &WatcherManager) {
    if let Ok(mut guard) = manager.debouncer.lock() {
        *guard = None;
    }
}

fn fail(sink: &Arc<dyn EventSink>, manager: &WatcherManager, message: String) {
    let status = WatcherStatus {
        state: WatcherState::Error,
        message: Some(message),
    };
    manager.set_status(status.clone());
    sink.watcher_status(&status);
}

/// Build a gitignore matcher from the workspace root's `.gitignore` and
/// `.git/info/exclude`. Both are best-effort: a missing or unreadable file
/// just yields fewer globs, never an error. **Limitation:** unlike a walk,
/// this is built once at watch start from only those two files — nested
/// per-directory `.gitignore` files elsewhere in the tree are not honoured.
fn build_gitignore(root: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    let _ = builder.add(root.join(".gitignore"));
    let _ = builder.add(root.join(".git").join("info").join("exclude"));
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// A walk of `from` with ignored subtrees pruned. `.gitignore` files in
/// parent directories are honoured, so this stays correct when walking a
/// subdirectory rather than the workspace root.
fn watchable_walker(from: &Path, show_ignored: bool) -> WalkBuilder {
    let mut builder = WalkBuilder::new(from);
    builder
        .hidden(false)
        .git_ignore(!show_ignored)
        .git_exclude(!show_ignored)
        .git_global(!show_ignored)
        .ignore(!show_ignored)
        .filter_entry(|entry| {
            // Depth 0 is `from` itself: a workspace that happens to be named
            // `target` still has to be walked.
            entry.depth() == 0
                || entry
                    .file_name()
                    .to_str()
                    .map(|name| !BUILTIN_IGNORED_DIRS.contains(&name))
                    .unwrap_or(true)
        });
    builder
}

/// Parallel `watchable_dirs`, for the one-off walk of a whole workspace —
/// roughly twice as fast on a large tree, which directly shrinks the window
/// where descendants are still unwatched. Not used for the incremental path,
/// where spawning a thread pool per newly created directory would cost more
/// than it saves.
fn watchable_dirs_parallel(from: &Path, show_ignored: bool) -> Vec<PathBuf> {
    let collected = Mutex::new(Vec::new());
    watchable_walker(from, show_ignored)
        .build_parallel()
        .run(|| {
            Box::new(|entry| {
                if let Ok(entry) = entry {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        if let Ok(mut guard) = collected.lock() {
                            guard.push(entry.into_path());
                        }
                    }
                }
                ignore::WalkState::Continue
            })
        });
    collected.into_inner().unwrap_or_default()
}

/// `from` plus every descendant directory that isn't ignored — the set of
/// directories that need their own non-recursive watch. Built-in ignores
/// prune whole subtrees, so `node_modules` contributes nothing.
fn watchable_dirs(from: &Path, show_ignored: bool) -> Vec<PathBuf> {
    watchable_walker(from, show_ignored)
        .build()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|entry| entry.into_path())
        .collect()
}

/// Give every non-ignored descendant directory its own watch. Runs once per
/// session on the watcher thread.
///
/// The main walk prunes ignored subtrees, so a pinned directory inside one
/// would never get a watch and its changes would never glow in the tree that
/// is showing it. Each pinned directory therefore gets its own walk, which
/// costs nothing when nothing is pinned.
fn register_descendant_watches(
    manager: &WatcherManager,
    root: &Path,
    generation: u64,
    filters: &Filters,
) {
    let mut dirs = watchable_dirs_parallel(root, filters.visibility.show_ignored);
    if !filters.visibility.show_ignored {
        for pin in &filters.visibility.pinned {
            if let Ok(target) = resolve_in_workspace(root, pin) {
                if target.is_dir() {
                    dirs.extend(watchable_dirs(&target, true));
                }
            }
        }
        // A pin inside a visible subtree overlaps the main walk, and watching
        // the same directory twice would double every event from it.
        dirs.sort();
        dirs.dedup();
    }

    // The workspace can be closed or swapped while the walk is in flight.
    if manager.generation.load(Ordering::SeqCst) != generation {
        return;
    }
    let Ok(mut guard) = manager.debouncer.lock() else {
        return;
    };
    let Some(debouncer) = guard.as_mut() else {
        return;
    };
    for dir in dirs {
        if dir != root {
            // Best-effort: a directory can vanish between the walk and here,
            // and one unwatchable directory must not sink the whole watch.
            let _ = debouncer.watch(&dir, RecursiveMode::NonRecursive);
        }
    }
}

/// Watch the repository's own `.git` directory (that one directory, not its
/// subtree). Its contents never reach the feed, but a write to `.git/index`
/// or `.git/HEAD` is the only signal a `git add` or `git commit` gives — and
/// the status bar has to follow those.
fn watch_git_dir(debouncer: &mut Debouncer<RecommendedWatcher, RecommendedCache>, root: &Path) {
    let git_dir = root.join(".git");
    if git_dir.is_dir() {
        let _ = debouncer.watch(&git_dir, RecursiveMode::NonRecursive);
    }
}

/// Take ownership of directories that just appeared: give each one (and each
/// of its descendants) a watch, and report what was already inside them.
///
/// Both halves are needed because the watches are non-recursive. A new
/// directory stays invisible until it has a watch of its own, and it can
/// arrive already populated — a move, or a branch checkout — in which case
/// that content never produced an event, because the watch landed after it
/// did. Those entries are synthesized as `Created` so the feed matches what
/// the disk actually gained.
fn adopt_new_dirs(
    manager: &WatcherManager,
    root: &Path,
    filters: &Filters,
    batch: &[FsEvent],
    generation: u64,
    at: i64,
) -> Vec<FsEvent> {
    let appeared: Vec<PathBuf> = batch
        .iter()
        .filter(|event| {
            // `is_dir` is stat-based, so the vanished half of a rename is
            // already excluded.
            event.is_dir && matches!(event.kind, FsEventKind::Created | FsEventKind::Renamed)
        })
        .map(|event| root.join(&event.path))
        .collect();
    if appeared.is_empty() {
        return Vec::new();
    }

    // A workspace switch between this batch arriving and now must not graft
    // the old workspace's directories onto the new watch.
    if manager.generation.load(Ordering::SeqCst) != generation {
        return Vec::new();
    }
    let Ok(mut guard) = manager.debouncer.lock() else {
        return Vec::new();
    };
    let Some(debouncer) = guard.as_mut() else {
        return Vec::new();
    };

    let mut adopted = Vec::new();
    for dir in appeared {
        for nested in watchable_dirs(&dir, filters.visibility.show_ignored) {
            let _ = debouncer.watch(&nested, RecursiveMode::NonRecursive);
        }
        adopted.extend(
            existing_entries_under(&dir, filters.visibility.show_ignored)
                .iter()
                .filter_map(|path| to_fs_event(root, path, FsEventKind::Created, path.is_dir(), at))
                .filter(|event| !filters.is_ignored(&event.path, event.is_dir)),
        );
        if adopted.len() >= MAX_BATCH {
            break;
        }
    }
    adopted
}

/// Everything under `from`, excluding `from` itself, with ignored subtrees
/// pruned. Capped, so moving a large directory in can't stall the batch.
fn existing_entries_under(from: &Path, show_ignored: bool) -> Vec<PathBuf> {
    watchable_walker(from, show_ignored)
        .build()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.into_path())
        .filter(|path| path != from)
        .take(MAX_BATCH)
        .collect()
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Map a raw `notify` event kind to our simplified `FsEventKind`. `None` for
/// kinds we don't surface (access, "any", other) — pure, no I/O.
fn classify_kind(kind: EventKind) -> Option<FsEventKind> {
    match kind {
        EventKind::Create(_) => Some(FsEventKind::Created),
        // A rename shows up as a `Modify(Name(..))`; both `From`/`To`/`Both`
        // forms map to `Renamed` — see `map_debounced_event` for how the
        // (possibly two) paths get turned into events.
        EventKind::Modify(ModifyKind::Name(_)) => Some(FsEventKind::Renamed),
        EventKind::Modify(_) => Some(FsEventKind::Modified),
        EventKind::Remove(_) => Some(FsEventKind::Deleted),
        _ => None,
    }
}

/// Build one `FsEvent` from an absolute `path`, or `None` if it's outside
/// the workspace root. Pure — no I/O, just path string logic.
fn to_fs_event(
    root: &Path,
    path: &Path,
    kind: FsEventKind,
    is_dir: bool,
    at: i64,
) -> Option<FsEvent> {
    let relative = to_workspace_relative(root, path)?;
    Some(FsEvent {
        kind,
        path: relative,
        is_dir,
        at,
    })
}

/// Coalesce a batch to at most one event per path (last kind wins — a
/// `Deleted` that arrives after a `Created` for the same path naturally
/// stays `Deleted` since batches are chronologically ordered), preserving
/// first-seen path order, capped at `max`. Pure.
fn coalesce(events: Vec<FsEvent>, max: usize) -> Vec<FsEvent> {
    let mut order: Vec<String> = Vec::new();
    let mut by_path: HashMap<String, FsEvent> = HashMap::new();

    for event in events {
        match by_path.get_mut(&event.path) {
            Some(existing) => {
                existing.kind = event.kind;
                existing.is_dir = event.is_dir;
                existing.at = event.at;
            }
            None => {
                order.push(event.path.clone());
                by_path.insert(event.path.clone(), event);
            }
        }
    }

    order
        .into_iter()
        .take(max)
        .filter_map(|path| by_path.remove(&path))
        .collect()
}

/// Turn one debounced event into zero or more `FsEvent`s. Stats each path
/// for `is_dir` (best-effort — `false` if the path no longer exists), so
/// this touches the filesystem and isn't part of the pure core above.
fn map_debounced_event(root: &Path, event: &DebouncedEvent, at: i64) -> Vec<FsEvent> {
    let Some(kind) = classify_kind(event.kind) else {
        return Vec::new();
    };
    event
        .paths
        .iter()
        .filter_map(|path| {
            let is_dir = fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false);
            to_fs_event(root, path, kind, is_dir, at)
        })
        .collect()
}

/// True if `relative` is inside the repository's own `.git` directory. Those
/// events are kept out of the feed, but they are what a `git add` or
/// `git commit` produces, so they still have to refresh git status.
///
/// Only the top-level `.git` counts: `gitstatus::status` opens the workspace
/// root's repository and nothing else.
fn touches_git_dir(relative: &str) -> bool {
    relative == ".git" || relative.starts_with(".git/")
}

/// Process one debounced batch end to end: map, filter, coalesce, cap, and
/// emit `fs-changes`. Then recompute and emit a full `git-status` snapshot —
/// cheap at debounced batch rates (at most a few per second), and a full
/// snapshot avoids a whole class of delta-merge bugs. This is a deliberate
/// deviation from the phase plan's `git-status-delta`.
///
/// A batch that is empty after filtering — pure `node_modules` churn, say —
/// emits nothing at all, so `npm install` produces zero feed spam and no
/// wasted git reads. A batch that only touched `.git` is the one exception:
/// no feed rows, but git status still has to be re-read, otherwise staging
/// and committing would leave the badges stale until an unrelated edit.
fn handle_batch(
    sink: &Arc<dyn EventSink>,
    manager: &WatcherManager,
    root: &Path,
    filters: &Filters,
    generation: u64,
    events: Vec<DebouncedEvent>,
) {
    let at = now_millis();
    let raw: Vec<FsEvent> = events
        .iter()
        .flat_map(|event| map_debounced_event(root, event, at))
        .collect();
    let git_dir_touched = raw.iter().any(|event| touches_git_dir(&event.path));
    let filtered: Vec<FsEvent> = raw
        .into_iter()
        .filter(|event| !filters.is_ignored(&event.path, event.is_dir))
        .collect();
    let batch = coalesce(filtered, MAX_BATCH);

    if batch.is_empty() && !git_dir_touched {
        return;
    }

    if !batch.is_empty() {
        // Adopting first means content that arrived with a new directory is
        // part of the same emitted batch rather than a second one.
        let adopted = adopt_new_dirs(manager, root, filters, &batch, generation, at);
        let batch = if adopted.is_empty() {
            batch
        } else {
            coalesce([batch, adopted].concat(), MAX_BATCH)
        };
        sink.fs_changes(&batch);
    }

    if let Ok(snapshot) = gitstatus::status(root) {
        sink.git_status(&snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
    use std::fs as stdfs;

    fn fs_event(path: &str, kind: FsEventKind) -> FsEvent {
        FsEvent {
            kind,
            path: path.to_string(),
            is_dir: false,
            at: 0,
        }
    }

    // -- classify_kind --------------------------------------------------

    #[test]
    fn classify_kind_maps_create_modify_remove() {
        assert_eq!(
            classify_kind(EventKind::Create(CreateKind::File)),
            Some(FsEventKind::Created)
        );
        assert_eq!(
            classify_kind(EventKind::Modify(ModifyKind::Data(
                notify::event::DataChange::Content
            ))),
            Some(FsEventKind::Modified)
        );
        assert_eq!(
            classify_kind(EventKind::Remove(RemoveKind::File)),
            Some(FsEventKind::Deleted)
        );
    }

    #[test]
    fn classify_kind_maps_any_rename_mode_to_renamed() {
        for mode in [
            RenameMode::Any,
            RenameMode::From,
            RenameMode::To,
            RenameMode::Both,
        ] {
            assert_eq!(
                classify_kind(EventKind::Modify(ModifyKind::Name(mode))),
                Some(FsEventKind::Renamed)
            );
        }
    }

    #[test]
    fn classify_kind_drops_access_and_other() {
        assert_eq!(
            classify_kind(EventKind::Access(notify::event::AccessKind::Any)),
            None
        );
        assert_eq!(classify_kind(EventKind::Any), None);
        assert_eq!(classify_kind(EventKind::Other), None);
    }

    // -- to_fs_event ------------------------------------------------------

    #[test]
    fn to_fs_event_relativizes_a_path_under_root() {
        let root = Path::new("/workspace");
        let event = to_fs_event(
            root,
            Path::new("/workspace/src/main.rs"),
            FsEventKind::Modified,
            false,
            42,
        );
        assert_eq!(
            event,
            Some(FsEvent {
                kind: FsEventKind::Modified,
                path: "src/main.rs".to_string(),
                is_dir: false,
                at: 42,
            })
        );
    }

    #[test]
    fn to_fs_event_drops_paths_outside_the_root() {
        let root = Path::new("/workspace");
        let event = to_fs_event(
            root,
            Path::new("/elsewhere/file.txt"),
            FsEventKind::Created,
            false,
            0,
        );
        assert_eq!(event, None);
    }

    // -- Filters::is_ignored ------------------------------------------------

    /// Filters for `root` with no extra globs and nothing forced visible.
    fn filters_for(root: &Path, show_ignored: bool) -> Filters {
        Filters::new(
            root,
            Gitignore::empty(),
            Visibility {
                show_ignored,
                show_agent_context: false,
                pinned: Vec::new(),
            },
        )
    }

    #[test]
    fn is_ignored_drops_builtin_dirs_at_any_depth() {
        let dir = tempfile::tempdir().unwrap();
        let f = filters_for(dir.path(), false);
        assert!(f.is_ignored("node_modules/x/y.js", false));
        assert!(f.is_ignored(".git/HEAD", false));
        assert!(f.is_ignored("target/debug/build", true));
        assert!(!f.is_ignored("src/main.rs", false));
    }

    #[test]
    fn builtin_dirs_stay_ignored_even_when_showing_ignored_files() {
        // This is what preserves "an `npm install` produces zero feed spam"
        // regardless of the toggle.
        let dir = tempfile::tempdir().unwrap();
        let f = filters_for(dir.path(), true);
        assert!(f.is_ignored("node_modules/left-pad/index.js", false));
        assert!(f.is_ignored("target/debug/build", true));
        assert!(f.is_ignored(".git/HEAD", false));
    }

    #[test]
    fn is_ignored_drops_the_workspace_root_itself() {
        // A modify reported against the root would otherwise reach the feed
        // as a row with a blank path.
        let dir = tempfile::tempdir().unwrap();
        assert!(filters_for(dir.path(), false).is_ignored("", true));
        assert!(filters_for(dir.path(), true).is_ignored("", true));
    }

    #[test]
    fn is_ignored_honours_workspace_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        stdfs::write(root.join(".gitignore"), "dist/\n*.log\n").unwrap();
        let f = filters_for(root, false);

        assert!(f.is_ignored("dist/bundle.js", false));
        assert!(f.is_ignored("debug.log", false));
        assert!(!f.is_ignored("src/main.rs", false));
    }

    #[test]
    fn show_ignored_lets_gitignored_paths_through() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        stdfs::write(root.join(".gitignore"), "dist/\n*.log\n").unwrap();
        let f = filters_for(root, true);

        assert!(!f.is_ignored("dist/bundle.js", false));
        assert!(!f.is_ignored("debug.log", false));
    }

    #[test]
    fn is_ignored_honours_git_info_exclude() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        stdfs::create_dir_all(root.join(".git").join("info")).unwrap();
        stdfs::write(
            root.join(".git").join("info").join("exclude"),
            "local-only/\n",
        )
        .unwrap();
        let f = filters_for(root, false);

        assert!(f.is_ignored("local-only/scratch.txt", false));
        assert!(!f.is_ignored("src/main.rs", false));
    }

    #[test]
    fn build_gitignore_is_fine_with_no_gitignore_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!filters_for(dir.path(), false).is_ignored("src/main.rs", false));
    }

    #[test]
    fn extra_globs_apply_regardless_of_the_show_ignored_toggle() {
        let dir = tempfile::tempdir().unwrap();
        let extra = crate::ignores::build_matcher(
            dir.path(),
            &crate::protocol::WorkspaceSettings {
                extra_ignores: vec!["*.tmp".to_string(), "vendor/".to_string()],
                ..Default::default()
            },
        );
        for show_ignored in [false, true] {
            let f = Filters::new(
                dir.path(),
                extra.clone(),
                Visibility {
                    show_ignored,
                    show_agent_context: false,
                    pinned: Vec::new(),
                },
            );
            assert!(f.is_ignored("scratch.tmp", false));
            assert!(f.is_ignored("vendor/lib/x.rs", false));
            assert!(!f.is_ignored("src/main.rs", false));
        }
    }

    #[test]
    fn forced_paths_reach_the_feed_despite_gitignore() {
        // A tree that shows the file has to glow when it changes, so the
        // watcher applies the same visibility rules the listing does.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        stdfs::write(root.join(".gitignore"), "AGENTS.md\nnotes/\n").unwrap();

        let f = Filters::new(
            root,
            Gitignore::empty(),
            Visibility {
                show_ignored: false,
                show_agent_context: true,
                pinned: vec!["notes/drafts".to_string()],
            },
        );

        assert!(!f.is_ignored("AGENTS.md", false));
        assert!(!f.is_ignored("notes/drafts/spec.md", false));
        // Everything else under `notes/` stays out of the feed.
        assert!(f.is_ignored("notes/loose-end.md", false));
    }

    // -- touches_git_dir --------------------------------------------------

    #[test]
    fn touches_git_dir_matches_only_the_top_level_git_directory() {
        assert!(touches_git_dir(".git"));
        assert!(touches_git_dir(".git/index"));
        assert!(touches_git_dir(".git/refs/heads/main"));
        assert!(!touches_git_dir("src/.git/index"));
        assert!(!touches_git_dir(".gitignore"));
        assert!(!touches_git_dir("src/main.rs"));
    }

    // -- watchable_dirs -----------------------------------------------------

    /// Workspace-relative, forward-slash names of the walked directories, so
    /// assertions don't depend on the tempdir path.
    fn relative_dirs(root: &Path) -> Vec<String> {
        let mut names: Vec<String> = watchable_dirs(root, false)
            .iter()
            .map(|dir| to_workspace_relative(root, dir).unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn watchable_dirs_prunes_ignored_subtrees_but_keeps_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        stdfs::write(root.join(".gitignore"), "dist/\n").unwrap();
        for nested in [
            "src/lib",
            "node_modules/pkg/deep",
            "target/debug/incremental",
            ".git/refs/heads",
            "dist/assets",
            ".github/workflows",
        ] {
            stdfs::create_dir_all(root.join(nested)).unwrap();
        }

        assert_eq!(
            relative_dirs(root),
            vec![
                "".to_string(),
                ".github".to_string(),
                ".github/workflows".to_string(),
                "src".to_string(),
                "src/lib".to_string(),
            ]
        );
    }

    #[test]
    fn parallel_and_sequential_walks_agree() {
        // The startup path uses the parallel walk and the incremental path
        // the sequential one; they must not disagree about what's watched.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        stdfs::write(root.join(".gitignore"), "dist/\n").unwrap();
        for nested in ["src/a/b/c", "node_modules/pkg", "dist/x", ".github"] {
            stdfs::create_dir_all(root.join(nested)).unwrap();
        }

        let mut sequential = relative_dirs(root);
        let mut parallel: Vec<String> = watchable_dirs_parallel(root, false)
            .iter()
            .map(|d| to_workspace_relative(root, d).unwrap())
            .collect();
        sequential.sort();
        parallel.sort();

        assert_eq!(sequential, parallel);
        assert!(parallel.contains(&"src/a/b/c".to_string()));
    }

    #[test]
    fn watchable_dirs_still_yields_a_root_named_like_an_ignored_dir() {
        // Opening `~/code/target` as the workspace must not prune everything.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("target");
        stdfs::create_dir_all(root.join("src")).unwrap();

        assert_eq!(
            relative_dirs(&root),
            vec!["".to_string(), "src".to_string()]
        );
    }

    // -- coalesce -------------------------------------------------------

    #[test]
    fn coalesce_merges_multiple_modifies_into_one() {
        let events = vec![
            fs_event("a.rs", FsEventKind::Modified),
            fs_event("a.rs", FsEventKind::Modified),
            fs_event("a.rs", FsEventKind::Modified),
        ];
        let result = coalesce(events, 500);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, FsEventKind::Modified);
    }

    #[test]
    fn coalesce_create_then_delete_in_same_batch_is_delete() {
        let events = vec![
            fs_event("a.rs", FsEventKind::Created),
            fs_event("a.rs", FsEventKind::Deleted),
        ];
        let result = coalesce(events, 500);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].kind, FsEventKind::Deleted);
    }

    #[test]
    fn coalesce_preserves_first_seen_path_ordering() {
        let events = vec![
            fs_event("b.rs", FsEventKind::Created),
            fs_event("a.rs", FsEventKind::Created),
            fs_event("b.rs", FsEventKind::Modified),
        ];
        let result = coalesce(events, 500);
        assert_eq!(
            result.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            vec!["b.rs", "a.rs"]
        );
    }

    #[test]
    fn coalesce_caps_the_batch_dropping_excess_by_first_seen_order() {
        let events: Vec<FsEvent> = (0..600)
            .map(|i| fs_event(&format!("file{i}.rs"), FsEventKind::Created))
            .collect();
        let result = coalesce(events, 500);
        assert_eq!(result.len(), 500);
        assert_eq!(result[0].path, "file0.rs");
        assert_eq!(result[499].path, "file499.rs");
    }
}
