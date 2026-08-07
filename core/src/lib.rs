//! Everything AgentLens does to observe a directory, with no UI attached.
//!
//! Filesystem watching, git status and operations, previews, session
//! snapshots, and agent transcript tailing all live here. Nothing in this
//! crate depends on Tauri, on a window, or on there being a user present.
//!
//! That constraint is the point. Phase 4 runs this same code as a headless
//! daemon inside a WSL distro or on an SSH host, streaming protocol messages
//! to a UI running somewhere else — watching files over a network filesystem
//! does not work, so the observer has to live where the files are. Keeping
//! the split honest from the start is cheaper than discovering later that the
//! watcher cannot be lifted out.
//!
//! The seam is `protocol`: every type crossing between this crate and a
//! front end is serializable, so the transport can become a pipe without any
//! of these modules noticing.

pub mod agents;
pub mod browse;
pub mod correlate;
pub mod engine;
pub mod gitops;
pub mod gitstatus;
pub mod ignores;
pub mod paths;
pub mod preview;
pub mod protocol;
pub mod settings;
pub mod snapshots;
pub mod tree;
pub mod visibility;
pub mod watcher;
pub mod workspace;
