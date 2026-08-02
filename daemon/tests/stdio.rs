//! Drives the real daemon binary over a real pipe.
//!
//! The unit tests in `main.rs` prove one line in produces the right line out.
//! This proves the thing an app actually depends on: spawn it, shake hands,
//! open a workspace, touch a file, and see the change arrive as an event —
//! phase 4's acceptance criterion with the transport shortened to a pipe on
//! this machine.
//!
//! Every read is bounded. A framing bug's usual symptom is a reply that never
//! comes, and a test that hangs is a test that tells you nothing.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use agentlens_core::protocol::{
    Command as Cmd, Frame, FsEvent, Hello, WorkspaceInfo, WorkspaceSettings, EVENT_FS_CHANGES,
    PROTOCOL_VERSION,
};
use serde_json::Value;

/// Long enough to absorb a cold, loaded CI runner; short enough that a wedged
/// daemon fails this test rather than the whole job.
const TIMEOUT: Duration = Duration::from_secs(20);

struct Daemon {
    child: Child,
    stdin: Option<ChildStdin>,
    frames: Receiver<Frame>,
    next_id: u64,
}

impl Daemon {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_agentlens-daemon"))
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Inherited, so a daemon-side panic shows up in the test output
            // instead of vanishing.
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the daemon binary must be runnable");

        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let (tx, frames) = mpsc::channel();
        std::thread::spawn(move || {
            for line in stdout.lines().map_while(Result::ok) {
                assert!(!line.is_empty(), "the daemon must not write blank lines");
                let frame = serde_json::from_str(&line)
                    .unwrap_or_else(|e| panic!("unreadable frame {line:?}: {e}"));
                if tx.send(frame).is_err() {
                    return;
                }
            }
        });

        Daemon {
            child,
            stdin: Some(stdin),
            frames,
            next_id: 1,
        }
    }

    /// Send a command and wait for *its* response, discarding events that
    /// arrive in the meantime.
    fn call(&mut self, command: Cmd) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(&Frame::Request { id, command });

        let deadline = Instant::now() + TIMEOUT;
        loop {
            match self.next_frame(deadline) {
                Frame::Response {
                    id: got,
                    result,
                    error,
                } => {
                    assert_eq!(got, id, "the daemon must echo the id it was given");
                    return match error {
                        Some(message) => Err(message),
                        None => Ok(result.unwrap_or(Value::Null)),
                    };
                }
                Frame::Event { .. } => continue,
                other => panic!("unexpected frame: {other:?}"),
            }
        }
    }

    /// Wait for a push named `event`.
    fn wait_for_event(&mut self, event: &str) -> Value {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Frame::Event {
                event: name,
                payload,
            } = self.next_frame(deadline)
            {
                if name == event {
                    return payload;
                }
            }
        }
    }

    fn next_frame(&mut self, deadline: Instant) -> Frame {
        let left = deadline.saturating_duration_since(Instant::now());
        match self.frames.recv_timeout(left) {
            Ok(frame) => frame,
            Err(RecvTimeoutError::Timeout) => panic!("the daemon went quiet for {TIMEOUT:?}"),
            Err(RecvTimeoutError::Disconnected) => panic!("the daemon closed stdout"),
        }
    }

    fn write(&mut self, frame: &Frame) {
        let stdin = self.stdin.as_mut().expect("stdin is still open");
        let mut line = serde_json::to_string(frame).unwrap();
        line.push('\n');
        stdin.write_all(line.as_bytes()).unwrap();
        stdin.flush().unwrap();
    }

    /// What the transport does when the app closes: drop the pipe.
    fn close_stdin(&mut self) {
        self.stdin.take();
    }

    fn open(&mut self, root: &std::path::Path) -> WorkspaceInfo {
        let info: WorkspaceInfo = serde_json::from_value(
            self.call(Cmd::OpenWorkspace {
                path: root.to_string_lossy().into_owned(),
            })
            .expect("workspace opens"),
        )
        .unwrap();
        self.call(Cmd::SetWorkspaceSettings {
            value: WorkspaceSettings::default(),
        })
        .expect("settings apply and start the watch");
        info
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn handshake_reports_the_protocol_the_binary_speaks() {
    let mut daemon = Daemon::start();

    let hello: Hello = serde_json::from_value(
        daemon
            .call(Cmd::Hello {
                protocol_version: PROTOCOL_VERSION,
            })
            .expect("the handshake succeeds"),
    )
    .unwrap();

    assert_eq!(hello.protocol_version, PROTOCOL_VERSION);
    assert_eq!(hello.name, "agentlens-core");
    assert!(!hello.version.is_empty());
}

#[test]
fn a_version_mismatch_is_refused_with_an_explanation() {
    let mut daemon = Daemon::start();

    let err = daemon
        .call(Cmd::Hello {
            protocol_version: PROTOCOL_VERSION + 41,
        })
        .expect_err("a mismatched handshake must fail");

    assert!(err.contains("protocol version mismatch"), "{err}");
}

#[test]
fn a_file_written_after_opening_arrives_as_an_event() {
    let dir = tempfile::tempdir().unwrap();
    let mut daemon = Daemon::start();
    daemon.open(dir.path());

    std::fs::write(dir.path().join("touched.txt"), "hello").unwrap();

    let events: Vec<FsEvent> =
        serde_json::from_value(daemon.wait_for_event(EVENT_FS_CHANGES)).unwrap();
    assert!(
        events.iter().any(|e| e.path == "touched.txt"),
        "expected touched.txt in {events:?}"
    );
}

#[test]
fn the_tree_and_previews_come_back_over_the_pipe() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    let mut daemon = Daemon::start();
    daemon.open(dir.path());

    let files: Vec<String> = serde_json::from_value(daemon.call(Cmd::ListFiles).unwrap()).unwrap();
    assert_eq!(files, vec!["src/main.rs".to_string()]);

    let preview = daemon
        .call(Cmd::ReadPreview {
            path: "src/main.rs".into(),
        })
        .unwrap();
    assert_eq!(preview["kind"], "text");
    assert_eq!(preview["text"], "fn main() {}\n");
}

#[test]
fn multi_line_content_survives_line_framing() {
    // The failure this guards against is silent and total: one unescaped
    // newline in a payload and every frame after it is misread.
    let dir = tempfile::tempdir().unwrap();
    let text = "first\nsecond\r\nthird\n";
    std::fs::write(dir.path().join("multi.txt"), text).unwrap();
    let mut daemon = Daemon::start();
    daemon.open(dir.path());

    let preview = daemon
        .call(Cmd::ReadPreview {
            path: "multi.txt".into(),
        })
        .unwrap();
    assert_eq!(preview["text"], text);

    // Still in sync afterwards.
    assert!(daemon.call(Cmd::Ping).is_ok());
}

#[test]
fn errors_come_back_as_errors_rather_than_closing_the_stream() {
    let mut daemon = Daemon::start();

    assert_eq!(
        daemon.call(Cmd::ListFiles).unwrap_err(),
        "no workspace is open"
    );
    // Still there, still counting.
    assert!(daemon.call(Cmd::Ping).is_ok());
}

#[test]
fn closing_stdin_shuts_the_daemon_down() {
    let dir = tempfile::tempdir().unwrap();
    let mut daemon = Daemon::start();
    daemon.open(dir.path());

    daemon.close_stdin();

    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(status) = daemon.child.try_wait().unwrap() {
            assert!(status.success(), "a clean shutdown must exit 0");
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the daemon outlived its stdin by more than {TIMEOUT:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
