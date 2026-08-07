//! Correlation engine: join filesystem events to the agent tool calls that
//! caused them.
//!
//! The watcher and the agent poller never meet on their own. This module is
//! the join: an [`EventSink`] decorator that sits between the watcher and the
//! real sink, holds recently-seen tool calls in a time window, and tags
//! matching [`FsEvent`]s with an [`Attribution`].
//!
//! Rules, in order of importance:
//!
//! 1. **Prefer under-claiming.** A wrong badge is worse than a missing one.
//! 2. A tool call with explicit paths claims matching fs events within
//!    ±[`WINDOW_MS`] of the call. The window is symmetric because the
//!    transcript write can lag *or lead* the actual filesystem write.
//! 3. Shell tools (`Bash`, `run_terminal_command`) carry no paths. They claim
//!    otherwise-unclaimed events in their window as lower-confidence
//!    "via command", capped at [`COMMAND_PATH_CAP`] so a `cargo build` does
//!    not attribute hundreds of files.
//! 4. Unclaimed events pass through with `attribution: None` — the user's
//!    own editor edits must stay visible and visibly unattributed.
//! 5. Entries older than the window are pruned so memory does not grow
//!    across a long session.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::protocol::{
    AgentEvent, AgentKind, AttributedEvent, Attribution, FsEvent, GitStatusSnapshot, WatcherStatus,
};
use crate::watcher::EventSink;

/// Half-width of the correlation window, in milliseconds. Symmetric: an fs
/// event 3 s before a tool call and one 3 s after it are both candidates.
/// Outside that, prefer leaving the event unattributed.
pub const WINDOW_MS: i64 = 3_000;

/// Maximum number of fs paths one shell tool call may claim. Beyond this the
/// rest stay unattributed rather than listing every file a build touched.
const COMMAND_PATH_CAP: usize = 20;

/// Tool names that name a command, not a path. They participate in
/// via-command claiming only — never as explicit path matches.
fn is_command_tool(tool: &str) -> bool {
    tool == "Bash" || tool == "run_terminal_command"
}

/// A recently-seen tool call waiting to claim fs events.
#[derive(Debug, Clone)]
struct PendingCall {
    at: i64,
    session_id: String,
    agent: AgentKind,
    tool: String,
    summary: Option<String>,
    /// Workspace-relative paths the tool named. Empty for shell tools.
    paths: HashSet<String>,
    /// True for `Bash` / `run_terminal_command`.
    via_command: bool,
    /// How many via-command claims this call has already taken — enforces
    /// [`COMMAND_PATH_CAP`].
    command_claims: usize,
}

/// Joins agent tool calls to filesystem events, then forwards everything to
/// an inner sink.
///
/// Inserted between the watcher and the real sink so `watcher.rs` does not
/// need to know correlation exists. Tool calls arrive from the agent poller
/// via [`Correlator::observe_tool_call`]; fs events arrive through the
/// [`EventSink`] impl.
pub struct Correlator {
    inner: Arc<dyn EventSink>,
    state: Mutex<State>,
}

/// Most recently-emitted unattributed events held for late upgrade. Bounded
/// so a burst of unattributed churn cannot grow without limit; the window
/// prune usually gets there first.
const UNCLAIMED_CAP: usize = 500;

#[derive(Default)]
struct State {
    pending: Vec<PendingCall>,
    /// Events already emitted with no attribution, still inside the window
    /// and therefore still claimable by a tool call that has not arrived yet.
    ///
    /// This is what makes the symmetric window real rather than theoretical.
    /// The watcher debounces for 300 ms while the agent poller runs on a
    /// 1 s interval, so the *ordinary* case is that a file change is seen
    /// before the tool call that caused it — attributing only against calls
    /// already known would therefore miss most of them.
    unclaimed: Vec<FsEvent>,
}

impl Correlator {
    pub fn new(inner: Arc<dyn EventSink>) -> Self {
        Correlator {
            inner,
            state: Mutex::new(State::default()),
        }
    }

