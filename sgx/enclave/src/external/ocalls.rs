use enclave_ffi_types::{Ctx, EnclaveBuffer, OcallReturn, UserSpaceBuffer};
use sgx_types::*;

extern "C" {
    pub fn ocall_allocate(
        retval: *mut UserSpaceBuffer,
        buffer: *const u8,
        length: usize,
    ) -> sgx_status_t;

    pub fn ocall_db_get(
        retval: *mut OcallReturn,
        context: Ctx,
        value: *mut EnclaveBuffer,
        key: *const u8,
        key_len: usize,
    ) -> sgx_status_t;

    pub fn ocall_db_delete(
        retval: *mut OcallReturn,
        context: Ctx,
        key: *const u8,
        key_len: usize,
    ) -> sgx_status_t;

    pub fn ocall_db_put(
        retval: *mut OcallReturn,
        context: Ctx,
        key: *const u8,
        key_len: usize,
        value: *const u8,
        value_len: usize,
    ) -> sgx_status_t;

    pub fn ocall_db_flush(
        retval: *mut OcallReturn,
        context: Ctx,
    ) -> sgx_status_t;
}
