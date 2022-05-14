use log::trace;
use sgx_types::{sgx_status_t, SgxResult};

use enclave::ocall::allocate::ecall_allocate;
use ENCLAVE_DOORBELL;
use enclave_ffi_types::EnclaveBuffer;

/// This is a safe wrapper for allocating buffers inside the enclave.
pub(crate) fn allocate_enclave_buffer(buffer: &[u8]) -> SgxResult<EnclaveBuffer> {
    let ptr = buffer.as_ptr();
    let len = buffer.len();
    let mut enclave_buffer = EnclaveBuffer::default();

    // Bind the token to a local variable to ensure its
    // destructor runs in the end of the function
    let enclave_access_token = ENCLAVE_DOORBELL
        // This is always called from an ocall contxt
        .get_access(true)
        .ok_or(sgx_status_t::SGX_ERROR_BUSY)?;

    let enclave_id = enclave_access_token
        .expect("If we got here, surely the enclave has been loaded")
        .geteid();

    trace!(
        target: module_path!(),
        "allocate_enclave_buffer() called with len: {:?} enclave_id: {:?}",
        len,
        enclave_id,
    );

    match unsafe { ecall_allocate(enclave_id, &mut enclave_buffer, ptr, len) } {
        sgx_status_t::SGX_SUCCESS => Ok(enclave_buffer),
        failure_status => Err(failure_status),
    }
}
