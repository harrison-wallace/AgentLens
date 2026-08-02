//! AgentLens, headless, spoken to over stdio.
//!
//! This is `agentlens-core` with a front door and nothing else: no window, no
//! settings store, no state that outlives the process. It exists because
//! watching files over a network filesystem does not work — 9P (`\\wsl$`)
//! does not propagate inotify, and SFTP has no events at all — so the
//! observer has to run where the files are and stream results back.
//!
//! ## The stdio contract
//!
//! - **stdout is protocol, exclusively.** One JSON `Frame` per line, nothing
//!   else, ever. A stray `println!` here corrupts the stream and there is no
//!   recovering from it, which is why every diagnostic in this file goes to
//!   stderr.
//! - **stderr is logs, exclusively.** The app mirrors it to its own stderr and
//!   keeps the tail for error messages, so it is the right place for "I could
//!   not start" — but nothing on it is ever parsed.
//! - **stdin closing means shut down.** The transport drops the pipe; the loop
//!   ends, the watcher stops, the process exits 0.
//!
//! Requests are dispatched on their own threads, matching what the desktop app
//! does in-process. Ordering between *concurrent* requests is therefore the
//! caller's business — as it is with any async transport — and the app awaits
//! each dependent command before issuing the next.

use std::io::{BufRead, BufReader, Stdout, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agentlens_core::engine::Engine;
use agentlens_core::protocol::{
    Frame, FsEvent, GitStatusSnapshot, WatcherStatus, EVENT_FS_CHANGES, EVENT_GIT_STATUS,
    EVENT_HEARTBEAT, EVENT_WATCHER_STATUS, PROTOCOL_VERSION,
};
use agentlens_core::watcher::EventSink;
use agentlens_core::workspace::now_millis;
use serde_json::{json, Value};

/// How often a quiet connection says something.
///
/// Not a liveness check — the transport already learns about death from EOF.
/// This is for the network in between: an SSH session with nothing on it for
/// an hour is exactly what an idle NAT or a corporate firewall reaps, and a
/// workspace nobody is touching is a normal state for an observer.
const HEARTBEAT: Duration = Duration::from_secs(30);

const USAGE: &str = "\
agentlens-daemon — headless AgentLens observer

USAGE:
    agentlens-daemon --stdio      speak the AgentLens protocol on stdin/stdout
    agentlens-daemon --version    print version and protocol version
    agentlens-daemon --help       print this message

Not meant to be run by hand: the AgentLens desktop app spawns it over
`wsl.exe` or `ssh`. Running it in a terminal will just sit there waiting for
JSON on stdin.
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--stdio") => serve(),
        Some("--version" | "-V") => {
            println!(
                "{} {} (protocol {})",
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
                PROTOCOL_VERSION
            );
        }
        Some("--help" | "-h") => print!("{USAGE}"),
        _ => {
            eprint!("{USAGE}");
            std::process::exit(2);
        }
    }
}

/// How long a shutdown waits for requests that are still being answered.
///
/// Bounded rather than unconditional: the app is entitled to a reply to
/// everything it asked for, but a command wedged on a hung filesystem must not
/// keep the process alive after the connection it belonged to has gone.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// Read frames until stdin closes, then stop watching and go.
fn serve() {
    let out = Arc::new(Out::new());
    let engine = Arc::new(Engine::new(Arc::new(StdoutSink(Arc::clone(&out)))));
    // Requests are handled off-thread, so the read loop reaching EOF does not
    // mean the work is finished. Without this the process can exit between a
    // command being read and its answer being written, and the app waits out
    // its timeout for a reply that was computed and thrown away.
    let in_flight = Arc::new(AtomicUsize::new(0));

    let beat = Arc::clone(&out);
    std::thread::spawn(move || loop {
        std::thread::sleep(HEARTBEAT);
        beat.event(EVENT_HEARTBEAT, json!({ "at": now_millis() }));
    });

    let stdin = std::io::stdin();
    for line in BufReader::new(stdin.lock()).lines() {
        let Ok(line) = line else {
            eprintln!("agentlens-daemon: stdin became unreadable, shutting down");
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let engine = Arc::clone(&engine);
        let out = Arc::clone(&out);
        let counter = Arc::clone(&in_flight);
        counter.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            out.frame(&respond(&engine, &line));
            counter.fetch_sub(1, Ordering::SeqCst);
        });
    }

    let deadline = Instant::now() + DRAIN_TIMEOUT;
    while in_flight.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }

    engine.shutdown();
}

/// Turn one inbound line into the frame that answers it.
///
/// Kept pure — it takes text and returns a frame — because framing bugs are
/// the subtle kind: a partial line, an unexpected shape, an id that isn't
/// echoed back leaves the app waiting for a reply that never comes.
fn respond(engine: &Engine, line: &str) -> Frame {
    match serde_json::from_str::<Frame>(line) {
        Ok(Frame::Request { id, command }) => match engine.handle(command) {
            Ok(result) => Frame::Response {
                id,
                result: Some(result),
                error: None,
            },
            Err(error) => Frame::Response {
                id,
                result: None,
                error: Some(error),
            },
        },
        // Responses and events travel the other way. Answering id 0 rather
        // than staying silent means a confused client still gets told.
        Ok(_) => Frame::Response {
            id: 0,
            result: None,
            error: Some("only request frames may be sent to the daemon".to_string()),
        },
        Err(err) => Frame::Response {
            id: 0,
            result: None,
            error: Some(format!("unreadable frame: {err}")),
        },
    }
}

