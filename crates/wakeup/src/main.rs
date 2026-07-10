//! wakeup - a `caffeinate`-compatible keep-awake utility for macOS.
//!
//! This tool creates IOKit power assertions directly, without shelling out to the
//! `/usr/bin/caffeinate` binary.
//!
//! Flags mirror the common `caffeinate` interface:
//!   -d  prevent the display from sleeping        (PreventUserIdleDisplaySleep)
//!   -i  prevent the system from idle sleeping     (PreventUserIdleSystemSleep)
//!   -m  prevent the disk from idle sleeping       (PreventDiskIdle)
//!   -s  prevent system sleep (only on AC power)   (PreventSystemSleep)
//!   -u  declare the user is active (wakes display)(UserIsActive, 5s default)
//!   -t <seconds>  hold the assertion for N seconds, then exit
//!   -w <pid>      hold until the given process exits
//!   [command ...] hold while running command, then exit with its status
//!
//! With no assertion flag, `-i` is assumed (same default as `caffeinate`).

use std::os::raw::c_int;
use std::process::Command;
use std::time::{Duration, Instant};

// ---- CoreFoundation / IOKit FFI ------------------------------------------ //

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::{c_void, CString};
    use std::os::raw::{c_char, c_int};

    type CFStringRef = *const c_void;
    type IOPMAssertionID = u32;

    const KCF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const KIOPM_ASSERTION_LEVEL_ON: u32 = 255;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFRelease(cf: *const c_void);
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionCreateWithName(
            assertion_type: CFStringRef,
            assertion_level: u32,
            assertion_name: CFStringRef,
            assertion_id: *mut IOPMAssertionID,
        ) -> c_int;
        fn IOPMAssertionRelease(assertion_id: IOPMAssertionID) -> c_int;
    }

    fn cfstr(s: &str) -> Result<CFStringRef, String> {
        let c = CString::new(s).map_err(|_| "string contained a NUL byte".to_string())?;
        let r = unsafe {
            CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), KCF_STRING_ENCODING_UTF8)
        };
        if r.is_null() {
            return Err("failed to allocate CFString".to_string());
        }
        Ok(r)
    }

    /// An RAII power assertion: released automatically on drop (and by the kernel
    /// if the process dies, which is how Ctrl-C is handled).
    pub struct Assertion {
        id: IOPMAssertionID,
    }

    impl Assertion {
        pub fn new(assertion_type: &str, reason: &str) -> Result<Self, String> {
            let atype = cfstr(assertion_type)?;
            let aname = cfstr(reason)?;
            let mut id: IOPMAssertionID = 0;
            let rc = unsafe {
                IOPMAssertionCreateWithName(atype, KIOPM_ASSERTION_LEVEL_ON, aname, &mut id)
            };
            unsafe {
                CFRelease(atype);
                CFRelease(aname);
            }
            if rc == 0 {
                Ok(Assertion { id })
            } else {
                Err(format!("IOPMAssertionCreateWithName returned {rc}"))
            }
        }
    }

    impl Drop for Assertion {
        fn drop(&mut self) {
            unsafe { IOPMAssertionRelease(self.id) };
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    pub struct Assertion;

    impl Assertion {
        pub fn new(_assertion_type: &str, _reason: &str) -> Result<Self, String> {
            Err("power assertions are currently implemented only on macOS".to_string())
        }
    }
}

use platform::Assertion;

// ---- CLI ------------------------------------------------------------------ //

const USAGE: &str = "\
wakeup - keep macOS awake with direct IOKit assertions

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

// ---- main ----------------------------------------------------------------- //

#[cfg(unix)]
fn pid_alive(pid: i32) -> bool {
    // kill(pid, 0): 0 => alive, EPERM => alive (not ours), ESRCH => gone.
    let rc = unsafe { libc_kill(pid, 0) };
    if rc == 0 {
        return true;
    }
    // errno is not easily portable without libc; treat EPERM as alive.
    errno() == EPERM
}

#[cfg(not(unix))]
fn pid_alive(_pid: i32) -> bool {
    false
}

#[cfg(unix)]
extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: c_int) -> c_int;
}

#[cfg(all(unix, target_os = "macos"))]
extern "C" {
    #[link_name = "__error"]
    fn libc_errno_location() -> *mut c_int;
}

#[cfg(all(unix, not(target_os = "macos")))]
extern "C" {
    #[link_name = "__errno_location"]
    fn libc_errno_location() -> *mut c_int;
}

#[cfg(unix)]
const EPERM: c_int = 1;

