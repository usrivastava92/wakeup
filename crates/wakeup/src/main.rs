//! wakeup - a `caffeinate`-compatible keep-awake CLI.
//!
//! This binary only parses flags and drives `wakeup-core`'s `Session`; all
//! platform-specific power assertion logic lives in that crate so it can be
//! reused by other consumers (see the workspace `Cargo.toml`).
//!
//! Flags mirror the common `caffeinate` interface:
//!   -d  prevent the display from sleeping
//!   -i  prevent the system from idle sleeping
//!   -m  prevent the disk from idle sleeping
//!   -s  prevent system sleep (only on AC power, where the platform honors that)
//!   -u  declare the user is active (wakes the display, 5s default)
//!   -t <seconds>  hold the assertion for N seconds, then exit
//!   -w <pid>      hold until the given process exits
//!   [command ...] hold while running command, then exit with its status
//!
//! With no assertion flag, `-i` is assumed (same default as `caffeinate`).

use std::time::{Duration, Instant};
use wakeup_core::{AssertionKind, AssertionRequest, Session};

const USAGE: &str = "\
wakeup - a cross-platform, caffeinate-compatible keep-awake CLI

USAGE:
    wakeup [-dimsu] [-t seconds] [-w pid] [command [args...]]

OPTIONS:
    -d              Prevent the display from sleeping (caffeinate -d).
    -i              Prevent the system from idle sleeping (caffeinate -i). [default]
    -m              Prevent the disk from idle sleeping (caffeinate -m).
    -s              Prevent system sleep entirely; effective only on AC power.
    -u              Declare the user active and wake the display. Defaults to 5s without -t.
    -t <seconds>    Release after N seconds, then exit.
    -w <pid>        Release when process <pid> exits.
    -h, --help      Show this help.
    -V, --version   Show version.

If a command is given, wakeup holds the assertion while the command runs and
exits with the command's status. With no flags, -i is assumed. Press Ctrl-C to
release when running interactively.

Not every flag is supported on every platform yet; see the wakeup-core release
plan for what is implemented on macOS, Linux, and Windows today.

EXAMPLES:
    wakeup                 # keep the system awake until Ctrl-C
    wakeup -d              # also keep the display awake
    wakeup -m              # prevent disk idle sleep
    wakeup -u              # declare user activity for 5 seconds
    wakeup -t 3600         # stay awake for one hour
    wakeup -di make build  # keep system+display awake while `make build` runs
";

#[derive(Default)]
struct Opts {
    display: bool,
    idle_system: bool,
    disk: bool,
    system: bool,
    user_active: bool,
    timeout: Option<u64>,
    wait_pid: Option<i32>,
    command: Vec<String>,
}

