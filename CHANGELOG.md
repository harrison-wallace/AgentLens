# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0] - 2026-08-07

### Added

- **Diff against `HEAD` and against the index.** The preview's diff view now
  offers three sources instead of one: since the session started, working tree
  vs `HEAD`, and staged vs `HEAD`. All three share the existing renderer, so
  hunk collapsing and the large-diff cap apply to each. Every side is read
  through `git cat-file -s` first and capped at 1 MiB, matching the session
  snapshot limit — a large tracked file reports itself as too large instead of
  shipping megabytes down the protocol. Binary content and untracked paths say
  what they are rather than rendering as a whole-file addition.
- **Command palette and a shortcuts help page.** `Ctrl+Shift+P` opens a fuzzy
  command palette over one flat registry (file, view, git, session commands);
  `F1` or `Ctrl+/` opens a shortcuts reference generated from that same
  registry, so the documented keys cannot drift from the real ones. `Ctrl+P`
  remains the file jump, and both modes share the existing quick-pick widget.
- **Light theme, with an option to follow the OS.** Selectable from Settings
  (System / Dark / Light). The theme is resolved before first paint by a small
  inline script, so switching does not flash the old colours, and syntax
  highlighting carries both ramps at once rather than re-highlighting open
  files on a change.
- **Non-modal error toasts.** Failures that previously had nowhere to go — a
  workspace that would not open, a settings save that failed with the panel
  closed — now surface as dismissible toasts with a copyable detail drawer.
  Never a dialog, and never a timer that removes an error before it is read.
- **Window size and position persist** across restarts, alongside the panel
  layout that already did.
- **Notify-only update check.** At startup AgentLens asks GitHub whether a
  newer release exists and, if so, says so once with a link. Nothing is ever
  downloaded or installed, every failure (offline, rate-limited, unparseable)
  is silent, and the whole thing switches off in Settings.

### Changed

- `DiffUnavailable` gained `notText` and `tooLarge`, and its wording is no
  longer session-specific now that git diffs share the type.
- The window no longer pins itself to the dark theme in `tauri.conf.json`, so
  the runtime theme setting owns the title bar.

### Fixed

- The branch picker's open state moved into a store, so the palette's
  "Switch branch…" opens the same widget the status bar does rather than a
  second copy of it.

## [0.5.1] - 2026-08-02

### Fixed

- **WSL/SSH auto-install of the daemon no longer dies with empty paths**
  (`mkdir: cannot create directory ''`, `grep: ".sums"`, empty `sha256sum`).
  Multi-line install/bootstrap scripts are base64-packed before they cross
  `wsl.exe` / `ssh`, so `$HOME` / `$TMP` are not expanded or stripped on the
  Windows side; install vars use `al_*` names to avoid Windows-imported `TMP`.
  Affects every Linux WSL distro and SSH host, not only Debian.
- **Packed scripts keep protocol stdio.** Decode to a temp file and `exec sh`
  on it rather than piping into `sh`, so the daemon still inherits the pipe
  from `wsl.exe` / `ssh` (piping stole stdin and caused "lost the connection").

## [0.5.0] - 2026-08-02

VS Code-style preview tabs so several files stay open while watching an agent.

### Added

- **Preview tabs** — open multiple files in the preview pane. Single-click /
  arrow opens a preview tab (italic, replaced by the next); double-click or
  Enter keeps it permanent. Feed, git, pin, and `Ctrl+P` jumps open permanent
  tabs. `Ctrl+Tab` / `Ctrl+Shift+Tab` cycle, `Ctrl+W` closes, middle-click
  closes a tab. Current / Diff remain modes of the active file.
- **Open-set persistence** — tabs (and active file / mode) are remembered per
  workspace location (`local` path, `wsl://…`, `ssh://…`), not bare root alone.
- **Tree open dots** — a small accent mark on files that have a tab open.
- **Payload cache** — switching tabs reuses the last loaded content instead of
  re-fetching every time.
- **App screenshot** updated (`docs/images/v0-5-0.png`).

### Fixed

