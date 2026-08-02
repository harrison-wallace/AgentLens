//! Bridges between core and Tauri.
//!
//! `agentlens-core` knows nothing about windows, so the app supplies the one
//! thing that is inherently local to a running desktop process: somewhere for
//! a backend's output to go.
//!
//! Two traits, one struct. The in-process engine pushes typed values through
//! `EventSink`; a remote daemon pushes JSON that arrived off a pipe, already
//! named, through `EventEmitter`. Both end up as the same Tauri events, which
//! is what lets the front end stay ignorant of where its backend runs.

use agentlens_core::protocol::{
    FsEvent, GitStatusSnapshot, WatcherStatus, EVENT_FS_CHANGES, EVENT_GIT_STATUS,
    EVENT_WATCHER_STATUS,
};
use agentlens_core::watcher::EventSink;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

/// Anything that can push a named, already-serialized event to the front end.
///
/// The remote transport needs this rather than `EventSink`: what comes off the
/// pipe is a name and a payload it has no reason to parse, and re-typing it
/// only to re-serialize it would be work done twice for no safety.
pub trait EventEmitter: Send + Sync + 'static {
    fn emit(&self, event: &str, payload: &Value);
}

/// Forwards a backend's output to the webview as Tauri events.
///
/// Emission is best-effort by design: a window that has gone away must not
/// take the watcher thread — or the transport's reader thread — down with it.
#[derive(Clone)]
pub struct TauriEvents(pub AppHandle);

impl EventSink for TauriEvents {
    fn fs_changes(&self, events: &[FsEvent]) {
        let _ = self.0.emit(EVENT_FS_CHANGES, events);
    }

    fn git_status(&self, snapshot: &GitStatusSnapshot) {
        let _ = self.0.emit(EVENT_GIT_STATUS, snapshot);
    }

    fn watcher_status(&self, status: &WatcherStatus) {
        let _ = self.0.emit(EVENT_WATCHER_STATUS, status);
    }
}

impl EventEmitter for TauriEvents {
    fn emit(&self, event: &str, payload: &Value) {
        let _ = self.0.emit(event, payload);
    }
}
