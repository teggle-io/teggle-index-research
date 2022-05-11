use enclave_ffi_types::{EnclaveBuffer, OcallReturn, UserSpaceBuffer};
use sgx_types::*;

extern "C" {
    pub fn ocall_allocate(
        retval: *mut UserSpaceBuffer,
        buffer: *const u8,
        length: usize,
    ) -> sgx_status_t;

    pub fn ocall_db_get(
        retval: *mut OcallReturn,
        value: *mut EnclaveBuffer,
        key: *const u8,
        key_len: usize,
    ) -> sgx_status_t;

    pub fn ocall_db_get_fixed(
        retval: *mut OcallReturn,
        key: *const u8,
        key_len: usize,
        value: *mut u8,
        value_max_len: usize,
        value_len: *mut usize
    ) -> sgx_status_t;

    pub fn ocall_db_delete(
        retval: *mut OcallReturn,
        key: *const u8,
        key_len: usize,
    ) -> sgx_status_t;

    pub fn ocall_db_put(
        retval: *mut OcallReturn,
        key: *const u8,
        key_len: usize,
        value: *const u8,
        value_len: usize,
    ) -> sgx_status_t;

    pub fn ocall_db_flush(
        retval: *mut OcallReturn,
    ) -> sgx_status_t;
}
