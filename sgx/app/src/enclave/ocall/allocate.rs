use sgx_types::*;
use enclave_ffi_types::{EnclaveBuffer};

extern {
    pub fn ecall_allocate(
        eid: sgx_enclave_id_t,
        retval: *mut EnclaveBuffer,
        buffer: *const u8,
        length: usize,
    ) -> sgx_status_t;
}