#[cfg(unix)]
fn errno() -> c_int {
    unsafe { *libc_errno_location() }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AssertionKind {
    Normal,
    UserActive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AssertionSpec {
    assertion_type: &'static str,
    reason: &'static str,
    kind: AssertionKind,
}

struct HeldAssertion {
    _assertion: Assertion,
    release_at: Option<Instant>,
}

fn assertion_specs(opts: &Opts) -> Vec<AssertionSpec> {
    let mut want = Vec::new();
    let any_explicit =
        opts.display || opts.idle_system || opts.disk || opts.system || opts.user_active;

    if opts.idle_system || !any_explicit {
        want.push(AssertionSpec {
            assertion_type: "PreventUserIdleSystemSleep",
            reason: "wakeup: preventing idle system sleep",
            kind: AssertionKind::Normal,
        });
    }
    if opts.display {
        want.push(AssertionSpec {
            assertion_type: "PreventUserIdleDisplaySleep",
            reason: "wakeup: preventing display sleep",
            kind: AssertionKind::Normal,
        });
    }
    if opts.disk {
        want.push(AssertionSpec {
            assertion_type: "PreventDiskIdle",
            reason: "wakeup: preventing disk idle sleep",
            kind: AssertionKind::Normal,
        });
    }
    if opts.system {
        want.push(AssertionSpec {
            assertion_type: "PreventSystemSleep",
            reason: "wakeup: preventing system sleep (AC only)",
            kind: AssertionKind::Normal,
        });
    }
    if opts.user_active {
        want.push(AssertionSpec {
            assertion_type: "UserIsActive",
            reason: "wakeup: user is active",
            kind: AssertionKind::UserActive,
        });
    }

    want
}

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

fn create_assertions(opts: &Opts) -> Vec<HeldAssertion> {
    let specs = assertion_specs(opts);
    let user_active_release = user_active_default_release(opts, Instant::now());
    let mut held = Vec::with_capacity(specs.len());

    for spec in specs {
        match Assertion::new(spec.assertion_type, spec.reason) {
            Ok(assertion) => held.push(HeldAssertion {
                _assertion: assertion,
                release_at: if spec.kind == AssertionKind::UserActive {
                    user_active_release
                } else {
                    None
                },
            }),
            Err(e) => {
                eprintln!(
                    "wakeup: could not create {} assertion ({e})",
                    spec.assertion_type
                );
                std::process::exit(1);
            }
        }
    }

    held
}

fn drop_expired_assertions(held: &mut Vec<HeldAssertion>) {
    let now = Instant::now();
    let mut i = 0;
    while i < held.len() {
        if held[i].release_at.map(|t| now >= t).unwrap_or(false) {
            held.swap_remove(i);
        } else {
            i += 1;
        }
    }
}

fn next_sleep_until_release(held: &[HeldAssertion], fallback: Duration) -> Duration {
    let now = Instant::now();
    held.iter()
        .filter_map(|h| h.release_at.map(|t| t.saturating_duration_since(now)))
        .min()
        .unwrap_or(fallback)
}

fn main() {
    let opts = parse_args();

    // Hold the assertions for the lifetime of `held`.
    let mut held = create_assertions(&opts);

    // Mode 1: run a command, hold while it runs.
    if !opts.command.is_empty() {
        let status = Command::new(&opts.command[0])
            .args(&opts.command[1..])
            .status();
        let code = match status {
            Ok(s) => s.code().unwrap_or(1),
            Err(e) => {
                eprintln!("wakeup: failed to run {}: {e}", opts.command[0]);
                126
            }
        };
        std::process::exit(code);
    }

    // Mode 2: wait for a pid to exit.
    if let Some(pid) = opts.wait_pid {
        while pid_alive(pid) {
            drop_expired_assertions(&mut held);
            std::thread::sleep(
                next_sleep_until_release(&held, Duration::from_millis(500))
                    .min(Duration::from_millis(500)),
            );
        }
        return;
    }

    // Mode 3: hold for a fixed duration. Ctrl-C still exits early and the
    // kernel releases the assertion.
    if let Some(secs) = opts.timeout {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            drop_expired_assertions(&mut held);
            let until_timeout = deadline.saturating_duration_since(Instant::now());
            std::thread::sleep(next_sleep_until_release(&held, until_timeout).min(until_timeout));
        }
        return;
    }

    // Mode 4: hold until killed (Ctrl-C). Kernel releases the assertion on exit.
    while !held.is_empty() {
        drop_expired_assertions(&mut held);
        if held.is_empty() {
            break;
        }
        std::thread::sleep(next_sleep_until_release(&held, Duration::from_secs(3600)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(args: &[&str]) -> Opts {
        parse_args_from(args.iter().copied())
    }

    #[test]
    fn defaults_to_idle_system_assertion() {
        let specs = assertion_specs(&opts(&[]));
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].assertion_type, "PreventUserIdleSystemSleep");
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
        let specs = assertion_specs(&opts(&["-m"]));
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].assertion_type, "PreventDiskIdle");
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
