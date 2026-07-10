//! Linux backend: systemd-logind inhibitor locks.
//!
//! We deliberately shell out to `systemd-inhibit` rather than speaking D-Bus
//! directly, to keep this crate dependency-free (no `zbus`/`dbus` crate) the
//! same way the macOS backend links IOKit directly instead of pulling in a
//! wrapper crate.
//!
//! `systemd-inhibit --what=<what> --mode=block ... <command>` takes the
//! inhibitor lock for as long as `<command>` runs; we wrap `sleep infinity`
//! and kill that child on drop, which releases the lock immediately (systemd
//! releases the lock as soon as the wrapped process exits, by design).
//!
//! `systemd-logind`'s inhibitor `what` categories only cover `sleep` (system
//! suspend/hibernate) and `idle` (the idle action, e.g. auto-suspend after
//! inactivity). There is no logind equivalent for display-only sleep, disk
//! idle, or "declare the user active"; those are desktop-environment-specific
//! and are reported as unsupported for now (see the release plan for the
//! tracked follow-up).

use super::{label, BackendError};
use crate::AssertionKind;
use std::process::{Child, Command, Stdio};

fn what_for(kind: AssertionKind) -> Result<&'static str, BackendError> {
    match kind {
        AssertionKind::IdleSystem => Ok("idle"),
        AssertionKind::System => Ok("sleep"),
        AssertionKind::Display | AssertionKind::Disk | AssertionKind::UserActive => {
            Err(BackendError(format!(
                "{} assertions are not supported on Linux yet (systemd-logind has no matching \
                 inhibitor category; this is desktop-environment-specific)",
                label(kind)
            )))
        }
    }
}

pub(crate) struct BackendAssertion {
    child: Child,
}

pub(crate) fn create(kind: AssertionKind, reason: &str) -> Result<BackendAssertion, BackendError> {
    let what = what_for(kind)?;
    let child = Command::new("systemd-inhibit")
        .arg(format!("--what={what}"))
        .arg("--mode=block")
        .arg("--who=wakeup")
        .arg(format!("--why={reason}"))
        .arg("sleep")
        .arg("infinity")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            BackendError(format!(
                "could not create {} assertion: failed to spawn systemd-inhibit \
                 (is systemd-logind installed and running?): {e}",
                label(kind)
            ))
        })?;
    Ok(BackendAssertion { child })
}

impl Drop for BackendAssertion {
    fn drop(&mut self) {
        // Killing the wrapped `sleep infinity` makes systemd-inhibit exit and
        // release the inhibitor lock right away.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