    /// Record a tool call so later (or earlier-within-window) fs events can
    /// claim it. Called by the agent poller for every `ToolCall` it sees.
    ///
    /// `agent` is passed separately because `AgentEvent::ToolCall` does not
    /// carry it — the poller already knows which provider the session
    /// belongs to.
    pub fn observe_tool_call(
        &self,
        session_id: &str,
        agent: AgentKind,
        at: i64,
        tool: &str,
        summary: Option<&str>,
        paths: &[String],
    ) {
        let via_command = is_command_tool(tool);
        // Shell tools with accidental path args still count as via-command;
        // explicit-path tools with an empty path list claim nothing (under-
        // claim rather than treating every empty-path call as a shell).
        let path_set: HashSet<String> = if via_command {
            HashSet::new()
        } else {
            paths.iter().cloned().collect()
        };
        if !via_command && path_set.is_empty() {
            return;
        }

        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.pending.push(PendingCall {
            at,
            session_id: session_id.to_string(),
            agent,
            tool: tool.to_string(),
            summary: summary.map(str::to_string),
            paths: path_set,
            via_command,
            command_claims: 0,
        });
        // Prune against this call's timestamp so a long-running session does
        // not keep every tool call ever seen.
        state.prune(at);

        // The call may explain events already emitted unattributed. Re-claim
        // them now and emit the upgrade so the feed can revise those rows in
        // place — without this the symmetric window only ever works when the
        // call happens to be seen first, which is the rarer ordering.
        let mut upgrades = Vec::new();
        let mut still_unclaimed = Vec::new();
        for event in std::mem::take(&mut state.unclaimed) {
            match state.claim(&event) {
                Some(attribution) => upgrades.push(AttributedEvent {
                    event,
                    attribution: Some(attribution),
                }),
                None => still_unclaimed.push(event),
            }
        }
        state.unclaimed = still_unclaimed;
        // Released before touching the sink: the inner sink serialises and
        // emits, and holding the correlator's lock across that would put an
        // unrelated component's latency inside our critical section.
        drop(state);

        if !upgrades.is_empty() {
            self.inner.attributed_changes(&upgrades);
        }
    }

    /// Convenience: feed a whole batch of agent events, observing only the
    /// `ToolCall`s. Other event kinds are ignored here — the poller still
    /// forwards the full batch to the sink for UI session state.
    pub fn observe_agent_events(&self, events: &[AgentEvent], agent: AgentKind) {
        for event in events {
            if let AgentEvent::ToolCall {
                session_id,
                at,
                tool,
                summary,
                paths,
                ..
            } = event
            {
                self.observe_tool_call(session_id, agent, *at, tool, summary.as_deref(), paths);
            }
        }
    }

    /// Attribute a batch of fs events against currently pending tool calls.
    /// Public so unit tests can exercise the join without a running watcher.
    pub fn attribute(&self, events: &[FsEvent]) -> Vec<AttributedEvent> {
        let Ok(mut state) = self.state.lock() else {
            return events
                .iter()
                .map(|event| AttributedEvent {
                    event: event.clone(),
                    attribution: None,
                })
                .collect();
        };
        // Reference time for pruning: the newest event in the batch, or
        // leave pending alone if the batch is empty.
        if let Some(newest) = events.iter().map(|e| e.at).max() {
            state.prune(newest);
        }
        events
            .iter()
            .map(|event| {
                let attribution = state.claim(event);
                if attribution.is_none() {
                    // Hold it: a tool call inside the window may still arrive
                    // and claim it, at which point `observe_tool_call` emits
                    // the upgrade. See `State::unclaimed`.
                    state.unclaimed.push(event.clone());
                }
                AttributedEvent {
                    event: event.clone(),
                    attribution,
                }
            })
            .collect()
    }

    /// How many tool calls are still held. Test-only: proves prune drops
    /// entries outside the window.
    #[cfg(test)]
    pub fn pending_count(&self) -> usize {
        self.state.lock().map(|s| s.pending.len()).unwrap_or(0)
    }
}

