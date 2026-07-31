# Contributing to AgentLens

## Dev setup

Prerequisites:

- Node (version pinned in [`.nvmrc`](.nvmrc))
- Rust stable (pinned via [`rust-toolchain.toml`](rust-toolchain.toml)), with
  `rustfmt` and `clippy` components
- **Ubuntu only:** `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev
librsvg2-dev patchelf build-essential curl wget file libxdo-dev libssl-dev`
- **Windows:** the [Tauri v2 prerequisites](https://tauri.app/start/prerequisites/)
  (WebView2, MSVC build tools)

```sh
npm install
npm run tauri dev
```

## Repository layout

```
src/                    # React + TS frontend (Vite)
├── components/         # shared UI
└── lib/                # Tauri invoke/event wrappers, typed protocol mirror
src-tauri/
├── src/
│   ├── main.rs
│   ├── protocol.rs      # serializable UI<->backend message types
│   └── lib.rs
├── Cargo.toml
└── tauri.conf.json
```

## The protocol rule

Everything crossing the UI↔backend boundary is a serde-serializable type
defined in `src-tauri/src/protocol.rs`, mirrored in `src/lib/protocol.ts`.
Never invoke ad-hoc shapes across that boundary — this is what lets phase 4
swap the in-process backend for a remote daemon without touching the
frontend.

## Required checks

These all run in CI and must pass before merge:

```sh
npm run lint
npm run typecheck
npm test
cargo fmt --all --check   # from src-tauri/
cargo clippy --all-targets --all-features -- -D warnings   # from src-tauri/
cargo test                # from src-tauri/
```

## PR flow

1. Branch off `main`.
2. Open a PR. CI must be green (both `ubuntu-latest` and `windows-latest`).
3. No direct pushes to `main`.

Use [Conventional Commits](https://www.conventionalcommits.org/) for commit
messages, e.g.:

```
feat: add debounced fs watcher
fix: correct git status badge for renamed files
docs: update README roadmap table
chore: bump tauri to 2.x
```

## Cutting a release

1. Bump the version in `package.json`, `src-tauri/Cargo.toml`,
   `src-tauri/tauri.conf.json`, and the README status line — all four must
   match.
2. Update `CHANGELOG.md` (Keep a Changelog format).
3. `git tag vX.Y.Z && git push --tags`.
4. `release.yml` builds installers for Windows and Ubuntu and opens a draft
   GitHub release — review and publish it manually.
