# AgentLens

A lightweight, open-source observer for agentic coding. Not an IDE — a
read-only window into what your terminal coding agent (Claude Code, opencode,
…) is doing to a directory: live file tree, change feed, git status,
read-only previews. Runs on Windows and Ubuntu.

**Status:** v0.0.4 — pre-alpha, with the local-observer feature set now in
place and awaiting acceptance testing on both platforms.

What works today:

- **Live file tree** — virtualized, lazy, gitignore-aware, with git status
  badges and a fading highlight on whatever just changed.
- **Activity feed** — filesystem changes grouped per debounced burst, newest
  first; click a row to reveal it in the tree. An `npm install` produces no
  feed spam.
- **Read-only preview** — syntax-highlighted code, images, rendered markdown,
  with a size guard and an "open externally" escape hatch.
- **Diff since session** — what changed in a file since you started watching,
  not merely since the last commit.
- **`Ctrl+P` file jump**, arrow-key tree navigation, resizable and
  collapsible panels, and per-workspace extra ignore globs.

<!-- TODO: screenshot once Phase 1 (local observer) lands -->

## Why not VS Code / a file explorer

AgentLens is not an editor. It doesn't open files for editing and it doesn't
run your agent — it passively watches a directory and correlates filesystem
changes with what your terminal agent is doing, in a window you can leave
open next to your terminal. If you want to edit something, open it in your
real editor; AgentLens stays out of the way.

## Roadmap

| Phase | Description                                                                        | Status      |
| ----- | ---------------------------------------------------------------------------------- | ----------- |
| 0     | Scaffolding — app skeleton, CI on Windows + Ubuntu, release pipeline               | Done        |
| 1     | Local observer MVP — file tree, gitignore-aware watcher, activity feed, git status | In progress |
| 2     | Agent integration — agent transcript tailing, file-event ↔ tool-call correlation   | Planned     |
| 3     | Git actions + polish — stage/commit/branch/stash, diff view, command palette       | Planned     |
| 4     | Remote — headless daemon for WSL-from-Windows and SSH                              | Planned     |

## Install

Download the installer for your platform from
[Releases](https://github.com/harrison-wallace/AgentLens/releases). Pre-1.0
Windows installers are unsigned, so SmartScreen will warn before you can run
them — this is expected until code signing lands (see the phase plans).

## Build from source

Prerequisites:

- Node (version pinned in [`.nvmrc`](.nvmrc))
- Rust stable (pinned via [`rust-toolchain.toml`](rust-toolchain.toml))
- On Ubuntu, the Tauri v2 system packages: `libwebkit2gtk-4.1-dev
libayatana-appindicator3-dev librsvg2-dev patchelf build-essential curl wget
file libxdo-dev libssl-dev`

```sh
npm install
npm run tauri dev    # run locally
npm run tauri build  # produce an installer
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for dev setup, required checks, and
the PR flow.

## License

[MIT](LICENSE)
