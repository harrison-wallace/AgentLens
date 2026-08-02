//! Where commands go.
//!
//! Every operation the UI performs is a [`Command`] handed to a `Backend`. One
//! implementation runs the engine in this process ([`InProcess`]); another
//! spawns it inside a WSL distro or on an SSH host and speaks JSON over its
//! stdio ([`child::ChildProcess`]). The Tauri commands in `lib.rs` cannot tell
//! which they are talking to, which is the whole point — watching files over a
//! network filesystem does not work, so the observer has to move, not the UI.

pub mod child;

use std::sync::{Arc, Mutex};

use agentlens_core::engine::Engine;
use agentlens_core::protocol::{Command, CommandResult, ConnectionInfo};
use agentlens_core::watcher::EventSink;
use serde_json::Value;

/// A place commands can be run and events can come back from.
pub trait Backend: Send + Sync + 'static {
    /// Run `command` where the files are, and return its JSON result.
    fn send(&self, command: Command) -> CommandResult<Value>;

    /// How this connection presents in the status bar.
    fn info(&self) -> ConnectionInfo;

    /// Stop watching and release whatever the transport holds. Must be safe to
    /// call more than once — closing a window and switching connections both
    /// land here.
    fn shutdown(&self);
}

/// The engine running in this process, watching this machine's filesystem.
///
/// The zero-regression path: a local workspace goes through exactly the code
/// it did before the split, one function call deep.
pub struct InProcess {
    engine: Engine,
}

impl InProcess {
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        InProcess {
            engine: Engine::new(sink),
        }
    }
}

impl Backend for InProcess {
    fn send(&self, command: Command) -> CommandResult<Value> {
        self.engine.handle(command)
    }

    fn info(&self) -> ConnectionInfo {
        ConnectionInfo::default()
    }

    fn shutdown(&self) {
        self.engine.shutdown();
    }
}

/// Tauri-managed state holding the backend currently in use.
///
/// Swappable, because connecting to a WSL distro or an SSH host replaces the
/// backend wholesale rather than reconfiguring one — a workspace belongs to
/// exactly one machine.
pub struct BackendState(Mutex<Arc<dyn Backend>>);

impl BackendState {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        BackendState(Mutex::new(backend))
    }

    /// The backend in use. Cloned out rather than held under the lock, so a
    /// slow remote command can't block the connection status the status bar
    /// is asking for.
    pub fn current(&self) -> CommandResult<Arc<dyn Backend>> {
        let guard = self.0.lock().map_err(|_| "backend state poisoned")?;
        Ok(guard.clone())
    }

    /// Install `backend` and shut the previous one down. Returns nothing: the
    /// old backend is finished with by the time this returns.
    pub fn replace(&self, backend: Arc<dyn Backend>) -> CommandResult<()> {
        let previous = {
            let mut guard = self.0.lock().map_err(|_| "backend state poisoned")?;
            std::mem::replace(&mut *guard, backend)
        };
        previous.shutdown();
        Ok(())
    }
}