fn parse_args() -> Opts {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from<I, S>(args: I) -> Opts
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut o = Opts::default();
    let mut args = args.into_iter().map(Into::into).peekable();

    while let Some(arg) = args.peek().cloned() {
        if o.command.is_empty() && arg.starts_with('-') && arg != "-" {
            args.next();
            match arg.as_str() {
                "-h" | "--help" => {
                    print!("{USAGE}");
                    std::process::exit(0);
                }
                "-V" | "--version" => {
                    println!("wakeup {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                "-t" => {
                    o.timeout = Some(next_num(&mut args, "-t"));
                }
                "-w" => {
                    o.wait_pid = Some(next_num::<i32>(&mut args, "-w"));
                }
                "--" => {
                    o.command.extend(args.by_ref());
                }
                s if s.starts_with("--") => {
                    fail(&format!("unknown option: {s}"));
                }
                s => {
                    // Bundled short flags, e.g. -di. -t/-w must take a value; if
                    // bundled, the remainder of the cluster is the value.
                    let mut chars = s.chars().skip(1).peekable();
                    while let Some(c) = chars.next() {
                        match c {
                            'd' => o.display = true,
                            'i' => o.idle_system = true,
                            'm' => o.disk = true,
                            's' => o.system = true,
                            'u' => o.user_active = true,
                            't' | 'w' => {
                                let rest: String = chars.by_ref().collect();
                                let val = if rest.is_empty() {
                                    next_raw(&mut args, &format!("-{c}"))
                                } else {
                                    rest
                                };
                                if c == 't' {
                                    o.timeout = Some(parse_num(&val, "-t"));
                                } else {
                                    o.wait_pid = Some(parse_num::<i32>(&val, "-w"));
                                }
                            }
                            other => fail(&format!("unknown option: -{other}")),
                        }
                    }
                }
            }
        } else {
            // First non-flag token begins the command.
            o.command.extend(args.by_ref());
        }
    }
    o
}

fn next_raw(args: &mut impl Iterator<Item = String>, flag: &str) -> String {
    match args.next() {
        Some(v) => v,
        None => fail(&format!("{flag} requires a value")),
    }
}

fn next_num<T: std::str::FromStr>(args: &mut impl Iterator<Item = String>, flag: &str) -> T {
    let v = next_raw(args, flag);
    parse_num(&v, flag)
}

fn parse_num<T: std::str::FromStr>(v: &str, flag: &str) -> T {
    match v.parse::<T>() {
        Ok(n) => n,
        Err(_) => fail(&format!("{flag} expects a number, got {v:?}")),
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("wakeup: {msg}");
    eprintln!("try `wakeup --help`");
    std::process::exit(2);
}

// ---- mapping CLI flags onto wakeup-core ------------------------------------ //

/// Build the platform-neutral assertion requests for the given flags, in the
/// same order and with the same default-to-`-i` behavior as the original
/// single-crate implementation.
fn assertion_requests(opts: &Opts) -> Vec<AssertionRequest> {
    let mut want = Vec::new();
    let any_explicit =
        opts.display || opts.idle_system || opts.disk || opts.system || opts.user_active;

    if opts.idle_system || !any_explicit {
        want.push(AssertionRequest::new(
            AssertionKind::IdleSystem,
            "wakeup: preventing idle system sleep",
        ));
    }
    if opts.display {
        want.push(AssertionRequest::new(
            AssertionKind::Display,
            "wakeup: preventing display sleep",
        ));
    }
    if opts.disk {
        want.push(AssertionRequest::new(
            AssertionKind::Disk,
            "wakeup: preventing disk idle sleep",
        ));
    }
    if opts.system {
        want.push(AssertionRequest::new(
            AssertionKind::System,
            "wakeup: preventing system sleep (AC only)",
        ));
    }
    if opts.user_active {
        want.push(AssertionRequest::new(
            AssertionKind::UserActive,
            "wakeup: user is active",
        ));
    }

    want
}

/// `-u`'s default: release after 5 seconds, but only in "direct" mode (no
/// explicit timeout, PID wait, or command was also given).
fn user_active_default_release(opts: &Opts, now: Instant) -> Option<Instant> {
    if opts.user_active
        && opts.timeout.is_none()
        && opts.wait_pid.is_none()
        && opts.command.is_empty()
    {
        Some(now + Duration::from_secs(5))
    } else {
        None
    }
}

fn main() {
    let opts = parse_args();
    let requests = assertion_requests(&opts);

    let mut session = match Session::create(&requests) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("wakeup: {e}");
            std::process::exit(1);
        }
    };

    if let Some(at) = user_active_default_release(&opts, Instant::now()) {
        session.set_release_at(AssertionKind::UserActive, at);
    }

    // Mode 1: run a command, hold while it runs.
    if !opts.command.is_empty() {
        let code = session.run_command(&opts.command[0], &opts.command[1..]);
        std::process::exit(code);
    }

    // Mode 2: wait for a pid to exit.
    if let Some(pid) = opts.wait_pid {
        session.wait_for_pid(pid, Duration::from_millis(500));
        return;
    }

    // Mode 3: hold for a fixed duration. Ctrl-C still exits early and the
    // kernel releases the assertion.
    if let Some(secs) = opts.timeout {
        session.hold_for(Duration::from_secs(secs));
        return;
    }

    // Mode 4: hold until killed (Ctrl-C) or every assertion has expired.
    session.hold_until_released(Duration::from_secs(3600));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(args: &[&str]) -> Opts {
        parse_args_from(args.iter().copied())
    }

    #[test]
    fn defaults_to_idle_system_assertion() {
        let reqs = assertion_requests(&opts(&[]));
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].kind, AssertionKind::IdleSystem);
    }

    #[test]
    fn parses_caffeinate_assertion_flags() {
        let o = opts(&["-dimsu"]);
        assert!(o.display);
        assert!(o.idle_system);
        assert!(o.disk);
        assert!(o.system);
        assert!(o.user_active);
    }

    #[test]
    fn supports_disk_idle_assertion() {
        let reqs = assertion_requests(&opts(&["-m"]));
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].kind, AssertionKind::Disk);
    }

    #[test]
    fn user_active_gets_default_timeout_only_in_direct_mode() {
        let direct = opts(&["-u"]);
        assert!(user_active_default_release(&direct, Instant::now()).is_some());

        let command = opts(&["-u", "make"]);
        assert!(user_active_default_release(&command, Instant::now()).is_none());

        let timeout = opts(&["-u", "-t", "10"]);
        assert!(user_active_default_release(&timeout, Instant::now()).is_none());
    }

    #[test]
    fn first_non_flag_begins_command() {
        let o = opts(&["-di", "make", "build", "-x"]);
        assert_eq!(o.command, vec!["make", "build", "-x"]);
    }
}
