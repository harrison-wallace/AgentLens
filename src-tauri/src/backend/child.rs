//! A backend running in another process, reached over its stdin/stdout.
//!
//! `wsl.exe -d Ubuntu -- agentlens-daemon --stdio` and
//! `ssh box 'agentlens-daemon' --stdio` differ only in the argv, so both go
//! through here. stdio rather than a socket is deliberate: it means no ports,
//! no tunnels, no firewall holes, and — for SSH — the user's own auth, agent
//! forwarding, jump hosts and 2FA, because the system `ssh` binary is doing
//! all of it.
//!
//! Three rules keep the framing honest, and the daemon holds up its end:
//! stdout carries protocol lines and nothing else, stderr carries logs and
//! nothing else, and every frame is exactly one line.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use agentlens_core::protocol::{
    AppSettings, Command, CommandResult, ConnectionInfo, ConnectionState, ConnectionTarget, Frame,
    Hello, WorkspaceSettings, EVENT_CONNECTION, EVENT_HEARTBEAT, PROTOCOL_VERSION,
};
use serde_json::Value;

use crate::backend::Backend;
use crate::events::EventEmitter;
use crate::remote;

/// How long a single command may take before the caller is told the link is
/// unhealthy. Generous, because a cold tree listing over a slow link is slow,
/// but finite, because a wedged daemon must not hang the UI forever.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Handshakes get their own budget: SSH may be prompting for a passphrase or
/// waiting on a hardware key.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Backoff schedule for reconnect attempts, in seconds. Running out means the
/// connection is `Failed` and the user has to intervene — retrying forever
/// would leave the status bar lying about a link that is never coming back.
const RECONNECT_BACKOFF: [u64; 6] = [1, 2, 4, 8, 16, 30];

/// Lines of the daemon's stderr kept for diagnostics. The failure that
/// matters most — "agentlens-daemon: command not found" — is one line, and
/// keeping a few covers the rest.
const STDERR_KEPT: usize = 20;

/// How long a failed handshake waits for the remote's stderr to finish.
/// The process is already dead by then, so this is a formality that costs
/// milliseconds — but skipping it loses the one line that explains why.
const STDERR_DRAIN: Duration = Duration::from_secs(2);

/// A daemon spawned as a child process, and the reconnect logic around it.
pub struct ChildProcess {
    shared: Arc<Shared>,
}

/// Everything the reader and reconnect threads share with the caller.
struct Shared {
    program: String,
    args: Vec<String>,
    emitter: Arc<dyn EventEmitter>,
    info: Mutex<ConnectionInfo>,
    /// The write half, on its own lock. Separate from `child` on purpose: a
    /// daemon that stops reading its stdin eventually fills the pipe and
    /// blocks whoever is writing, and `close_link` must be able to kill it
    /// *while that is happening* rather than queue behind it.
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    pending: Mutex<HashMap<u64, mpsc::Sender<CommandResult<Value>>>>,
    next_id: AtomicU64,
    /// The commands whose effects have to be re-applied to a *fresh* daemon.
    /// A reconnected daemon is a new process with no workspace, no settings
    /// and no watch, so without this a reconnect silently becomes a blank app.
    replay: Mutex<Replay>,
    stderr: Mutex<Vec<String>>,
    /// Set when the stderr pipe reaches EOF, which happens when the remote
    /// process ends. Read by `await_stderr`.
    stderr_done: AtomicBool,
    /// Set by `shutdown`, so the reader thread's EOF reads as "we asked for
    /// this" rather than as a crash worth reconnecting to.
    stopping: AtomicBool,
    /// Bumped per connection. A reader thread from a superseded connection
    /// exits instead of reporting a disconnect against its replacement.
    generation: AtomicU64,
}

/// The minimum state that makes a fresh daemon equivalent to the one that
/// died: what settings apply, which workspace is open.
#[derive(Default, Clone)]
struct Replay {
    app_settings: Option<AppSettings>,
    workspace: Option<String>,
    workspace_settings: Option<WorkspaceSettings>,
}

/// This app's version, which is also the version of daemon it installs.
const VERSION: &str = env!("CARGO_PKG_VERSION");

