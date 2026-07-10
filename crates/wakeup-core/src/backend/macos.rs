//! macOS backend: direct IOKit power assertions via `IOPMAssertionCreateWithName`.
//!
//! Ported from the original single-crate `wakeup` implementation with zero
//! behavior change; only the assertion-type-name lookup is now driven by the
//! platform-neutral `AssertionKind` instead of a CLI-local table.

use super::{label, BackendError};
use crate::AssertionKind;
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

fn cfstr(s: &str) -> Result<CFStringRef, BackendError> {
    let c = CString::new(s).map_err(|_| BackendError("string contained a NUL byte".into()))?;
    let r = unsafe {
        CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), KCF_STRING_ENCODING_UTF8)
    };
    if r.is_null() {
        return Err(BackendError("failed to allocate CFString".into()));
    }
    Ok(r)
}

fn assertion_type_name(kind: AssertionKind) -> &'static str {
    match kind {
        AssertionKind::Display => "PreventUserIdleDisplaySleep",
        AssertionKind::IdleSystem => "PreventUserIdleSystemSleep",
        AssertionKind::Disk => "PreventDiskIdle",
        AssertionKind::System => "PreventSystemSleep",
        AssertionKind::UserActive => "UserIsActive",
    }
}

/// An RAII IOKit power assertion: released automatically on drop (and by the
/// kernel if the process dies, which is how Ctrl-C is handled).
pub(crate) struct BackendAssertion {
    id: IOPMAssertionID,
}

pub(crate) fn create(kind: AssertionKind, reason: &str) -> Result<BackendAssertion, BackendError> {
    let atype = cfstr(assertion_type_name(kind))?;
    let aname = cfstr(reason)?;
    let mut id: IOPMAssertionID = 0;
    let rc =
        unsafe { IOPMAssertionCreateWithName(atype, KIOPM_ASSERTION_LEVEL_ON, aname, &mut id) };
    unsafe {
        CFRelease(atype);
        CFRelease(aname);
    }
    if rc == 0 {
        Ok(BackendAssertion { id })
    } else {
        Err(BackendError(format!(
            "could not create {} assertion: IOPMAssertionCreateWithName returned {rc}",
            label(kind)
        )))
    }
}

impl Drop for BackendAssertion {
    fn drop(&mut self) {
        unsafe { IOPMAssertionRelease(self.id) };
    }
}