impl State {
    /// Drop tool calls that can no longer match any future fs event at or
    /// after `now`: a call older than `now - WINDOW_MS` is outside every
    /// remaining window.
    fn prune(&mut self, now: i64) {
        let cutoff = now.saturating_sub(WINDOW_MS);
        self.pending.retain(|call| call.at >= cutoff);
        self.unclaimed.retain(|event| event.at >= cutoff);
        if self.unclaimed.len() > UNCLAIMED_CAP {
            let excess = self.unclaimed.len() - UNCLAIMED_CAP;
            self.unclaimed.drain(..excess);
        }
    }

    /// Try to claim `event` for one pending tool call. Explicit path matches
    /// beat via-command claims; among equals, the closest-in-time wins so a
    /// later unrelated call does not steal a path.
    fn claim(&mut self, event: &FsEvent) -> Option<Attribution> {
        // Pass 1: explicit path match.
        let mut best_explicit: Option<usize> = None;
        let mut best_explicit_delta = i64::MAX;
        for (index, call) in self.pending.iter().enumerate() {
            if call.via_command {
                continue;
            }
            if !call.paths.contains(&event.path) {
                continue;
            }
            let delta = (event.at - call.at).abs();
            if delta > WINDOW_MS {
                continue;
            }
            if delta < best_explicit_delta {
                best_explicit_delta = delta;
                best_explicit = Some(index);
            }
        }
        if let Some(index) = best_explicit {
            return Some(self.pending[index].to_attribution(false));
        }

        // Pass 2: via-command, only for still-under-cap shell tools.
        let mut best_command: Option<usize> = None;
        let mut best_command_delta = i64::MAX;
        for (index, call) in self.pending.iter().enumerate() {
            if !call.via_command {
                continue;
            }
            if call.command_claims >= COMMAND_PATH_CAP {
                continue;
            }
            let delta = (event.at - call.at).abs();
            if delta > WINDOW_MS {
                continue;
            }
            if delta < best_command_delta {
                best_command_delta = delta;
                best_command = Some(index);
            }
        }
        if let Some(index) = best_command {
            self.pending[index].command_claims += 1;
            return Some(self.pending[index].to_attribution(true));
        }

        None
    }
}

impl PendingCall {
    fn to_attribution(&self, via_command: bool) -> Attribution {
        Attribution {
            session_id: self.session_id.clone(),
            agent: self.agent,
            tool: self.tool.clone(),
            summary: self.summary.clone(),
            via_command,
        }
    }
}

impl EventSink for Correlator {
    fn fs_changes(&self, events: &[FsEvent]) {
        // Raw stream first so a feed that only listens for fs-changes still
        // paints immediately; attributed_changes follows for in-place upgrade.
        self.inner.fs_changes(events);
        let attributed = self.attribute(events);
        if !attributed.is_empty() {
            self.inner.attributed_changes(&attributed);
        }
    }

    fn git_status(&self, snapshot: &GitStatusSnapshot) {
        self.inner.git_status(snapshot);
    }

    fn watcher_status(&self, status: &WatcherStatus) {
        self.inner.watcher_status(status);
    }

    fn attributed_changes(&self, events: &[AttributedEvent]) {
        self.inner.attributed_changes(events);
    }

