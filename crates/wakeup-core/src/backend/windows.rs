//! Windows backend: kernel32's Power Request API.
//!
//! Uses `PowerCreateRequest` / `PowerSetRequest` / `PowerClearRequest`
//! directly via FFI (no external crate), matching this workspace's
//! "link the OS API directly" convention used by the macOS IOKit backend.
//!
//! Only `PowerRequestSystemRequired` and `PowerRequestDisplayRequired` are
//! used; there is no clean Windows equivalent for disk-idle or "declare the
//! user active" (see the release plan), so those are reported as
//! unsupported for now.
//!
//! `REASON_CONTEXT` is a tagged union in the Windows SDK; we only ever
//! populate it in `POWER_REQUEST_CONTEXT_SIMPLE_STRING` mode; the unused
//! "Detailed" variant is safely omitted below because we both write and the
//! OS only reads the fields implied by `flags`.

use super::{label, BackendError};
use crate::AssertionKind;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;

type Handle = *mut c_void;
type Wchar = u16;

const POWER_REQUEST_CONTEXT_VERSION: u32 = 0;
const POWER_REQUEST_CONTEXT_SIMPLE_STRING: u32 = 0x1;

/// `REASON_CONTEXT`, restricted to the simple-string variant we use.
#[repr(C)]
struct ReasonContext {
    version: u32,
    flags: u32,
    simple_reason_string: *mut Wchar,
}

/// `POWER_REQUEST_TYPE` (a C enum, passed by value as a 4-byte int on Windows).
#[repr(C)]
#[derive(Clone, Copy)]
enum PowerRequestType {
    DisplayRequired = 0,
    SystemRequired = 1,
}

#[link(name = "kernel32")]
extern "system" {
    fn PowerCreateRequest(context: *mut ReasonContext) -> Handle;
    fn PowerSetRequest(power_request: Handle, request_type: PowerRequestType) -> i32;
    fn PowerClearRequest(power_request: Handle, request_type: PowerRequestType) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
    fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> Handle;
    fn GetExitCodeProcess(process: Handle, exit_code: *mut u32) -> i32;
}

const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const STILL_ACTIVE: u32 = 259;

fn to_wide(s: &str) -> Vec<Wchar> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn request_type_for(kind: AssertionKind) -> Result<PowerRequestType, BackendError> {
    match kind {
        AssertionKind::Display => Ok(PowerRequestType::DisplayRequired),
        AssertionKind::IdleSystem | AssertionKind::System => Ok(PowerRequestType::SystemRequired),
        AssertionKind::Disk | AssertionKind::UserActive => Err(BackendError(format!(
            "{} assertions are not supported on Windows (no matching Power Request type)",
            label(kind)
        ))),
    }
}

pub(crate) struct BackendAssertion {
    handle: Handle,
    request_type: PowerRequestType,
}

pub(crate) fn create(kind: AssertionKind, reason: &str) -> Result<BackendAssertion, BackendError> {
    let request_type = request_type_for(kind)?;

    // `wide_reason` must outlive the `PowerCreateRequest` call below.
    let mut wide_reason = to_wide(reason);
    let mut context = ReasonContext {
        version: POWER_REQUEST_CONTEXT_VERSION,
        flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
        simple_reason_string: wide_reason.as_mut_ptr(),
    };

    let handle = unsafe { PowerCreateRequest(&mut context) };
    if handle.is_null() {
        return Err(BackendError(format!(
            "could not create {} assertion: PowerCreateRequest failed",
            label(kind)
        )));
    }

    let ok = unsafe { PowerSetRequest(handle, request_type) };
    if ok == 0 {
        unsafe {
            CloseHandle(handle);
        }
        return Err(BackendError(format!(
            "could not create {} assertion: PowerSetRequest failed",
            label(kind)
        )));
    }

    Ok(BackendAssertion {
        handle,
        request_type,
    })
}

impl Drop for BackendAssertion {
    fn drop(&mut self) {
        unsafe {
            PowerClearRequest(self.handle, self.request_type);
            CloseHandle(self.handle);
        }
    }
}

/// Used by `wakeup-core`'s `-w <pid>` support on Windows, since Unix `kill(pid, 0)`
/// has no meaning here.
pub(crate) fn pid_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32) };
    if handle.is_null() {
        // Could not open the process: either it does not exist, or we lack
        // access. Treat "access denied" as alive, matching the Unix EPERM case.
        return false;
    }
    let mut exit_code: u32 = 0;
    let ok = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
    unsafe {
        CloseHandle(handle);
    }
    ok != 0 && exit_code == STILL_ACTIVE
}
