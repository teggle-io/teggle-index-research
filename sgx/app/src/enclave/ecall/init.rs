use sgx_types::*;

extern {
    pub(crate) fn ecall_init(eid: sgx_enclave_id_t) -> sgx_status_t;
}