# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.4] - 2026-08-01

### Changed

- Releases no longer include an `.rpm`. AgentLens targets Windows and Ubuntu,
  so the RPM was an untested package for a platform the project doesn't
  claim to support. Bundle targets are now stated explicitly (`deb`,
  `appimage`, `nsis`, `msi`) instead of `"all"`.
- CI builds with `--no-bundle`. It only needs to prove the app compiles and
  links, and it discarded the installers it was producing — bundling them
  cost roughly 15 minutes per run on Linux, where the RPM bundler gzips the
  225 MB debug binary at about 250 KB/s. Installers still come from
  `release.yml` on every tag.

## [0.0.3] - 2026-08-01

Completes the local-observer feature set. Acceptance testing on Windows and
Ubuntu against a large repository is still outstanding, which is why this is a
patch release and not `0.1.0`.

### Added

- Filesystem watcher (`notify` + 300 ms debounce) feeding an activity feed:
  changes arrive as one batch per burst, newest first, grouped under a
  relative-time header, with click-to-reveal in the tree. Ignored churn such
  as `npm install` produces no feed entries at all.
- Read-only preview pane — syntax-highlighted code (Shiki, grammars loaded on
  demand), images, and rendered markdown, with a 2 MB size guard, a binary
  guard, and an "open externally" hand-off to the OS.
- "Diff since session" tab showing what changed in a file since watching
  started, rather than since the last commit. Files already dirty when the
  session began are baselined against their content at that moment, so
  pre-existing work isn't misattributed to the agent.
- `Ctrl+P` fuzzy file jump over a gitignore-aware index, and arrow-key
  navigation in the tree (expand, collapse, step to parent, preview).
- Per-workspace extra ignore globs, persisted and applied consistently to the
  tree, the file index, and the feed.
- Resizable, collapsible tree/preview/feed panels with persisted widths, and a
  "Clear" action that re-baselines the session.
- Tree rows glow briefly when a file changes, and the status bar reports live
  watcher state.

### Changed

- The watcher registers a non-recursive watch per directory instead of one
  recursive watch on the root. A recursive watch registers an OS watch for
  every descendant including `node_modules`, which can exhaust the inotify
  limit on a large repository and take the whole watcher down.
- Watch registration happens on a background thread. Measured cold on a
  100k-file tree the walk takes ~2.6 s, far too long to hold up opening a
  workspace, so only the root is watched before the window is usable.
- `.gitignore` is now honoured in directories that aren't git repositories,
  where the tree previously ignored it while the watcher did not.

### Fixed

- Staging or committing left the git badges and status-bar counts stale:
  those operations touch only `.git`, which is filtered from the feed, so
  nothing triggered a re-read.
- A filesystem event reported against the workspace root produced a blank row
  in the activity feed.
- A batch queued just before a workspace switch could be emitted against the
  workspace that replaced it, including overwriting its git status.
- Previewing a file resolved symlinks only after the containment check, so a
  link planted inside the workspace could read a file outside it.
- Reveal-in-tree and the file jump highlighted the target row without
  scrolling to it, so nothing visibly happened when it was off-screen.
- Large diffs and large files are capped before rendering; a rewritten file or
  a sizeable `package-lock.json` previously froze the preview pane.
- The file tree rebuilt its entire row list on every scroll frame.

## [0.0.2] - 2026-07-31

### Added

- Workspace open/close with a native folder picker, plus a recent-workspaces
  list persisted across restarts — the app now has something to observe.
- Lazy, gitignore-aware directory listing (`ignore` crate) with `.git`,
  `node_modules`, and `target` filtered at the source, so large repos don't
  flood the tree.
- Virtualized file tree (`@tanstack/react-virtual`) with expand/collapse and
  per-file git status badges, a workspace header with Refresh/Close, and a
  status bar showing the branch and M/A/D/? counts.
- Git status via `git2`, mapped so worktree changes win over index changes —
  that's what a file's badge should reflect.
- Path normalization at the protocol boundary, including a guard that rejects
  `..` traversal, absolute paths, and Windows drive/UNC prefixes so the
  frontend cannot read outside the open workspace.
- `scripts/update-latest-ubuntu.sh` to install or update the newest release
  build on an Ubuntu test machine.

### Changed

- The window is no longer an empty placeholder: with no workspace open it now
  offers "Open folder" and the recent list.
- Ubuntu CI/release dependencies drop `libappindicator3-dev`, which conflicts
  with `libayatana-appindicator3-dev` and broke the Ubuntu build on noble.

## [0.0.1] - 2026-07-31

### Added

- Tauri v2 + React/TypeScript application skeleton (Vite, strict TS, Tailwind
  dark-theme tokens) opening an empty window titled with the app version.
- `protocol.rs` / `protocol.ts` boundary convention for all UI↔backend types.
- GitHub Actions CI matrix (`windows-latest` + `ubuntu-latest`): lint,
  typecheck, tests, `cargo fmt`/`clippy`, and a debug `tauri build`.
- GitHub Actions release pipeline (`tauri-action`) that builds installers
  from a `v*` tag into a draft release.
- Contributor docs: `LICENSE` (MIT), `CONTRIBUTING.md`, issue templates.
