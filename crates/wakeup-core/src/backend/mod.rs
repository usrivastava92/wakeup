//! Per-OS power assertion backends, selected at compile time.
//!
//! Each backend exposes the same two items to the rest of the crate:
//!
//!   - `BackendAssertion`: an opaque, `Drop`-releases-it handle type.
//!   - `create(kind, reason) -> Result<BackendAssertion, BackendError>`
//!
//! Add a new platform by adding a module here, gating it on `target_os`, and
//! re-exporting `create`/`BackendAssertion` from it below.

use crate::AssertionKind;

/// An error from a backend, such as an OS call failing or a given
/// `AssertionKind` not being supported on the current platform yet.
#[derive(Debug)]
pub struct BackendError(pub(crate) String);

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BackendError {}

/// A short, human-readable, platform-neutral label for error messages.
pub(crate) fn label(kind: AssertionKind) -> &'static str {
    match kind {
        AssertionKind::Display => "display",
        AssertionKind::IdleSystem => "idle-system",
        AssertionKind::Disk => "disk",
        AssertionKind::System => "system",
        AssertionKind::UserActive => "user-active",
    }
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::{create, BackendAssertion};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::{create, BackendAssertion};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub(crate) use windows::{create, pid_alive as windows_pid_alive, BackendAssertion};

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod unsupported;
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub(crate) use unsupported::{create, BackendAssertion};