    fn agent_events(&self, events: &[AgentEvent]) {
        self.inner.agent_events(events);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::FsEventKind;

    fn fs(path: &str, at: i64) -> FsEvent {
        FsEvent {
            kind: FsEventKind::Modified,
            path: path.to_string(),
            is_dir: false,
            at,
        }
    }

    fn correlator() -> Correlator {
        // Inner sink is never consulted by `attribute`; a no-op is enough.
        struct Discard;
        impl EventSink for Discard {
            fn fs_changes(&self, _: &[FsEvent]) {}
            fn git_status(&self, _: &GitStatusSnapshot) {}
            fn watcher_status(&self, _: &WatcherStatus) {}
        }
        Correlator::new(Arc::new(Discard))
    }

    /// A sink that records the attributed batches pushed to it, so the
    /// late-upgrade path can be observed rather than inferred.
    #[derive(Default)]
    struct Recording(Mutex<Vec<Vec<AttributedEvent>>>);

    impl EventSink for Recording {
        fn fs_changes(&self, _: &[FsEvent]) {}
        fn git_status(&self, _: &GitStatusSnapshot) {}
        fn watcher_status(&self, _: &WatcherStatus) {}
        fn attributed_changes(&self, events: &[AttributedEvent]) {
            self.0.lock().unwrap().push(events.to_vec());
        }
    }

    #[test]
    fn a_call_arriving_after_its_file_change_upgrades_it_in_place() {
        // The ordinary production ordering: the watcher debounces for 300 ms
        // while the agent poller runs on a 1 s interval, so the file change is
        // normally seen *before* the tool call that caused it. Attributing
        // only against calls already known would miss almost everything.
        let sink = Arc::new(Recording::default());
        let correlator = Correlator::new(sink.clone());

        // File change lands first and goes out unattributed.
        let first = correlator.attribute(&[fs("src/main.rs", 10_000)]);
        assert!(
            first[0].attribution.is_none(),
            "nothing has claimed it yet: {first:?}"
        );
        assert!(sink.0.lock().unwrap().is_empty(), "no upgrade emitted yet");

        // The call turns up a beat later, still inside the window.
        note_edit(&correlator, 10_800, "src/main.rs");

        let batches = sink.0.lock().unwrap();
        assert_eq!(batches.len(), 1, "exactly one upgrade batch: {batches:?}");
        assert_eq!(batches[0].len(), 1);
        let upgraded = &batches[0][0];
        assert_eq!(upgraded.event.path, "src/main.rs");
        let attribution = upgraded
            .attribution
            .as_ref()
            .expect("the late call must claim the earlier change");
        assert_eq!(attribution.tool, "Edit");
        assert!(!attribution.via_command);
    }

    #[test]
    fn a_call_arriving_outside_the_window_does_not_upgrade_anything() {
        // Under-claiming is the standing rule: a call far away in time must
        // not retro-fit an explanation onto an unrelated edit.
        let sink = Arc::new(Recording::default());
        let correlator = Correlator::new(sink.clone());

        correlator.attribute(&[fs("src/main.rs", 10_000)]);
        note_edit(&correlator, 10_000 + WINDOW_MS + 1, "src/main.rs");

        assert!(
            sink.0.lock().unwrap().is_empty(),
            "a call outside the window must claim nothing"
        );
    }

    fn note_edit(c: &Correlator, at: i64, path: &str) {
        c.observe_tool_call(
            "sess-1",
            AgentKind::ClaudeCode,
            at,
            "Edit",
            None,
            &[path.to_string()],
        );
    }

    fn note_bash(c: &Correlator, at: i64) {
        c.observe_tool_call(
            "sess-1",
            AgentKind::ClaudeCode,
            at,
            "Bash",
            Some("cargo build"),
            &[],
        );
    }

    fn note_shell(c: &Correlator, at: i64) {
        c.observe_tool_call(
            "sess-2",
            AgentKind::Grok,
            at,
            "run_terminal_command",
            Some("cargo test"),
            &[],
        );
    }

    #[test]
    fn a_tool_call_with_an_explicit_path_claims_an_fs_event_one_second_after_it() {
        let c = correlator();
        note_edit(&c, 10_000, "src/main.rs");
        let out = c.attribute(&[fs("src/main.rs", 11_000)]);
        assert_eq!(out.len(), 1);
        let attr = out[0].attribution.as_ref().expect("claimed");
        assert_eq!(attr.session_id, "sess-1");
        assert_eq!(attr.tool, "Edit");
        assert!(!attr.via_command);
    }

    #[test]
    fn a_tool_call_claims_an_fs_event_one_second_before_it() {
        // Lead case: the transcript write can precede the actual fs write,
        // so the tool call's timestamp is *after* the fs event's. The
        // window is symmetric so this still matches.
        let c = correlator();
        note_edit(&c, 10_000, "src/main.rs");
        let out = c.attribute(&[fs("src/main.rs", 9_000)]);
        assert!(
            out[0].attribution.is_some(),
            "fs 1 s before the call must still be claimed"
        );
    }

    #[test]
    fn an_fs_event_thirty_seconds_away_is_not_claimed() {
        let c = correlator();
        note_edit(&c, 10_000, "src/main.rs");
        let out = c.attribute(&[fs("src/main.rs", 40_000)]);
        assert!(
            out[0].attribution.is_none(),
            "30 s is well outside the ±3 s window"
        );
    }

    #[test]
    fn run_terminal_command_and_bash_claim_unclaimed_events_flagged_via_command() {
        let c = correlator();
        // Attribute each call in its own window. Noting a later call first
        // would prune the earlier one (correctly — it can no longer match
        // anything still in flight), which is not what this test is about.
        note_bash(&c, 10_000);
        let bash_out = c.attribute(&[fs("target/foo.o", 10_500)]);
        let attr = bash_out[0].attribution.as_ref().expect("bash claims");
        assert_eq!(attr.tool, "Bash");
        assert!(attr.via_command);
        assert_eq!(attr.summary.as_deref(), Some("cargo build"));

        note_shell(&c, 20_000);
        let shell_out = c.attribute(&[fs("target/bar.o", 20_500)]);
        let attr = shell_out[0].attribution.as_ref().expect("shell claims");
        assert_eq!(attr.tool, "run_terminal_command");
        assert!(attr.via_command);
        assert_eq!(attr.agent, AgentKind::Grok);
    }

    #[test]
    fn fan_out_cap_holds_at_most_twenty_paths_per_shell_call() {
        let c = correlator();
        note_bash(&c, 10_000);
        let events: Vec<FsEvent> = (0..100)
            .map(|i| fs(&format!("target/obj{i}.o"), 10_000 + i as i64))
            .collect();
        let out = c.attribute(&events);
        let claimed = out.iter().filter(|e| e.attribution.is_some()).count();
        assert_eq!(
            claimed, COMMAND_PATH_CAP,
            "100 events in a shell call's window must yield at most {COMMAND_PATH_CAP} attributions, got {claimed}"
        );
        // The overflow stays visibly unattributed rather than being forced.
        assert!(out[COMMAND_PATH_CAP].attribution.is_none());
    }

    #[test]
    fn an_fs_event_with_no_matching_call_passes_through_unattributed() {
        let c = correlator();
        // A call for a different path must not steal this one.
        note_edit(&c, 10_000, "src/other.rs");
        let out = c.attribute(&[fs("src/main.rs", 10_000)]);
        assert!(
            out[0].attribution.is_none(),
            "no path match and no shell tool → external edit"
        );
    }

    #[test]
    fn entries_older_than_the_window_are_pruned() {
        let c = correlator();
        note_edit(&c, 10_000, "src/main.rs");
        assert_eq!(c.pending_count(), 1);
        // An event just past the window forces prune against its timestamp.
        let _ = c.attribute(&[fs("unrelated.rs", 10_000 + WINDOW_MS + 1)]);
        assert_eq!(
            c.pending_count(),
            0,
            "a call older than now − WINDOW_MS must be dropped"
        );
    }

    #[test]
    fn explicit_path_match_beats_a_competing_shell_call() {
        // Prefer under-claiming wrong shell badges: a path-bearing tool in
        // the same window owns the file it named.
        let c = correlator();
        note_bash(&c, 10_000);
        note_edit(&c, 10_100, "src/main.rs");
        let out = c.attribute(&[fs("src/main.rs", 10_200)]);
        let attr = out[0].attribution.as_ref().expect("claimed");
        assert_eq!(attr.tool, "Edit");
        assert!(!attr.via_command);
    }
}
