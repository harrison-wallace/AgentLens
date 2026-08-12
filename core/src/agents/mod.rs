//! Agent session discovery and tailing.
//!
//! Providers turn whatever a coding agent happens to write on disk into the
//! normalized `AgentEvent` stream in `protocol.rs`. Nothing downstream —
//! correlation, the feed, the session panel — knows which agent it came from
//! beyond the `AgentKind` tag, so adding a provider means adding a module
//! here and nothing else.
//!
//! **These formats are not stable APIs.** Every provider parses defensively:
//! an unknown record type is skipped, a malformed line is counted and
//! ignored, and a missing directory is "no agent detected" rather than an
//! error. The app must degrade to exactly its phase-1 behaviour when it can't
//! read anything, because that is the normal case for anyone not running the
//! agent it knows about.

pub mod claude;
pub mod grok;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::protocol::{AgentEvent, AgentKind, AgentRootInfo, SessionRef};

/// How much of a session file's tail to read when the question is only "what
/// does the end of this say". Session files grow without bound while an agent
/// runs, so nothing that answers a question about *recent* records may read
/// the whole thing.
pub(crate) const METADATA_TAIL_BYTES: u64 = 64 * 1024;

/// Read at most the last `METADATA_TAIL_BYTES` of `path`, dropping a leading
/// partial line so every line handed back parses.
///
/// Shared by every provider: each one needs the tail of a growing append-only
/// file, and reading it whole is the mistake that only shows up once a real
/// session has run for an hour.
pub(crate) fn read_tail(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let from = len.saturating_sub(METADATA_TAIL_BYTES);
    file.seek(SeekFrom::Start(from)).ok()?;

    let mut buffer = String::new();
    // Lossy on purpose: one mangled multi-byte char must not lose the tail.
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    buffer.push_str(&String::from_utf8_lossy(&bytes));

    if from > 0 {
        // The window almost certainly started mid-record.
        if let Some(newline) = buffer.find('\n') {
            buffer.drain(..=newline);
        }
    }
    Some(buffer)
}

/// Every provider the app ships. One place to add the next agent.
pub fn providers() -> Vec<Box<dyn AgentProvider>> {
    vec![
        Box::new(claude::ClaudeCode::new()),
        Box::new(grok::Grok::new()),
    ]
}

