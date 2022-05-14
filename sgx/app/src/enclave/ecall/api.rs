use sgx_types::*;

extern {
    pub(crate) fn ecall_api_server_new(eid: sgx_enclave_id_t, retval: *mut size_t,
                                       fd: c_int) -> sgx_status_t;
    pub(crate) fn ecall_api_server_handle(eid: sgx_enclave_id_t, retval: *mut c_int,
                                          session_id: size_t) -> sgx_status_t;
    pub(crate) fn ecall_api_server_wants_read(eid: sgx_enclave_id_t, retval: *mut c_int,
                                              session_id: size_t) -> sgx_status_t;
    pub(crate) fn ecall_api_server_wants_write(eid: sgx_enclave_id_t, retval: *mut c_int,
                                               session_id: size_t) -> sgx_status_t;
    pub(crate) fn ecall_api_server_close(eid: sgx_enclave_id_t,
                                         session_id: size_t) -> sgx_status_t;
}