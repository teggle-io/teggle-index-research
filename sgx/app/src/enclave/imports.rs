use sgx_types::*;
use enclave_ffi_types::{EnclaveBuffer, OcallReturn};

extern {
    pub fn ecall_perform_test(
        eid: sgx_enclave_id_t,
        retval: *mut sgx_status_t
    ) -> sgx_status_t;
}