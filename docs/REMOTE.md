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

## 1. Install the daemon on the remote machine

The daemon is a single binary with no runtime dependencies. It is **not** part
of the app installer, because it usually runs on a different machine and often
a different OS.

Download the one matching the remote machine from
[Releases](https://github.com/harrison-wallace/AgentLens/releases):

| Remote machine      | Asset                                 |
| ------------------- | ------------------------------------- |
| Linux / WSL, x86-64 | `agentlens-daemon-linux-x86_64`       |
| Linux, ARM64        | `agentlens-daemon-linux-aarch64`      |
| Windows, x86-64     | `agentlens-daemon-windows-x86_64.exe` |

Run this **inside the WSL distro, or on the SSH host** — replace `<version>`
with the release you want and `<asset>` with the row above:

```sh
curl -fL -o ~/.local/bin/agentlens-daemon \
  https://github.com/harrison-wallace/AgentLens/releases/download/<version>/<asset>
chmod +x ~/.local/bin/agentlens-daemon
agentlens-daemon --version
```

Or build it from a checkout of this repository on that machine:

```sh
cargo build --release -p agentlens-daemon
install -Dm755 target/release/agentlens-daemon ~/.local/bin/agentlens-daemon
```

### If `--version` printed nothing, or "command not found"

`~/.local/bin` is on your `PATH` in an interactive shell but frequently
**not** in the non-interactive one that `ssh host command` uses. This is the
single most common reason a correctly installed daemon appears to be missing.

Either install it somewhere always on `PATH`:

```sh
sudo install -Dm755 ~/.local/bin/agentlens-daemon /usr/local/bin/agentlens-daemon
```

…or open **Settings → Remote → Daemon command** in AgentLens and put the
absolute path there (`/home/you/.local/bin/agentlens-daemon`). It applies to
the next connection.

## 2. Open the workspace

From the AgentLens start screen:

- **Open in WSL…** — pick a distro (listed by `wsl.exe -l -q`) and type the
  path _inside_ that distro, e.g. `/home/you/project`. Not `/mnt/c/...`: a
  Windows directory seen from WSL has the same inotify problem in reverse.
- **Open over SSH…** — type any host name your `ssh` command accepts, and
  optionally a path. Leave the path blank for your home directory.

Once open, it is recorded in **Recent** as a location — `wsl://Ubuntu/home/you/project`
or `ssh://build-box/srv/app` — so reopening it later is one click, and it
reconnects on the way.

You can type those locations anywhere a workspace path is accepted.

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
- The daemon is read-only except for the git actions you explicitly trigger,
  and it refuses paths outside the workspace root.
- It stores nothing on the remote machine: no config, no cache, no state that
  outlives the process.