- `Ctrl+W` / `Ctrl+Tab` no longer fire while typing in inputs (commit box,
  settings, etc.).

## [0.4.0] - 2026-08-02

Zoom, a type-to-filter branch picker, and tree decorations that surface changes
hidden under collapsed directories.

### Added

- **Interface zoom** — scales the whole webview (`Ctrl` `+`/`−`/`0`, Settings,
  status-bar chip when not 100%). Discrete steps from 80% to 200%, persisted
  per display in `localStorage`.
- **Preview text size** — independent of zoom: code, diffs and markdown in the
  preview pane only (Settings → All workspaces).
- **Branch picker** — type-to-filter checkout in the same QuickPick overlay as
  file jump; create a branch by typing a new name; stash / pop in the footer.
- **Stash and switch** — when a checkout fails with a dirty tree, the git
  error offers one-click stash → switch (and restores the tree if the switch
  still fails).
- **Collapsed-tree roll-ups** — recently-changed counts and git status counts
  bubble up onto the deepest visible ancestor so collapsed directories still
  show that work is happening underneath.
- **Activity feed name-first rows** — filename first, directory truncated from
  the left, so deep sibling paths stay distinguishable in a narrow panel.
- **Lens logo** — app icon set, empty-state and header mark, README branding
  (CC0 asset with provenance recorded).

### Fixed

- A failed recovery `stashPop` after stash-and-switch no longer overwrites its
  own error with the earlier switch failure.
- QuickPick cursor stays in range when the result list shrinks without a
  query change.

## [0.3.1] - 2026-08-02

### Added

- **App screenshot** in the README (`docs/images/v0-3-0.png`).

### Changed

- README: document `F11` fullscreen, link [DESIGN.md](DESIGN.md) from Architecture.

## [0.3.0] - 2026-08-02

Terminal-native UI redesign, a controllable activity feed, and session +/− in
the status bar.

### Added

- **Design system (`DESIGN.md`)** — dark TUI dialect: JetBrains Mono, warm
  near-black canvas, hairline panes, single-letter kind badges, semantic color
  only for change kinds.
- **Activity feed filter and sort** — toolbar stats (`+ M − →`) filter by kind;
  click the sort control to cycle `time` / `most +` / `most −` / `most`.
- **Session +/− in the footer** — running totals of files created and deleted
  since watching started (or last Clear), separate from git working-tree
  counts.
- **Configurable feed length** — default 250 batches (was 100); set 50–2000
  under Settings → All workspaces → Activity feed length. Session totals are
  not capped by the list.
- **F11 fullscreen** — toggles the native window (Tauri is not a browser tab).
- **Pin button clearance** — scrollbar gutter and row padding so the tree pin
  control is not covered when the scrollbar shows.

### Changed

- App chrome is mono-first at 12px density; markdown prose stays proportional.
- Git untracked badge is `?` (aligned with the status bar).
- Batch headers use compact stats (`M 3  + 1`) instead of English summaries.

## [0.2.1] - 2026-08-02

Three bugs that between them made a first remote connection to a machine with
an older daemon on it fail in three different confusing ways.

### Fixed

- **A command the daemon didn't recognise hung for thirty seconds instead of
  failing.** Deserializing a request fails as a unit, so an unknown command
  took the request's id down with it and the reply came back against id 0 —
  which nobody was waiting for. The app then waited out its full timeout for
  an answer it had already been sent. The id is now recovered before the
  command is parsed. This is what made `docs/PROTOCOL.md`'s promise that new
  commands are a safe additive change actually true; it wasn't.
- **A daemon of the wrong version was preferred over installing the right
  one.** The bootstrap ran the first executable it found, so a copy left in
  `~/.local/bin` by an earlier release won every connection from then on — and
  because finding it counted as success, the correct version was never
  installed. Worse, it looked fine: an older daemon speaks the same protocol,
  so it hand-shakes happily and only fails on the first newer command.
  Candidates outside the directory AgentLens manages now have to match the
  app's version before they are run.
- **Opening a workspace with a blank path used the wrong directory.** It
  resolved to the backend process's working directory, which for a daemon is
  wherever the thing that spawned it happened to be — not the home directory
  the UI promises. Blank now means home on whichever machine the backend is.
