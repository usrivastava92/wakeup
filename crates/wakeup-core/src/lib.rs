//! wakeup-core - the cross-platform power assertion engine behind `wakeup`.
//!
//! This crate owns everything that does not need to be reimplemented by each
//! consumer:
//!
//!   - platform-neutral request/handle types (`AssertionKind`,
//!     `AssertionRequest`, `Handle`)
//!   - per-OS backends selected at compile time (macOS IOKit, Linux
//!     systemd-logind, Windows Power Request API, or a clear "unsupported"
//!     error on anything else)
//!   - the shared `Session` policy loop: fixed timeout, PID wait, run-a-command,
//!     and hold-until-killed modes, plus the "release this assertion at time T"
//!     bookkeeping used by `-u`'s default 5 second release.
//!
//! `wakeup` (the CLI) is a thin layer on top of this crate: it only parses
//! flags into `AssertionRequest`s and picks which `Session` method to call.
//! Other consumers (like the `herdr-wakeup` plugin) can depend on this crate
//! directly instead of shelling out to the `wakeup` binary, though that is not
//! required.

mod backend;

use std::process::Command;
use std::time::{Duration, Instant};

pub use backend::BackendError;

/// What kind of sleep/idle behavior an assertion should prevent.
///
/// These map to `caffeinate`'s `-d`/`-i`/`-m`/`-s`/`-u` flags. Not every kind
/// is implemented on every platform; see each backend module for what is
/// actually supported today and `BackendError` for how unsupported kinds are
/// reported.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AssertionKind {
    /// Prevent the display from sleeping.
    Display,
    /// Prevent the system from idle-sleeping.
    IdleSystem,
    /// Prevent the disk from idle-sleeping.
    Disk,
    /// Prevent system sleep entirely (some platforms only honor this on AC power).
    System,
    /// Declare the user active. Typically wakes the display and is held only briefly.
    UserActive,
}

impl AssertionKind {
    /// A short, human-readable, platform-neutral label (used in error messages).
    pub fn label(self) -> &'static str {
        backend::label(self)
    }
}

/// A single platform-neutral request to create one assertion.
#[derive(Clone, Debug)]
pub struct AssertionRequest {
    pub kind: AssertionKind,
    pub reason: String,
}

impl AssertionRequest {
    pub fn new(kind: AssertionKind, reason: impl Into<String>) -> Self {
        Self {
            kind,
            reason: reason.into(),
        }
    }
}

/// An RAII handle for one held assertion.
///
/// Dropping it releases the assertion through the active backend. Consumers
/// normally do not hold `Handle`s directly; `Session` owns them and exposes
/// the higher-level "hold until X" policies below.
pub struct Handle {
    kind: AssertionKind,
    _backend: backend::BackendAssertion,
    release_at: Option<Instant>,
}

impl Handle {
    pub fn kind(&self) -> AssertionKind {
        self.kind
    }
}

/// A set of currently-held assertions, plus the shared "how long to hold"
/// policy loops so every consumer does not have to reimplement timers, PID
/// polling, and command-mode plumbing.
#[derive(Default)]
pub struct Session {
    held: Vec<Handle>,
}

impl Session {
    /// Create every requested assertion in order.
    ///
    /// On the first failure, every assertion already created for this call is
    /// released (via `Drop`) and the error is returned; nothing is left
    /// dangling.
    pub fn create(requests: &[AssertionRequest]) -> Result<Session, BackendError> {
        let mut held = Vec::with_capacity(requests.len());
        for req in requests {
            let assertion = backend::create(req.kind, &req.reason)?;
            held.push(Handle {
                kind: req.kind,
                _backend: assertion,
                release_at: None,
            });
        }
        Ok(Session { held })
    }

    /// True if no assertions are currently held (for example, all have expired).
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// The number of assertions currently held.
    pub fn len(&self) -> usize {
        self.held.len()
    }

    /// Set an absolute release time for every held assertion of `kind`.
    ///
    /// Used for `-u`'s "declare user active for 5 seconds" default when no
    /// explicit timeout, PID, or command was given.
    pub fn set_release_at(&mut self, kind: AssertionKind, at: Instant) {
        for h in self.held.iter_mut().filter(|h| h.kind == kind) {
            h.release_at = Some(at);
        }
    }

    fn drop_expired(&mut self) {
        let now = Instant::now();
        self.held
            .retain(|h| !h.release_at.map(|t| now >= t).unwrap_or(false));
    }

    fn next_wake(&self, fallback: Duration) -> Duration {
        let now = Instant::now();
        self.held
            .iter()
            .filter_map(|h| h.release_at.map(|t| t.saturating_duration_since(now)))
            .min()
            .unwrap_or(fallback)
    }

    /// Hold until process `pid` exits, checking at least every `poll_interval`.
    ///
    /// Keeps running even if every assertion has already expired, matching
    /// `wakeup -w <pid>`'s "wait for the process no matter what" contract.
    pub fn wait_for_pid(mut self, pid: i32, poll_interval: Duration) {
        while pid_alive(pid) {
            self.drop_expired();
            std::thread::sleep(self.next_wake(poll_interval).min(poll_interval));
        }
    }

    /// Hold for exactly `duration`, then return (dropping releases everything).
    pub fn hold_for(mut self, duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            self.drop_expired();
            let until_deadline = deadline.saturating_duration_since(Instant::now());
            std::thread::sleep(self.next_wake(until_deadline).min(until_deadline));
        }
    }

    /// Hold until every assertion has expired (or the process is killed).
    pub fn hold_until_released(mut self, backstop: Duration) {
        while !self.held.is_empty() {
            self.drop_expired();
            if self.held.is_empty() {
                break;
            }
            std::thread::sleep(self.next_wake(backstop));
        }
    }

    /// Run `command` with `args` while holding the assertions, then return its
    /// exit code (126 if it could not be spawned, mirroring shell convention).
    pub fn run_command(self, command: &str, args: &[String]) -> i32 {
        let status = Command::new(command).args(args).status();
        match status {
            Ok(s) => s.code().unwrap_or(1),
            Err(e) => {
                eprintln!("wakeup: failed to run {command}: {e}");
                126
            }
        }
    }
}

#[cfg(unix)]
fn pid_alive(pid: i32) -> bool {
    use std::os::raw::c_int;

    #[cfg(target_os = "macos")]
    extern "C" {
        #[link_name = "__error"]
        fn libc_errno_location() -> *mut c_int;
    }

    #[cfg(not(target_os = "macos"))]
    extern "C" {
        #[link_name = "__errno_location"]
        fn libc_errno_location() -> *mut c_int;
    }

    extern "C" {
        #[link_name = "kill"]
        fn libc_kill(pid: i32, sig: c_int) -> c_int;
    }

    const EPERM: c_int = 1;

    // kill(pid, 0): 0 => alive, EPERM => alive (not ours), ESRCH => gone.
    let rc = unsafe { libc_kill(pid, 0) };
    if rc == 0 {
        return true;
    }
    let errno = unsafe { *libc_errno_location() };
    errno == EPERM
}

#[cfg(not(unix))]
fn pid_alive(pid: i32) -> bool {
    windows_pid_alive(pid)
}

#[cfg(target_os = "windows")]
fn windows_pid_alive(pid: i32) -> bool {
    backend::windows_pid_alive(pid)
}

#[cfg(all(not(unix), not(target_os = "windows")))]
fn windows_pid_alive(_pid: i32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_is_stable_and_human_readable() {
        assert_eq!(AssertionKind::IdleSystem.label(), "idle-system");
        assert_eq!(AssertionKind::UserActive.label(), "user-active");
    }
}
