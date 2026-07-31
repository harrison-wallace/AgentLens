# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