- **Opening a filesystem root took the app down with it.** The watcher
  registers one OS watch per directory, so `/` meant walking every
  pseudo-filesystem on the machine and exhausting the kernel's watch limit. A
  root is now refused with an explanation; it was never a project.

## [0.2.0] - 2026-08-02

Remote machines now set themselves up, and you can browse them to find the
folder you want.

### Added

- **Nothing to install on the remote machine.** Open a workspace on a WSL
  distro or SSH host that has never run AgentLens and the app puts the observer
  there itself: it downloads the matching binary on the remote, verifies it
  against the release's `SHA256SUMS`, installs it under
  `~/.agentlens/bin/<version>/`, and prunes older versions once the new one
  works. No `sudo`, nothing outside your own home directory, and a status-bar
  _installing_ state while it happens. Turn it off with **Settings → Remote →
  Set up remote machines automatically**.
- **A folder browser for machines you aren't sitting at.** There is no OS file
  dialog for a WSL distro or an SSH host, so **Browse…** lists the remote's own
  directories a level at a time, starting at your home directory and marking
  which candidates are git repositories. Typing the path still works and is
  faster when you know it.
- `SHA256SUMS` is published alongside the daemon binaries, so a remote
  installing its own copy can check it got what was built.

### Changed

- **The app no longer depends on the remote `PATH`.** It runs a small bootstrap
  that looks for the daemon where AgentLens installs it, then `~/.local/bin`,
  `/usr/local/bin`, `/usr/bin`, then `PATH` — so a hand-placed binary in any of
  those is found without configuration. **Daemon command** is now the escape
  hatch rather than the fix, and naming one runs exactly that, skipping the
  search and the automatic install.
- While connected to another machine, **Open folder** browses _that_ machine.
  It previously opened a local file dialog, and picking anything from it
  silently dropped the connection.

### Fixed

- The daemon could exit between reading a command and writing its answer.
  Requests are handled off-thread, so stdin closing did not mean the work was
  done — to the app that looked like a hang until the 30-second timeout, and it
  happened whenever a connection closed just after a command. Shutdown now
  drains what is in flight.
- A failed handshake could miss the remote's explanation. The handshake fails
  when stdout closes, with no ordering against stderr having been read, so the
  line saying _why_ was sometimes lost — and with it the app's ability to tell
  "no daemon there" from "the connection broke". It now waits for stderr to
  finish first.
- A host or distro name beginning with `-` is refused rather than passed to
  `ssh`, which has no `--` to end option parsing and would have read
  `-oProxyCommand=…` as an instruction.
- Killing a remote connection no longer waits on a write that a wedged daemon
  is refusing to read, which could hang the app on exit.
- A half-open connection attempt no longer leaves its process running; a few
  failed reconnects would otherwise strand a handful of `ssh` sessions.

## [0.1.0] - 2026-08-02

The MVP: AgentLens now works when the files are on another machine.

### Added

- **Remote workspaces over WSL and SSH.** Open a directory inside a WSL distro
  from the Windows app, or on a Linux box over SSH, and get the same file
  tree, activity feed, git decorations, git actions, previews,
  diff-since-session and agent tailing as a local workspace. Agent transcripts
  are read from the _remote_ `~/.claude`, which is where an agent running
  there actually writes them.
- **`agentlens-daemon`** — a single dependency-free binary that runs where the
  files are and speaks newline-delimited JSON over stdio. Published for
  linux-x86_64, linux-aarch64 and windows-x86_64 alongside the installers.
  Watching a remote filesystem is not possible: the 9P bridge behind `\\wsl$`
  does not propagate inotify events and SFTP has no events at all, so the
  observer has to move rather than the UI.
- stdio rather than a socket, which means no listening ports, no tunnels and
  no firewall rules — and over SSH, the system `ssh` binary does the
  authenticating, so `~/.ssh/config` aliases, agent forwarding, jump hosts and
  2FA behave exactly as they do in your terminal.
