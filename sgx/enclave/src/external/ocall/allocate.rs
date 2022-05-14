use enclave_ffi_types::{UserSpaceBuffer};
use sgx_types::*;

extern "C" {
    pub fn ocall_allocate(
        retval: *mut UserSpaceBuffer,
        buffer: *const u8,
        length: usize,
    ) -> sgx_status_t;
}
