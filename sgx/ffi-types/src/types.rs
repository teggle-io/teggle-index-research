#![allow(unused)]

use core::ffi::c_void;
use derive_more::Display;

/// This type represents an opaque pointer to a memory address in normal user space.
#[repr(C)]
pub struct UserSpaceBuffer {
    pub ptr: *mut c_void,
}

/// This type represents an opaque pointer to a memory address inside the enclave.
#[repr(C)]
pub struct EnclaveBuffer {
    pub ptr: *mut c_void,
}

impl EnclaveBuffer {
    /// # Safety
    /// Very unsafe. Much careful
    pub unsafe fn unsafe_clone(&self) -> Self {
        EnclaveBuffer { ptr: self.ptr }
    }
}

/// This is safe because `Vec<u8>`s are `Send`
unsafe impl Send for EnclaveBuffer {}

impl Default for EnclaveBuffer {
    fn default() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
        }
    }
}

/// This struct holds a pointer to memory in userspace, that contains the storage
#[repr(C)]
pub struct Ctx {
    pub data: *mut c_void,
}

impl Ctx {
    /// # Safety
    /// Very unsafe. Much careful
    pub unsafe fn unsafe_clone(&self) -> Self {
        Self { data: self.data }
    }
}

/// This type represents the possible error conditions that can be encountered in the enclave
/// cbindgen:prefix-with-name
#[repr(C)]
#[derive(Debug, Display)]
pub enum EnclaveError {
    // TODO: Refine for use.
    #[display(fmt = "failed to validate transaction")]
    ValidationFailure,

    // serious issues
    /// The host was caught trying to disrupt the enclave.
    /// This can happen if e.g. the host provides invalid pointers as responses from ocalls.
    #[display(fmt = "communication with the enclave's host failed")]
    HostMisbehavior,
    #[display(fmt = "panicked due to unexpected behavior")]
    Panic,
    #[display(fmt = "enclave ran out of heap memory")]
    OutOfMemory,
    #[display(fmt = "depth of nested contract calls exceeded")]
    ExceededRecursionLimit,
    /// Unexpected Error happened, no more details available
    #[display(fmt = "unknown error")]
    Unknown,
}

/// This type represents the possible error conditions that can be encountered in the
/// enclave while authenticating a new node in the network.
/// cbindgen:prefix-with-name
#[repr(C)]
#[derive(Debug, Display, PartialEq, Eq)]
pub enum HealthCheckResult {
    Success,
}

impl Default for HealthCheckResult {
    fn default() -> Self {
        HealthCheckResult::Success
    }
}

/// This type represent return statuses from ocalls.
///
/// cbindgen:prefix-with-name
#[repr(C)]
#[derive(Debug, Display)]
pub enum OcallReturn {
    /// Ocall returned successfully.
    Success,
    /// Ocall failed for some reason.
    /// error parameters may be passed as out parameters.
    Failure,
    /// A panic happened during the ocall.
    Panic,
}