- **Open in WSL… / Open over SSH…** on the start screen. A remote workspace is
  recorded in Recent as a location (`wsl://Ubuntu/home/you/project`,
  `ssh://build-box/srv/app`), so reopening it later is one click and
  reconnects on the way.
- **Reconnection with visible gaps.** A dropped link is reported in the status
  bar and retried with backoff; the activity feed marks the window it was
  blind for (`disconnected 14:02–14:03`) rather than silently resuming, since
  "nothing happened" is the one thing it cannot know about that window. A
  reconnected daemon is a fresh process, so settings and the open workspace
  are re-applied before the link is declared healthy.
- A **Daemon command** setting, for the case that trips everyone up once:
  `ssh host command` runs without a login shell, so `~/.local/bin` is usually
  not on `PATH` and a correctly installed daemon reports "command not found".
- A protocol handshake with an explicit version, so an app and a daemon from
  different releases fail with a message naming both versions instead of
  half-working. `docs/PROTOCOL.md` records the wire format and what may change
  without a version bump; `docs/REMOTE.md` is the setup guide.
- CI gained a job that installs the daemon into a real WSL distro and asserts
  that a file edited _inside_ the distro reaches the app.

### Changed

- The crate is now a Cargo workspace: `core/` (watcher, git, previews,
  snapshots, agents, protocol — no Tauri), `daemon/`, and `src-tauri/` (the
  desktop app, settings persistence and transport choice). Every operation
  goes through one `Command` enum answered by one engine, so the local and
  remote paths cannot drift apart. Local workspaces behave as they did.
- Per-workspace settings are keyed by location rather than by path, so
  `/srv/app` on two different SSH hosts are two different workspaces. Existing
  local entries are unaffected — a local location _is_ its path.
- Build output moved to `target/` at the repo root, following the workspace.
- README roadmap corrected: phase 3 (git actions) has been feature-complete
  since 0.0.8 while still reading "planned".

## [0.0.8] - 2026-08-01

### Added

- **Git actions** — the first release that writes to your repository. Stage
  and unstage individual files or everything, commit (with amend), switch and
  create branches, stash and pop. A source-control panel under the tree shows
  staged and unstaged work with per-file actions; a branch control in the
  status bar handles the rest.
- Mutations run through the `git` CLI rather than libgit2, so your hooks run,
  your config is honoured, and the resulting commits are identical to ones
  made in the terminal. Reads stay on libgit2, which is faster and needs none
  of that.
- Git's own error text is shown verbatim in a dismissible strip rather than
  paraphrased into a dialog — a failed branch switch tells you exactly what
  git told it.
- Git actions are hidden with an explanation when `git` isn't on `PATH`,
  instead of offering buttons that fail when pressed.

### Fixed

- A file that was staged and then edited again appeared only under unstaged
  changes, so the commit box reported nothing to commit and disabled itself —
  while `git commit` would have succeeded and committed the staged version.
  Staged and unstaged work are now tracked as the two separate things git has
  always considered them. This was the likeliest path to hit, since an agent
  edits files it has already staged constantly.
- The status bar counted a partially-staged file twice, overstating how much
  had changed.

### Changed

- Git mutations are serialized. Two `git` processes writing one repository can
  collide on the index lock, and a slow hook makes the overlap easy to hit.

## [0.0.7] - 2026-08-01

### Added

- Agent session discovery and tailing for Claude Code — the groundwork for
  attributing file changes to the tool call that made them. Sessions are found
  by mapping the workspace to its transcript directory and confirming the
  `cwd` recorded inside, then read incrementally from a byte offset, tolerant
  of partial lines, unknown record types and malformed JSON. Nothing is shown
  in the UI yet; correlation and the session panel come next.
- Multiple agent profiles are supported. A profile is a separate config
  directory with its own login and history, and a machine commonly has
  several. Reading `CLAUDE_CONFIG_DIR` alone would not have worked: launched
  from a desktop icon the app inherits no such variable and would only ever
  have seen the default profile — never the one the work is actually in.
