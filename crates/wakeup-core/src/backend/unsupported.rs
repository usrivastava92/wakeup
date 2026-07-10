//! Fallback backend for any platform other than macOS, Linux, or Windows.
//!
//! Always returns a clear error instead of silently doing nothing, so a
//! consumer cannot mistake "no assertion was created" for "the assertion is
//! held".

use super::{label, BackendError};
use crate::AssertionKind;

pub(crate) struct BackendAssertion;

pub(crate) fn create(kind: AssertionKind, _reason: &str) -> Result<BackendAssertion, BackendError> {
    Err(BackendError(format!(
        "{} assertions are not implemented on this platform (only macOS, Linux, and Windows are supported)",
        label(kind)
    )))
}
