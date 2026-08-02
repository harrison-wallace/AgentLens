# Remote workspaces (WSL and SSH)

AgentLens can observe a directory that isn't on the machine running the app: a
WSL distro seen from the Windows app, or a Linux box reached over SSH.

## Why a daemon, and not just a network path

Windows exposes a WSL distro at `\\wsl$\<distro>\…`, and SFTP exposes an SSH
host's files too. Neither can be _watched_: the 9P bridge behind `\\wsl$` does
not propagate inotify events, and SFTP has no event mechanism at all. Polling a
large tree across either is unusable.

So AgentLens does what VS Code Remote and JetBrains Gateway do — it runs a
small headless observer **where the files are** and streams results back:

```
┌──────────────┐   newline-delimited JSON over stdio   ┌──────────────────┐
│ AgentLens UI │ ◄──────────────────────────────────►  │ agentlens-daemon │
│  (Tauri app) │   wsl:  wsl.exe -d <distro> …         │  watcher · git   │
└──────────────┘   ssh:  ssh <host> …                  │  agents · diffs  │
                                                       └──────────────────┘
```

stdio, not a socket. That means **no ports, no tunnels, no firewall rules**,
and — over SSH — your own authentication: `~/.ssh/config` aliases, agent
forwarding, jump hosts and 2FA all behave exactly as they do when you type
`ssh` yourself, because the system `ssh` binary is the one doing it.

## Setup: there isn't any

Open the workspace. If the machine has never run AgentLens, the app installs
the observer there itself — no `curl`, no `PATH` fiddling, no settings.

What happens on first connect:

1. A small shell bootstrap runs on the remote. It looks for a daemon in the
   place AgentLens manages, then `~/.local/bin`, `/usr/local/bin`, `/usr/bin`,
   then `PATH` — and `exec`s the first one it finds.
2. If there is none, it reports the remote's OS and architecture and exits.
3. The app downloads the matching binary **on the remote** (`curl` or `wget`),
   verifies it against the release's `SHA256SUMS`, and installs it to
   `~/.agentlens/bin/<version>/agentlens-daemon`. The status bar says
   _installing_ while this happens.
4. It connects. Versions from older releases are pruned once the new one
   works.

Nothing is installed outside your own home directory on that machine, nothing
needs `sudo`, and nothing is left behind on uninstall beyond that directory.

Turn it off with **Settings → Remote → Set up remote machines automatically**
if you would rather place the binary yourself.

## Opening a workspace

From the AgentLens start screen, **Open in WSL…** (distros come from
`wsl.exe -l -q`) or **Open over SSH…** (any host name your `ssh` command
accepts). Then either:

- **Browse…** — connects, then lists that machine's directories starting at
  your home directory, marking which ones are git repositories. Click to
  descend, `../` to go up, **Open this folder** to take the one you are in.
  There is no OS file dialog for a WSL distro or an SSH host, so this is the
  backend listing its own directories a level at a time.
- **Type the path** — faster when you know it. For WSL it is required and must
  be a path _inside_ the distro, e.g. `/home/you/project`. Not `/mnt/c/...`: a
  Windows directory seen from WSL has the same inotify problem in reverse. For
  SSH, leaving it blank opens your home directory.

Once a remote connection is live, the main **Open folder** button browses that
machine too, rather than opening a local dialog for the wrong filesystem.

Either way it is recorded in **Recent** as a location —
`wsl://Ubuntu/home/you/project` or `ssh://build-box/srv/app` — so reopening it
later is one click, and it reconnects on the way. You can type those locations
anywhere a workspace path is accepted.

## What works remotely

Everything the local observer does: the file tree, the activity feed, git
status and decorations, git actions, previews, diff-since-session, and agent
session tailing. Agent transcripts are read from `~/.claude` **on the remote
machine**, which is where an agent running in that WSL distro or on that host
writes them — so the attributed feed works exactly as it does locally.