impl ChildProcess {
    /// Spawn a daemon for `target` and complete the handshake, installing one
    /// first if the remote hasn't got it.
    ///
    /// The install is the difference between "download this binary, put it
    /// somewhere on the non-interactive PATH, and if that fails set this
    /// settings field" and connecting to a machine that has never heard of
    /// AgentLens. Everything needed to do it comes back from the failed
    /// attempt: the bootstrap reports the remote's OS and architecture on its
    /// way out, so there is nothing left to ask the user.
    ///
    /// Anything that is *not* a missing daemon — a refused login, an unknown
    /// host, a protocol mismatch — is reported as itself. Installing on top of
    /// those would be answering a question nobody asked.
    pub fn connect(
        target: ConnectionTarget,
        daemon: String,
        auto_install: bool,
        emitter: Arc<dyn EventEmitter>,
    ) -> CommandResult<Self> {
        let spec = remote::spawn_spec(&target, &daemon, VERSION).ok_or_else(|| {
            format!(
                "{} cannot be reached: a name starting with `-` would be read as an option \
                 by the program that connects to it.",
                target.label()
            )
        })?;

        let first = match Self::spawn(target.clone(), spec.clone(), Arc::clone(&emitter)) {
            Ok(backend) => return Ok(backend),
            Err(err) => err,
        };

        let Some(platform) = remote::parse_not_installed(&first) else {
            return Err(first);
        };
        if !auto_install {
            return Err(format!(
                "AgentLens is not installed on {}, and automatic installation is turned off \
                 in Settings.\n\nSee docs/REMOTE.md to install it by hand.",
                target.label()
            ));
        }
        let Some(asset) = platform.asset() else {
            return Err(format!(
                "{} runs {}, which AgentLens does not publish a daemon for. Build \
                 `agentlens-daemon` there and name it in Settings → Remote → Daemon command.",
                target.label(),
                platform.describe()
            ));
        };

        emit_status(
            &emitter,
            &target,
            ConnectionState::Installing,
            Some(format!(
                "installing the AgentLens daemon on {}",
                target.label()
            )),
        );
        let installed = remote::provision(&target, VERSION, asset).map_err(|err| {
            format!(
                "Could not install the AgentLens daemon on {}.\n{err}\n\nInstall it by hand \
                 (see docs/REMOTE.md) and name it in Settings → Remote → Daemon command.",
                target.label()
            )
        })?;
        eprintln!("agentlens: installed on {} — {installed}", target.label());

        // One retry only. If a daemon we just wrote still cannot be started,
        // installing it again will not help and the second error is the honest
        // one to show.
        Self::spawn(target, spec, emitter)
    }

    /// The same, given the argv outright.
    ///
    /// Everything below this point is indifferent to *how* the daemon is
    /// reached — `wsl.exe`, `ssh`, or (in tests) the binary directly — so the
    /// one place that decides lives in `remote`, above.
    fn spawn(
        target: ConnectionTarget,
        (program, args): (String, Vec<String>),
        emitter: Arc<dyn EventEmitter>,
    ) -> CommandResult<Self> {
        let shared = Arc::new(Shared {
            info: Mutex::new(ConnectionInfo {
                label: target.label(),
                remote: target.is_remote(),
                state: ConnectionState::Connecting,
                since: agentlens_core::workspace::now_millis(),
                target,
                message: None,
                daemon_version: None,
            }),
            program,
            args,
            emitter,
            stdin: Mutex::new(None),
            child: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            replay: Mutex::new(Replay::default()),
            stderr: Mutex::new(Vec::new()),
            stderr_done: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        });

        shared.publish(ConnectionState::Connecting, None);
        match shared.open() {
            Ok(hello) => {
                shared.publish_connected(&hello);
                Ok(ChildProcess { shared })
            }
            Err(err) => {
                // `open` may have got as far as spawning before failing the
                // handshake. No `ChildProcess` exists to be dropped, so
                // nothing else would ever reap that process.
                shared.stop();
                shared.publish(ConnectionState::Failed, Some(err.clone()));
                Err(err)
            }
        }
    }
}

/// Push a connection state for a target that has no live backend yet.
///
/// The installing step happens between two `ChildProcess` instances — the one
/// that failed is gone, the one that will work does not exist — so it has
/// nowhere else to report from, and a silent 30-second pause during a first
/// connection is exactly when the user most needs to be told what is going on.
fn emit_status(
    emitter: &Arc<dyn EventEmitter>,
    target: &ConnectionTarget,
    state: ConnectionState,
    message: Option<String>,
) {
    let info = ConnectionInfo {
        label: target.label(),
        remote: target.is_remote(),
        target: target.clone(),
        since: agentlens_core::workspace::now_millis(),
        daemon_version: None,
        state,
        message,
    };
    if let Ok(payload) = serde_json::to_value(&info) {
        emitter.emit(EVENT_CONNECTION, &payload);
    }
}

