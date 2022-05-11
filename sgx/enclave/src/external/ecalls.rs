use alloc::boxed::Box;
use std::ffi::c_void;
use std::panic;
use std::sync::SgxMutex;
use std::vec::Vec;

use lazy_static::lazy_static;
use log::*;

use enclave_ffi_types::{
    EnclaveBuffer, HealthCheckResult
};
use validate_const_ptr;

use crate::utils::{oom_handler};

lazy_static! {
    static ref ECALL_ALLOCATE_STACK: SgxMutex<Vec<EnclaveBuffer>> = SgxMutex::new(Vec::new());
}

/// # Safety
/// Always use protection
#[no_mangle]
pub unsafe extern "C" fn ecall_allocate(buffer: *const u8, length: usize) -> EnclaveBuffer {
    ecall_allocate_impl(buffer, length)
}

/// Allocate a buffer in the enclave and return a pointer to it. This is useful for ocalls that
/// want to return a response of unknown length to the enclave. Instead of pre-allocating it on the
/// ecall side, the ocall can call this ecall and return the EnclaveBuffer to the ecall that called
/// it.
///
/// host -> ecall_x -> ocall_x -> ecall_allocate
/// # Safety
/// Always use protection
unsafe fn ecall_allocate_impl(buffer: *const u8, length: usize) -> EnclaveBuffer {
    if let Err(_err) = oom_handler::register_oom_handler() {
        error!("Could not register OOM handler!");
        return EnclaveBuffer::default();
    }

    validate_const_ptr!(buffer, length as usize, EnclaveBuffer::default());

    let slice = std::slice::from_raw_parts(buffer, length);
    let result = panic::catch_unwind(|| {
        let vector_copy = slice.to_vec();
        let boxed_vector = Box::new(vector_copy);
        let heap_pointer = Box::into_raw(boxed_vector);
        let enclave_buffer = EnclaveBuffer {
            ptr: heap_pointer as *mut c_void,
        };
        ECALL_ALLOCATE_STACK
            .lock()
            .unwrap()
            .push(enclave_buffer.unsafe_clone());
        enclave_buffer
    });

    if let Err(_err) = oom_handler::restore_safety_buffer() {
        error!("Could not restore OOM safety buffer!");
        return EnclaveBuffer::default();
    }

    result.unwrap_or_else(|err| {
        // We can get here only by failing to allocate memory,
        // so there's no real need here to test if oom happened
        error!("Enclave ran out of memory: {:?}", err);
        oom_handler::get_then_clear_oom_happened();
        EnclaveBuffer::default()
    })
}

#[derive(Debug, PartialEq)]
pub struct BufferRecoveryError;

/// Take a pointer as returned by `ecall_allocate` and recover the Vec<u8> inside of it.
/// # Safety
///  This is a text
pub unsafe fn recover_buffer(ptr: EnclaveBuffer) -> Result<Option<Vec<u8>>, BufferRecoveryError> {
    if ptr.ptr.is_null() {
        return Ok(None);
    }

    let mut alloc_stack = ECALL_ALLOCATE_STACK.lock().unwrap();

    // search the stack from the end for this pointer
    let maybe_index = alloc_stack
        .iter()
        .rev()
        .position(|buffer| buffer.ptr as usize == ptr.ptr as usize);
    if let Some(index_from_the_end) = maybe_index {
        // This index is probably at the end of the stack, but we give it a little more flexibility
        // in case access patterns change in the future
        let index = alloc_stack.len() - index_from_the_end - 1;
        alloc_stack.swap_remove(index);
    } else {
        return Err(BufferRecoveryError);
    }
    let boxed_vector = Box::from_raw(ptr.ptr as *mut Vec<u8>);
    Ok(Some(*boxed_vector))
}

/// # Safety
/// Always use protection
#[no_mangle]
pub unsafe extern "C" fn ecall_health_check() -> HealthCheckResult {
    HealthCheckResult::Success
}