Two differences:

- **Opening a file in another application.** For WSL, the path is translated
  to `\\wsl$\<distro>\…` and handed to Windows. For SSH there is no such
  bridge, so AgentLens says so rather than doing nothing.
- **Settings are stored by location.** `/srv/app` on two different hosts are
  two different workspaces with their own ignore globs and pins.

## When the connection drops

The app notices, marks the status bar, and reconnects with backoff. The
activity feed shows the window it was blind for — `disconnected 14:02–14:03` —
rather than silently resuming, because "nothing happened" is the one thing it
cannot know about that gap.

On reconnect the daemon is a **new process**, so the app re-applies your
settings and reopens the workspace before declaring the link healthy.

If reconnecting keeps failing, the status bar goes to `failed` with the reason.
Reopening the workspace starts a fresh connection.

## Installing it yourself

Needed when the remote has no outbound internet, when automatic setup is
turned off, or when AgentLens publishes no daemon for that platform.

| Remote machine      | Asset                                 |
| ------------------- | ------------------------------------- |
| Linux / WSL, x86-64 | `agentlens-daemon-linux-x86_64`       |
| Linux, ARM64        | `agentlens-daemon-linux-aarch64`      |
| Windows, x86-64     | `agentlens-daemon-windows-x86_64.exe` |

Run this **inside the WSL distro, or on the SSH host** — replace `<version>`
with the release you want and `<asset>` with the row above:

```sh
curl -fL --create-dirs -o ~/.local/bin/agentlens-daemon \
  https://github.com/harrison-wallace/AgentLens/releases/download/<version>/<asset>
chmod +x ~/.local/bin/agentlens-daemon
~/.local/bin/agentlens-daemon --version
```

`~/.local/bin` is one of the places the bootstrap looks, so that is enough —
you do **not** need it on `PATH`, and you do not need `sudo`. (It is worth
knowing why: `ssh host command` runs without a login shell, so `~/.local/bin`
is usually absent from `PATH`. Earlier versions of AgentLens ran a bare
command name and tripped over exactly that.)

Or build it from a checkout of this repository on that machine:

```sh
cargo build --release -p agentlens-daemon
install -Dm755 target/release/agentlens-daemon ~/.local/bin/agentlens-daemon
```

If you keep it somewhere unusual, name the absolute path in **Settings →
Remote → Daemon command**. That runs exactly what you name, skipping the
search and the automatic install — it is the escape hatch for anything the
bootstrap cannot cope with, such as a Windows remote whose SSH shell is
`cmd.exe` rather than a POSIX one.

## Version mismatches

The app and the daemon shake hands before anything else. If the daemon speaks a
different protocol version you get a message naming both versions and telling
you to reinstall — never a half-working session. See
[PROTOCOL.md](PROTOCOL.md) for what is allowed to change and when.

## Security notes

- The daemon **listens on nothing**. It reads stdin and writes stdout; the only
  way to reach it is to already have the ability to run a command on that
  machine.
- AgentLens never handles your SSH credentials. It runs the system `ssh`
  binary and lets it do the authentication.
- Automatic setup writes one file into your own home directory on a machine
  the app can already run commands on — strictly less than the access it
  needed to get there. It never uses `sudo` and never touches system paths.
- Downloads come from GitHub over HTTPS and are checked against the release's
  `SHA256SUMS`. A release published before checksums existed installs with a
  warning on stderr rather than a silent skip.
- The daemon is read-only except for the git actions you explicitly trigger,
  and it refuses to read files outside the workspace root. The folder browser
  is the one thing that looks outside it, by design — choosing a workspace
  means seeing directories you have not chosen — and it returns names only,
  never contents.
- It writes no state of its own: no config, no cache, nothing that outlives the
  process. The only thing left on a remote machine is the binary itself, under
  `~/.agentlens/bin/`, and `rm -rf ~/.agentlens` is a complete uninstall.