impl Backend for ChildProcess {
    fn send(&self, command: Command) -> CommandResult<Value> {
        self.shared.remember(&command);
        self.shared.request(&command, REQUEST_TIMEOUT)
    }

    fn info(&self) -> ConnectionInfo {
        self.shared.snapshot()
    }

    fn shutdown(&self) {
        self.shared.stop();
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        self.shared.stop();
    }
}

impl Shared {
    /// How this connection is named in anything the user reads.
    fn label(&self) -> String {
        self.info
            .lock()
            .map(|info| info.label.clone())
            .unwrap_or_default()
    }

    /// Spawn the daemon, start its reader threads, and shake hands.
    ///
    /// Either this returns a live connection or it leaves none behind: a
    /// half-open attempt whose process is still running is how a few failed
    /// reconnects turn into a handful of orphaned `ssh` sessions.
    fn open(self: &Arc<Self>) -> CommandResult<Hello> {
        match self.try_open() {
            Ok(hello) => Ok(hello),
            Err(err) => {
                self.close_link();
                Err(err)
            }
        }
    }

    fn try_open(self: &Arc<Self>) -> CommandResult<Hello> {
        // Whatever was there is finished with; spawning over it would leak it.
        self.close_link();
        let program = &self.program;
        let mut command = ProcessCommand::new(program);
        command
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Windows would otherwise flash a console window every time the app
        // reconnects, which for a background retry is unacceptable.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command.spawn().map_err(|e| {
            format!("failed to start `{program}`: {e}. Is it installed and on this machine's PATH?")
        })?;

        let stdin = child.stdin.take().ok_or("daemon stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("daemon stdout unavailable")?;
        let stderr = child.stderr.take().ok_or("daemon stderr unavailable")?;

        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.stderr.lock().map(|mut kept| kept.clear()).ok();
        self.stderr_done.store(false, Ordering::SeqCst);
        {
            let mut guard = self.child.lock().map_err(|_| "connection state poisoned")?;
            *guard = Some(child);
        }
        {
            let mut guard = self.stdin.lock().map_err(|_| "connection state poisoned")?;
            *guard = Some(stdin);
        }

        let reader = Arc::clone(self);
        std::thread::spawn(move || reader.read_stdout(BufReader::new(stdout), generation));
        let logger = Arc::clone(self);
        std::thread::spawn(move || logger.read_stderr(BufReader::new(stderr)));

        let hello: Hello = serde_json::from_value(
            self.request(
                &Command::Hello {
                    protocol_version: PROTOCOL_VERSION,
                },
                HANDSHAKE_TIMEOUT,
            )
            .map_err(|e| self.explain_handshake_failure(&e))?,
        )
        .map_err(|e| format!("the daemon's handshake could not be read: {e}"))?;

        if hello.protocol_version != PROTOCOL_VERSION {
            return Err(format!(
                "the daemon on {} speaks protocol {} but this app speaks {}. \
                 Reinstall the daemon from the release matching AgentLens {}.",
                self.label(),
                hello.protocol_version,
                PROTOCOL_VERSION,
                env!("CARGO_PKG_VERSION"),
            ));
        }
        Ok(hello)
    }

    /// Turn a dead-on-arrival connection into something actionable.
    ///
    /// What the remote said on stderr is the whole diagnosis — usually the
    /// bootstrap's `not-installed` marker, which is what decides whether the
    /// app can fix this itself. So this waits for that stream to finish first:
    /// the handshake fails when *stdout* closes, and nothing orders that
    /// against stderr having been read. Without the wait, auto-install would
    /// work most of the time, which is worse than not working.
    fn explain_handshake_failure(&self, err: &str) -> String {
        self.await_stderr(STDERR_DRAIN);
        let tail = self
            .stderr
            .lock()
            .map(|kept| kept.join("\n"))
            .unwrap_or_default();
        let hint = format!(
            "Could not reach agentlens-daemon on {}: {err}",
            self.label()
        );
        if tail.is_empty() {
            hint
        } else {
            format!("{hint}\n\nThe remote said:\n{tail}")
        }
    }

    /// Wait for the stderr reader to reach the end of its pipe. Bounded: a
    /// daemon that is alive and simply quiet would otherwise never finish it.
    fn await_stderr(&self, within: Duration) {
        let deadline = std::time::Instant::now() + within;
        while !self.stderr_done.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Write one request and block until its response, or the timeout.
    fn request(&self, command: &Command, timeout: Duration) -> CommandResult<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| "connection state poisoned")?
            .insert(id, tx);

        let write = self.write(&Frame::Request {
            id,
            command: command.clone(),
        });
        if let Err(err) = write {
            self.pending.lock().ok().and_then(|mut p| p.remove(&id));
            return Err(err);
        }

        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(_) => {
                self.pending.lock().ok().and_then(|mut p| p.remove(&id));
                Err(format!(
                    "no reply from the daemon on {} after {}s",
                    self.label(),
                    timeout.as_secs()
                ))
            }
        }
    }

    fn write(&self, frame: &Frame) -> CommandResult<()> {
        let mut line =
            serde_json::to_string(frame).map_err(|e| format!("failed to encode a request: {e}"))?;
        line.push('\n');

        let mut guard = self.stdin.lock().map_err(|_| "connection state poisoned")?;
        let stdin = guard
            .as_mut()
            .ok_or_else(|| format!("not connected to {}", self.label()))?;
        stdin
            .write_all(line.as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("failed to reach the daemon: {e}"))
    }

    /// Demultiplex the daemon's stdout: responses go to whoever is waiting,
    /// events go straight to the front end.
    fn read_stdout(self: Arc<Self>, stdout: impl BufRead, generation: u64) {
        for line in stdout.lines() {
            if self.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Frame>(&line) {
                Ok(Frame::Response { id, result, error }) => {
                    if let Some(tx) = self.pending.lock().ok().and_then(|mut p| p.remove(&id)) {
                        let _ = tx.send(match error {
                            Some(message) => Err(message),
                            None => Ok(result.unwrap_or(Value::Null)),
                        });
                    }
                }
                // The heartbeat exists to keep the link warm; the front end
                // has no use for it and would only re-render on it.
                Ok(Frame::Event { event, .. }) if event == EVENT_HEARTBEAT => {}
                Ok(Frame::Event { event, payload }) => self.emitter.emit(&event, &payload),
                // A request arriving from the daemon, or an unparseable line,
                // is a bug on the far side. Log it and keep the link: one bad
                // line must not cost the user their session.
                Ok(Frame::Request { .. }) => {
                    eprintln!("agentlens: daemon sent a request, which it may not")
                }
                Err(err) => eprintln!("agentlens: unreadable frame from daemon: {err}"),
            }
        }
        self.lost(generation);
    }

    /// The daemon's stderr is logs. Mirrored to this process's stderr so it
    /// shows up in a terminal run, and the tail kept for the error message a
    /// failed handshake produces.
    fn read_stderr(self: Arc<Self>, stderr: impl BufRead) {
        for line in stderr.lines().map_while(Result::ok) {
            eprintln!("agentlens-daemon: {line}");
            if let Ok(mut kept) = self.stderr.lock() {
                if kept.len() == STDERR_KEPT {
                    kept.remove(0);
                }
                kept.push(line);
            }
        }
        self.stderr_done.store(true, Ordering::SeqCst);
    }

    /// The connection ended. Fail everything in flight, then try to get it
    /// back — unless we are the ones who ended it.
    fn lost(self: Arc<Self>, generation: u64) {
        if self.generation.load(Ordering::SeqCst) != generation {
            return;
        }
        let reason = format!("lost the connection to {}", self.label());
        self.fail_pending(&reason);
        if self.stopping.load(Ordering::SeqCst) {
            return;
        }
        self.publish(ConnectionState::Disconnected, Some(reason));
        std::thread::spawn(move || self.reconnect());
    }

    fn fail_pending(&self, reason: &str) {
        let Ok(mut pending) = self.pending.lock() else {
            return;
        };
        for (_, tx) in pending.drain() {
            let _ = tx.send(Err(reason.to_string()));
        }
    }

    /// Reconnect with backoff, then put the new daemon into the state the old
    /// one was in.
    fn reconnect(self: Arc<Self>) {
        let mut last = String::new();
        for wait in RECONNECT_BACKOFF {
            std::thread::sleep(Duration::from_secs(wait));
            if self.stopping.load(Ordering::SeqCst) {
                return;
            }
            match self.open() {
                Ok(hello) => match self.restore() {
                    Ok(()) => {
                        self.publish_connected(&hello);
                        return;
                    }
                    Err(err) => last = err,
                },
                Err(err) => last = err,
            }
        }
        self.publish(
            ConnectionState::Failed,
            Some(format!("could not reconnect to {}: {last}", self.label())),
        );
    }

    /// Re-apply the state a fresh daemon is missing, in the order the app
    /// itself would have: settings that gate visibility, then the workspace,
    /// then the settings that start the watch.
    fn restore(&self) -> CommandResult<()> {
        let replay = self
            .replay
            .lock()
            .map_err(|_| "connection state poisoned")?
            .clone();

        if let Some(value) = replay.app_settings {
            self.request(&Command::SetAppSettings { value }, REQUEST_TIMEOUT)?;
        }
        if let Some(path) = replay.workspace {
            self.request(&Command::OpenWorkspace { path }, REQUEST_TIMEOUT)?;
            self.request(
                &Command::SetWorkspaceSettings {
                    value: replay.workspace_settings.unwrap_or_default(),
                },
                REQUEST_TIMEOUT,
            )?;
        }
        Ok(())
    }

    /// Note the commands that a reconnect will have to repeat.
    fn remember(&self, command: &Command) {
        let Ok(mut replay) = self.replay.lock() else {
            return;
        };
        match command {
            Command::SetAppSettings { value } => replay.app_settings = Some(value.clone()),
            Command::OpenWorkspace { path } => {
                replay.workspace = Some(path.clone());
                replay.workspace_settings = None;
            }
            Command::SetWorkspaceSettings { value } => {
                replay.workspace_settings = Some(value.clone())
            }
            Command::CloseWorkspace => {
                replay.workspace = None;
                replay.workspace_settings = None;
            }
            _ => {}
        }
    }

    fn snapshot(&self) -> ConnectionInfo {
        self.info
            .lock()
            .map(|info| info.clone())
            .unwrap_or_default()
    }

    fn publish(&self, state: ConnectionState, message: Option<String>) {
        let info = {
            let Ok(mut info) = self.info.lock() else {
                return;
            };
            info.state = state;
            info.message = message;
            info.since = agentlens_core::workspace::now_millis();
            info.clone()
        };
        if let Ok(payload) = serde_json::to_value(&info) {
            self.emitter.emit(EVENT_CONNECTION, &payload);
        }
    }

    fn publish_connected(&self, hello: &Hello) {
        if let Ok(mut info) = self.info.lock() {
            info.daemon_version = Some(hello.version.clone());
        }
        self.publish(ConnectionState::Connected, None);
    }

    /// End the current connection's process, if there is one.
    ///
    /// `Child`'s own `Drop` neither kills nor reaps, so this has to be
    /// explicit — dropping the handle on its own leaves the daemon running.
    fn close_link(&self) {
        // Retire the reader thread first. It is about to see EOF, and without
        // this it would report a disconnect — and start reconnecting — against
        // a connection we are deliberately ending.
        self.generation.fetch_add(1, Ordering::SeqCst);

        // Kill before touching `stdin`, and never while holding its lock. A
        // writer blocked on a full pipe holds that lock until the process it
        // is writing to goes away, so doing this the other way round is how
        // closing the app hangs instead of closing.
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        if let Ok(mut guard) = self.stdin.lock() {
            guard.take();
        }
    }

    /// Close the link for good: stop the daemon, wake anyone waiting, and
    /// make sure no reconnect thread revives it.
    fn stop(&self) {
        if self.stopping.swap(true, Ordering::SeqCst) {
            return;
        }
        self.close_link();
        self.fail_pending("the connection was closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentlens_core::protocol::{ConnectionTarget, WorkspaceSettings, EVENT_FS_CHANGES};

    struct Silent;
    impl EventEmitter for Silent {
        fn emit(&self, _event: &str, _payload: &Value) {}
    }

    fn shared() -> Arc<Shared> {
        let target = ConnectionTarget::Ssh { host: "box".into() };
        let (program, args) = remote::spawn_spec(&target, "agentlens-daemon", VERSION).unwrap();
        Arc::new(Shared {
            program,
            args,
            emitter: Arc::new(Silent),
            info: Mutex::new(ConnectionInfo {
                label: target.label(),
                remote: true,
                target,
                ..Default::default()
            }),
            stdin: Mutex::new(None),
            child: Mutex::new(None),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            replay: Mutex::new(Replay::default()),
            stderr: Mutex::new(Vec::new()),
            stderr_done: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        })
    }

    #[test]
    fn replay_tracks_what_a_fresh_daemon_would_be_missing() {
        let shared = shared();
        shared.remember(&Command::SetAppSettings {
            value: AppSettings::default(),
        });
        shared.remember(&Command::OpenWorkspace {
            path: "/srv/app".into(),
        });
        shared.remember(&Command::SetWorkspaceSettings {
            value: WorkspaceSettings {
                show_ignored: true,
                ..Default::default()
            },
        });
        // Reads change nothing.
        shared.remember(&Command::ListFiles);

        let replay = shared.replay.lock().unwrap().clone();
        assert!(replay.app_settings.is_some());
        assert_eq!(replay.workspace.as_deref(), Some("/srv/app"));
        assert!(replay.workspace_settings.unwrap().show_ignored);
    }

    #[test]
    fn reopening_drops_the_previous_workspaces_settings() {
        let shared = shared();
        shared.remember(&Command::OpenWorkspace { path: "/a".into() });
        shared.remember(&Command::SetWorkspaceSettings {
            value: WorkspaceSettings {
                show_ignored: true,
                ..Default::default()
            },
        });
        shared.remember(&Command::OpenWorkspace { path: "/b".into() });

        let replay = shared.replay.lock().unwrap().clone();
        assert_eq!(replay.workspace.as_deref(), Some("/b"));
        assert!(
            replay.workspace_settings.is_none(),
            "settings belong to the workspace they were set for"
        );
    }

    #[test]
    fn closing_the_workspace_leaves_nothing_to_reopen() {
        let shared = shared();
        shared.remember(&Command::OpenWorkspace { path: "/a".into() });
        shared.remember(&Command::CloseWorkspace);

        assert!(shared.replay.lock().unwrap().workspace.is_none());
    }

    #[test]
    fn a_request_with_no_link_fails_instead_of_blocking() {
        let shared = shared();
        let err = shared
            .request(&Command::Ping, Duration::from_millis(50))
            .unwrap_err();
        assert!(err.contains("not connected"), "{err}");
        assert!(
            shared.pending.lock().unwrap().is_empty(),
            "a failed write must not leave the request pending"
        );
    }

    #[test]
    fn events_reach_the_front_end_and_responses_reach_their_caller() {
        let shared = shared();
        let (tx, rx) = mpsc::channel();
        shared.pending.lock().unwrap().insert(7, tx);

        let lines = concat!(
            r#"{"type":"event","event":"fs-changes","payload":[]}"#,
            "\n",
            r#"{"type":"response","id":7,"result":{"ok":true}}"#,
            "\n",
        );
        Arc::clone(&shared).read_stdout(BufReader::new(lines.as_bytes()), 0);

        assert_eq!(rx.recv().unwrap().unwrap(), serde_json::json!({"ok": true}));
    }

    #[test]
    fn an_unreadable_line_does_not_cost_the_rest_of_the_stream() {
        let shared = shared();
        let (tx, rx) = mpsc::channel();
        shared.pending.lock().unwrap().insert(1, tx);

        let lines = concat!(
            "not json at all\n",
            r#"{"type":"response","id":1,"error":"nope"}"#,
            "\n",
        );
        Arc::clone(&shared).read_stdout(BufReader::new(lines.as_bytes()), 0);

        assert_eq!(rx.recv().unwrap().unwrap_err(), "nope");
    }

    #[test]
    fn end_of_stream_fails_everything_still_waiting() {
        let shared = shared();
        let (tx, rx) = mpsc::channel();
        shared.pending.lock().unwrap().insert(1, tx);
        // Stopping first, so the test doesn't spawn a reconnect thread.
        shared.stopping.store(true, Ordering::SeqCst);

        Arc::clone(&shared).read_stdout(BufReader::new(&b""[..]), 0);

        assert!(rx
            .recv()
            .unwrap()
            .unwrap_err()
            .contains("lost the connection"));
    }

    #[test]
    fn a_daemon_that_cannot_even_be_started_says_which_program_was_missing() {
        let Err(err) = ChildProcess::spawn(
            ConnectionTarget::Ssh { host: "box".into() },
            ("agentlens-no-such-program".to_string(), Vec::new()),
            Arc::new(Silent),
        ) else {
            panic!("a program that does not exist must not connect");
        };

        assert!(err.contains("agentlens-no-such-program"), "{err}");
        assert!(err.contains("PATH"), "{err}");
    }

    /// Records what the transport pushes at the front end.
    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, Value)>>);

    impl EventEmitter for Recorder {
        fn emit(&self, event: &str, payload: &Value) {
            self.0
                .lock()
                .unwrap()
                .push((event.to_string(), payload.clone()));
        }
    }

    impl Recorder {
        /// Wait for an event named `name`, or give up.
        fn wait(&self, name: &str, within: Duration) -> Option<Value> {
            let deadline = std::time::Instant::now() + within;
            while std::time::Instant::now() < deadline {
                if let Some(payload) = self
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|(event, _)| event == name)
                    .map(|(_, payload)| payload.clone())
                {
                    return Some(payload);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            None
        }
    }

    fn wsl(distro: &str, script: &str) {
        let status = ProcessCommand::new("wsl.exe")
            .args(["-d", distro, "--", "sh", "-c", script])
            .status()
            .expect("wsl.exe must be runnable");
        assert!(status.success(), "`{script}` failed inside {distro}");
    }

    /// The real thing: a daemon inside a WSL distro, observing that distro's
    /// own filesystem, seen from Windows.
    ///
    /// Ignored by default — it needs a distro with the daemon installed, which
    /// only CI and a developer on Windows have. `ci.yml` runs it explicitly, so
    /// a broken WSL path fails the build rather than quietly skipping.
    ///
    /// The workspace lives under `/tmp` *inside* the distro on purpose. Watching
    /// it through `\\wsl$` is precisely what does not work — the 9P bridge does
    /// not propagate inotify — and that is the reason this whole phase exists.
    #[test]
    #[ignore = "needs a WSL distro with agentlens-daemon installed; run from ci.yml"]
    fn a_daemon_inside_wsl_reports_changes_made_inside_wsl() {
        let distro = std::env::var("AGENTLENS_WSL_DISTRO")
            .expect("set AGENTLENS_WSL_DISTRO to the distro to test against");
        let daemon = std::env::var("AGENTLENS_WSL_DAEMON")
            .unwrap_or_else(|_| remote::DEFAULT_DAEMON_COMMAND.to_string());
        let root = "/tmp/agentlens-smoke";
        wsl(
            &distro,
            &format!("rm -rf {root} && mkdir -p {root} && printf 'hello\\n' > {root}/a.txt"),
        );

        let events = Arc::new(Recorder::default());
        let backend = ChildProcess::connect(
            ConnectionTarget::Wsl {
                distro: distro.clone(),
            },
            daemon,
            // The distro has one installed already; this asserts the bootstrap
            // *finds* it rather than that provisioning papers over a miss.
            false,
            events.clone(),
        )
        .expect("the daemon inside the distro must answer the handshake");

        assert_eq!(backend.info().state, ConnectionState::Connected);
        assert!(backend.info().daemon_version.is_some());

        let opened = backend
            .send(Command::OpenWorkspace {
                path: root.to_string(),
            })
            .expect("the distro's own path opens");
        assert_eq!(opened["root"], root);
        backend
            .send(Command::SetWorkspaceSettings {
                value: WorkspaceSettings::default(),
            })
            .expect("settings apply and start the watch");

        let files: Vec<String> =
            serde_json::from_value(backend.send(Command::ListFiles).unwrap()).unwrap();
        assert_eq!(files, vec!["a.txt".to_string()]);

        // The edit happens inside the distro, which is the only place inotify
        // will see it.
        wsl(&distro, &format!("printf 'changed\\n' >> {root}/a.txt"));

        let payload = events
            .wait(EVENT_FS_CHANGES, Duration::from_secs(10))
            .expect("a change made inside the distro must reach the app");
        let changed: Vec<agentlens_core::protocol::FsEvent> =
            serde_json::from_value(payload).unwrap();
        assert!(
            changed.iter().any(|event| event.path == "a.txt"),
            "expected a.txt in {changed:?}"
        );

        backend.shutdown();
        wsl(&distro, &format!("rm -rf {root}"));
    }
}