- An **Agent session folders** setting, listing every directory being searched
  with the agent that recognises it, and adding your own for a layout the app
  can't guess. Detection is necessarily a guess — agents don't promise where
  they store sessions, and profile naming is a habit rather than a standard —
  so the list makes "no agent detected" diagnosable, and a mistyped path shows
  as "not recognised" instead of silently doing nothing.

### Changed

- README rewritten: CI, release, version, licence and stack badges; a
  configuration reference for all five settings; an architecture section
  covering the Rust/React protocol boundary; and a corrected roadmap — phase 1
  has been feature-complete for several releases while still reading "in
  progress".

## [0.0.6] - 2026-08-01

### Added

- Agent context files — `AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `.cursorrules`,
  `.clinerules`, `.github/copilot-instructions.md` — are now surfaced and
  marked in the tree even when `.gitignore` hides them, with no configuration.
  These files are the agent's instructions, so an observer that hides them by
  default has the inversion backwards. On by default, and app-level rather
  than per-workspace, since "always show me `AGENTS.md`" describes how you
  work rather than one repo.
- Pinned paths: pin any file or directory from its tree row (hover icon, or
  `p` on the focused row) to keep it visible whatever `.gitignore` says. A
  pinned directory brings its contents and its ancestors with it, so a pin
  buried inside an ignored subtree is still reachable. Pins appear in a group
  above the tree, are listed in settings for bulk editing, and survive
  reopening the workspace. Pins whose target has been renamed or deleted are
  shown struck through rather than silently dropped.
- An app-level settings scope, persisted alongside the per-workspace one, for
  settings that outlive any single workspace.

### Changed

- Settings is now one modal with two labelled sections — _This workspace_ and
  _All workspaces_ — so the scope of each setting is visible rather than
  implied. Toggles apply immediately and text fields on blur; the Save and
  Cancel buttons are gone, and Esc or the close button dismisses.
- The activity feed and the tree glow now follow the same visibility rules as
  the listing, so editing a gitignored `AGENTS.md` or a pinned file registers
  as a change instead of passing silently. Pinned directories inside ignored
  subtrees get their own watch registration.
- Tree rows are now `role="treeitem"` inside a `role="tree"` container, with
  `aria-level`, `aria-expanded` and `aria-selected`. Rows were `<button>`
  elements, which cannot legally contain the pin button.

### Fixed

- Contents of a git-ignored directory were reported as tracked. A walk started
  inside an ignored directory never evaluates that directory's own rule, so
  expanding one with show-ignored on rendered every file as if git were
  tracking it. The ignore state of the ancestor chain is now resolved the way
  git resolves it.
- Two settings saved at once could overwrite each other. Committing a text
  field on blur and clicking a toggle in the same gesture fired two writes,
  the second built from state the first had not yet updated, silently
  discarding the typed value. Writes are now serialized and each builds on
  what the previous one stored.
- A text field in settings could be cleared mid-edit by an unrelated save
  landing, discarding keystrokes.

## [0.0.5] - 2026-08-01

### Added

- A header toggle for showing git-ignored files. `.gitignore` marks both
  build noise and personal-but-important files — planning docs, agent
  context files — and the app previously treated both as uninteresting, so
  there was no way to see the latter at all. Ignored entries appear dimmed
  and italic, in the tree and the `Ctrl+P` jump, persisted per workspace and
  off by default.
- The activity feed and tree follow that toggle, so ignored files also
  update live rather than only on Refresh. `.git`, `node_modules` and
  `target` stay filtered regardless — that, not the gitignore filter, is
  what keeps an `npm install` from flooding the feed.

### Changed

- Extra ignore globs now apply whether or not ignored files are shown, so a
  noisy directory can be excluded individually instead of having to turn the
  whole toggle off.
- Watcher filtering moved into a single `Filters` type. Watch registration
  and event filtering were deciding "is this ignored" from three separately
  threaded arguments and could disagree.

### Fixed

- "Diff since session" showed a git-ignored file as wholly added, even when
  it already existed with different content. Git has no baseline for a file
  it doesn't track, so the tab now says so instead of inventing one.
- Settings saved by an earlier version no longer reset when a new setting is
  introduced.

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