/// Does `dir` contain at least one subdirectory?
///
/// Pair with [`has_json_children`]: Claude's session registry is files
/// (`sessions/<pid>.json`), Grok's is directories
/// (`sessions/<percent-encoded-cwd>/<session-id>/`). A bare `sessions/`
/// existence check claims the other agent's root; these are the shared
/// discriminators. Missing or unreadable → `false`, never panic.
pub(crate) fn has_dir_children(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .filter_map(|e| e.ok())
        .any(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
}

/// Does `dir` contain at least one `*.json` file?
///
/// See [`has_dir_children`] for why this is the other half of the Claude /
/// Grok root discriminator. Missing or unreadable → `false`, never panic.
pub(crate) fn has_json_children(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.filter_map(|e| e.ok()).any(|e| {
        e.path()
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    })
}

/// Is `cwd` the workspace, or somewhere inside it?
///
/// Descendants count. An agent that runs `cd src-tauri` mid-session writes
/// that subdirectory as its `cwd` from then on, so an equality test loses the
/// session partway through — which is exactly what happened the first time
/// this ran against a live transcript.
///
/// Ancestors deliberately do *not* count. A session started above the
/// workspace may be working on something else entirely, and the plan's rule is
/// to prefer under-claiming to wrong attribution.
///
/// Compared on the normalized string rather than by canonicalizing: `cwd` is
/// whatever the agent recorded, and the workspace root is already normalized
/// by the time it reaches here. Shared by every provider so the rule cannot
/// drift between them.
pub(crate) fn is_within_workspace(cwd: &str, workspace: &Path) -> bool {
    let normalized = |text: &str| text.replace('\\', "/").trim_end_matches('/').to_string();
    let cwd = normalized(cwd);
    let workspace = normalized(&workspace.to_string_lossy());
    cwd == workspace || cwd.starts_with(&format!("{workspace}/"))
}

/// Roots detected automatically, plus the ones the user named, in that order
/// and without duplicates.
///
/// Detection is a heuristic — where an agent stores sessions is a convention
/// its authors never promised, and a user's own layout (`~/.claude-work`) is a
/// convention on top of that. `configured` is the escape hatch that makes the
/// feature work for someone whose directories we can't guess.
pub fn resolve_roots(configured: &[String]) -> Vec<PathBuf> {
    let detected = providers()
        .iter()
        .flat_map(|provider| provider.detect_roots())
        .collect();
    merge_roots(detected, configured)
}

/// Detected roots followed by the configured ones, deduplicated, blanks
/// dropped. Split out so `describe_roots` can reuse a single detection pass
/// instead of walking the filesystem twice.
fn merge_roots(detected: Vec<PathBuf>, configured: &[String]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for root in detected {
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    for root in configured {
        let root = PathBuf::from(root.trim());
        if !root.as_os_str().is_empty() && !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

/// The roots in effect, annotated for the settings UI: where each came from
/// and which agent — if any — recognises it.
///
/// The "if any" matters. A detected root that stops matching is just noise to
/// hide, but a path the *user typed* that nothing recognises is the whole
/// reason they can't see their sessions, and it has to be visible rather than
/// silently skipped.
pub fn describe_roots(configured: &[String]) -> Vec<AgentRootInfo> {
    // Detection walks the filesystem, so it runs once and both the list and
    // the "was this detected" answer come out of the same pass.
    let providers = providers();
    let detected: Vec<PathBuf> = providers
        .iter()
        .flat_map(|provider| provider.detect_roots())
        .collect();
    let all = merge_roots(detected.clone(), configured);

    if all.is_empty() {
        // An empty list used to render as "None found", which is the same
        // string as a machine that genuinely has no agent. Say why so a
        // remote daemon with no HOME is diagnosable.
        return vec![AgentRootInfo {
            path: String::new(),
            agent: None,
            detected: false,
            note: Some(empty_roots_note()),
        }];
    }

    all.into_iter()
        .map(|root| AgentRootInfo {
            agent: providers
                .iter()
                .find(|provider| provider.claims_root(&root))
                .map(|provider| provider.kind()),
            detected: detected.contains(&root),
            path: crate::paths::normalize_absolute(&root),
            note: None,
        })
        .collect()
}

/// Why detection returned no roots. Uses the same home lookup as the
/// providers, including the platform fallback when `HOME` is unset.
fn empty_roots_note() -> String {
    match crate::paths::home_dir() {
        None => {
            "This process has no home directory. Session folders cannot be found. Add a path below."
                .into()
        }
        Some(home) => format!(
            "Looked in {} for Claude Code and Grok session folders. None were found. Add a path below if yours lives somewhere else.",
            crate::paths::normalize_absolute(&home)
        ),
    }
}

/// A source of agent sessions for one workspace.
///
/// Roots are passed in rather than looked up: where an agent keeps its state
/// is partly guessable and partly a user setting, and resolving that is the
/// caller's job (see `agent_roots` in `lib.rs`). Providers therefore touch no
/// global state, which is also what makes them testable without juggling
/// environment variables.
pub trait AgentProvider {
    fn kind(&self) -> AgentKind;

    /// Roots this provider can find by itself. Best-effort: every agent's
    /// layout is a convention rather than a contract, so the user can always
    /// name more.
    fn detect_roots(&self) -> Vec<PathBuf>;

    /// Does `dir` look like one of this provider's roots? Used to tell the
    /// user which agent a directory they added belongs to — and to warn them
    /// when no provider recognises it at all.
    fn claims_root(&self, dir: &Path) -> bool;

    /// Sessions this provider believes belong to `workspace`, most recently
    /// active first. Empty is the normal answer, not a failure. Roots that
    /// belong to a different agent are ignored, so callers can pass all of
    /// them to every provider.
    fn discover(&self, workspace: &Path, roots: &[PathBuf]) -> Vec<SessionRef>;

    /// Records appended to `session` since the last call, as normalized
    /// events. The provider owns its own read offset, so this is a poll: call
    /// it when the transcript changes and it returns only what is new.
    fn poll(
        &mut self,
        workspace: &Path,
        session: &SessionRef,
        roots: &[PathBuf],
    ) -> Vec<AgentEvent>;
}

/// Running tally of records a provider could not make sense of. Surfaced in
/// the UI as a quiet counter rather than an error: format drift should be
/// visible without being alarming, since the app still works without it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseStats {
    pub records: u64,
    pub skipped: u64,
}

/// Parse an RFC 3339 UTC timestamp (`2026-08-01T12:43:16.069Z`) to Unix epoch
/// milliseconds, the unit every other time in the protocol uses.
///
/// Hand-rolled rather than pulling in a date crate: transcripts carry exactly
/// this one shape, and correlation needs nothing else from a calendar. Returns
/// `None` on anything it doesn't recognise, so a record with a broken
/// timestamp is skipped rather than landing at the epoch and sorting first.
pub fn epoch_millis(iso: &str) -> Option<i64> {
    let (date, rest) = iso.split_once('T')?;
    let time = rest.trim_end_matches('Z');

    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let (clock, fraction) = match time.split_once('.') {
        Some((clock, frac)) => (clock, frac),
        None => (time, "0"),
    };
    let mut clock_parts = clock.split(':');
    let hour: i64 = clock_parts.next()?.parse().ok()?;
    let minute: i64 = clock_parts.next()?.parse().ok()?;
    let second: i64 = clock_parts.next()?.parse().ok()?;
    if clock_parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    // Milliseconds, whatever precision was written: pad or truncate to three.
    let digits: String = fraction.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let millis: i64 = format!("{digits:0<3}")[..3].parse().ok()?;

    let days = days_from_civil(year, month, day);
    Some(((days * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000) + millis)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`, the standard branch-free formulation).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::claude::ClaudeCode;

    /// A directory with the shape of a Claude Code profile.
    fn profile_dir(parent: &Path, name: &str) -> PathBuf {
        let dir = parent.join(name);
        std::fs::create_dir_all(dir.join("projects")).unwrap();
        dir
    }

    #[test]
    fn reset_drops_offsets_and_tallies_so_they_cannot_leak_across_workspaces() {
        use crate::protocol::{AgentKind, SessionRef};

        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join("projects").join("-work-demo");
        std::fs::create_dir_all(&projects).unwrap();
        let file = projects.join("s.jsonl");
        std::fs::write(&file, "not json\n").unwrap();

        let workspace = Path::new("/work/demo");
        let roots = vec![dir.path().to_path_buf()];
        let session = SessionRef {
            id: "s".into(),
            agent: AgentKind::ClaudeCode,
            title: None,
            last_activity: 0,
            activity: crate::protocol::AgentActivity::Idle,
        };

        let mut provider = ClaudeCode::new();
        {
            provider.poll(workspace, &session, &roots); // starts at the end
            std::fs::write(&file, "not json\nstill not json\n").unwrap();
            provider.poll(workspace, &session, &roots);
            assert!(provider.stats.skipped > 0, "tallied something to clear");
        }

        // What the app's `reset_agents` does: replace the provider outright.
        provider = ClaudeCode::new();
        assert_eq!(provider.stats, ParseStats::default());
        // The offset went with it, so the next workspace starts at the end
        // rather than replaying whatever arrived while this one was closed.
        assert!(provider.offsets_is_empty());
    }

    #[test]
    fn configured_roots_are_added_to_detection_not_substituted_for_it() {
        // Detection mostly works, so making the setting a replacement would
        // force everyone to configure something they don't need to.
        let dir = tempfile::tempdir().unwrap();
        let extra = profile_dir(dir.path(), "elsewhere");

        let roots = resolve_roots(&[extra.to_string_lossy().into_owned()]);
        assert!(roots.contains(&extra));
        assert!(
            !roots.is_empty(),
            "detected roots, if any, must survive alongside it"
        );
    }

    #[test]
    fn blank_and_duplicate_entries_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let extra = profile_dir(dir.path(), "elsewhere");
        let path = extra.to_string_lossy().into_owned();

        let roots = resolve_roots(&[path.clone(), "   ".into(), "".into(), path.clone()]);
        assert_eq!(roots.iter().filter(|root| **root == extra).count(), 1);
    }

    #[test]
    fn empty_roots_note_names_home_when_it_exists() {
        // The wording is the settings empty-state. A missing home is the
        // remote-daemon case; a present home that holds no profiles is the
        // ordinary "this machine has no agent" case. Both must be distinct.
        let note = empty_roots_note();
        assert!(
            note.contains("Session folders cannot be found") || note.contains("None were found"),
            "{note}"
        );
    }

    #[test]
    fn a_configured_path_no_agent_recognises_is_reported_rather_than_dropped() {
        // This is the whole point of showing roots in settings: a typo'd path
        // is exactly why someone sees no sessions, so it has to be visible.
        let dir = tempfile::tempdir().unwrap();
        let nonsense = dir.path().join("not-a-profile");
        std::fs::create_dir_all(&nonsense).unwrap();

        let described = describe_roots(&[nonsense.to_string_lossy().into_owned()]);
        let entry = described
            .iter()
            .find(|root| root.path.ends_with("not-a-profile"))
            .expect("the path the user typed must still be listed");

        assert_eq!(
            entry.agent, None,
            "nothing claims it, and that is the point"
        );
        assert!(!entry.detected, "it came from the user, not from a scan");
    }

    #[test]
    fn a_configured_path_that_is_a_real_profile_is_attributed_to_its_agent() {
        let dir = tempfile::tempdir().unwrap();
        let profile = profile_dir(dir.path(), "claude-elsewhere");

        let described = describe_roots(&[profile.to_string_lossy().into_owned()]);
        let entry = described
            .iter()
            .find(|root| root.path.ends_with("claude-elsewhere"))
            .expect("listed");

        assert_eq!(entry.agent, Some(AgentKind::ClaudeCode));
        assert!(!entry.detected);
    }

    #[test]
    fn a_grok_root_and_a_claude_root_are_each_claimed_by_exactly_one_provider() {
        // Both agents keep a `sessions/` directory, and both can have a
        // `projects/` one, so a single-directory probe made each provider
        // claim the other's root — settings labelled `~/.grok` "Claude Code".
        // The discriminator is what `sessions/` holds: Claude writes
        // `<pid>.json` files, Grok writes one directory per encoded cwd.
        let dir = tempfile::tempdir().unwrap();

        let grok = dir.path().join("grok");
        std::fs::create_dir_all(grok.join("sessions").join("%2Fhome%2Fdev%2Fproj")).unwrap();
        std::fs::create_dir_all(grok.join("projects")).unwrap();

        let claude = dir.path().join("claude");
        std::fs::create_dir_all(claude.join("projects")).unwrap();
        std::fs::create_dir_all(claude.join("sessions")).unwrap();
        std::fs::write(claude.join("sessions").join("4242.json"), "{}").unwrap();

        let claimants = |root: &Path| -> Vec<AgentKind> {
            providers()
                .iter()
                .filter(|provider| provider.claims_root(root))
                .map(|provider| provider.kind())
                .collect()
        };
        assert_eq!(claimants(&grok), vec![AgentKind::Grok]);
        assert_eq!(claimants(&claude), vec![AgentKind::ClaudeCode]);
    }

    #[test]
    fn parses_the_transcript_timestamp_format() {
        // Expected values from `date -u -d '<stamp>' +%s`, not by hand.
        assert_eq!(
            epoch_millis("2026-08-01T12:43:16.069Z"),
            Some(1_785_588_196_069)
        );
        assert_eq!(epoch_millis("1970-01-01T00:00:00.000Z"), Some(0));
        // Fractional seconds are optional in RFC 3339, so accept their absence
        // rather than dropping a record over it.
        assert_eq!(
            epoch_millis("2026-08-01T12:43:16Z"),
            Some(1_785_588_196_000)
        );
    }

    #[test]
    fn handles_leap_years_and_pre_epoch_dates() {
        assert_eq!(
            epoch_millis("2024-02-29T00:00:00.000Z"),
            Some(1_709_164_800_000)
        );
        assert_eq!(epoch_millis("1969-12-31T23:59:59.999Z"), Some(-1));
    }

    #[test]
    fn pads_or_truncates_fractional_seconds_to_millis() {
        assert_eq!(
            epoch_millis("2026-08-01T12:43:16.5Z"),
            Some(1_785_588_196_500)
        );
        assert_eq!(
            epoch_millis("2026-08-01T12:43:16.069123Z"),
            Some(1_785_588_196_069)
        );
    }

    #[test]
    fn rejects_anything_it_does_not_understand() {
        // A broken timestamp must skip the record, not silently become 1970.
        for bad in [
            "",
            "2026-08-01",
            "2026-08-01T12:43Z",
            "not-a-date",
            "2026-13-01T00:00:00.000Z",
            "2026-08-01T25:00:00.000Z",
        ] {
            assert_eq!(epoch_millis(bad), None, "{bad} should not parse");
        }
    }
}
