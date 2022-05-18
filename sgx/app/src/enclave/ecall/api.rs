use sgx_types::*;

extern {
    pub(crate) fn ecall_api_server_start(eid: sgx_enclave_id_t,
                                         addr: *const c_char,
                                         thread_count: uint8_t) -> sgx_status_t;
}