/// The one place anything is allowed to write to stdout.
///
/// Behind a mutex because the watcher thread, the heartbeat thread and every
/// in-flight request all emit: two interleaved half-lines would desynchronize
/// the stream permanently.
struct Out(Mutex<Stdout>);

impl Out {
    fn new() -> Self {
        Out(Mutex::new(std::io::stdout()))
    }

    fn frame(&self, frame: &Frame) {
        let Ok(mut line) = serde_json::to_string(frame) else {
            eprintln!("agentlens-daemon: a frame could not be encoded and was dropped");
            return;
        };
        line.push('\n');
        let Ok(mut out) = self.0.lock() else {
            return;
        };
        // A broken pipe means the app is gone; the read loop is about to see
        // EOF and exit, so there is nothing useful to do here but stop.
        let _ = out.write_all(line.as_bytes()).and_then(|_| out.flush());
    }

    fn event(&self, event: &str, payload: Value) {
        self.frame(&Frame::Event {
            event: event.to_string(),
            payload,
        });
    }
}

/// The engine's push output, on the wire.
struct StdoutSink(Arc<Out>);

impl StdoutSink {
    /// A payload that cannot be encoded is a bug in a protocol type. Dropping
    /// it costs one event; writing a half-line would cost the connection.
    fn emit(&self, event: &str, payload: Result<Value, serde_json::Error>) {
        match payload {
            Ok(payload) => self.0.event(event, payload),
            Err(err) => eprintln!("agentlens-daemon: could not encode {event}: {err}"),
        }
    }
}

impl EventSink for StdoutSink {
    fn fs_changes(&self, events: &[FsEvent]) {
        self.emit(EVENT_FS_CHANGES, serde_json::to_value(events));
    }

    fn git_status(&self, snapshot: &GitStatusSnapshot) {
        self.emit(EVENT_GIT_STATUS, serde_json::to_value(snapshot));
    }

    fn watcher_status(&self, status: &WatcherStatus) {
        self.emit(EVENT_WATCHER_STATUS, serde_json::to_value(status));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentlens_core::protocol::{Command, Hello};

    struct Discard;
    impl EventSink for Discard {
        fn fs_changes(&self, _: &[FsEvent]) {}
        fn git_status(&self, _: &GitStatusSnapshot) {}
        fn watcher_status(&self, _: &WatcherStatus) {}
    }

    fn engine() -> Engine {
        Engine::new(Arc::new(Discard))
    }

    fn line(frame: &Frame) -> String {
        serde_json::to_string(frame).unwrap()
    }

    #[test]
    fn a_request_is_answered_with_its_own_id() {
        let request = line(&Frame::Request {
            id: 42,
            command: Command::Hello {
                protocol_version: PROTOCOL_VERSION,
            },
        });

        match respond(&engine(), &request) {
            Frame::Response { id, result, error } => {
                assert_eq!(id, 42);
                assert!(error.is_none());
                let hello: Hello = serde_json::from_value(result.unwrap()).unwrap();
                assert_eq!(hello.protocol_version, PROTOCOL_VERSION);
            }
            other => panic!("expected a response, got {other:?}"),
        }
    }

    #[test]
    fn a_failing_command_answers_with_an_error_not_a_dropped_request() {
        let request = line(&Frame::Request {
            id: 3,
            command: Command::ListFiles,
        });

        match respond(&engine(), &request) {
            Frame::Response { id, result, error } => {
                assert_eq!(id, 3);
                assert!(result.is_none());
                assert_eq!(error.unwrap(), "no workspace is open");
            }
            other => panic!("expected a response, got {other:?}"),
        }
    }

    #[test]
    fn junk_and_wrong_way_frames_are_answered_rather_than_ignored() {
        for input in [
            "}{".to_string(),
            r#"{"type":"unknown"}"#.to_string(),
            line(&Frame::Event {
                event: "fs-changes".into(),
                payload: json!([]),
            }),
        ] {
            match respond(&engine(), &input) {
                Frame::Response { id, error, .. } => {
                    assert_eq!(id, 0);
                    assert!(error.is_some(), "{input}");
                }
                other => panic!("expected a response, got {other:?}"),
            }
        }
    }

    #[test]
    fn every_frame_the_daemon_writes_is_exactly_one_line() {
        // Multi-line content is the obvious way to break line framing, and a
        // commit message or a file preview is the obvious source of it.
        let frames = [
            Frame::Response {
                id: 1,
                result: Some(json!({ "text": "first\nsecond\r\nthird" })),
                error: None,
            },
            Frame::Response {
                id: 2,
                result: None,
                error: Some("failed:\nreason".into()),
            },
            Frame::Event {
                event: EVENT_HEARTBEAT.into(),
                payload: json!({ "at": 1_700_000_000_000i64 }),
            },
        ];
        for frame in frames {
            let encoded = serde_json::to_string(&frame).unwrap();
            assert_eq!(encoded.lines().count(), 1, "{encoded}");
            assert_eq!(serde_json::from_str::<Frame>(&encoded).unwrap(), frame);
        }
    }
}
