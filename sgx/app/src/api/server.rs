use std::ffi::CString;

use log::warn;
use sgx_types::*;

use enclave::ecall::api::ecall_api_server_start;
use ENCLAVE_DOORBELL;

const THREAD_NUM: u8 = 8;

pub(crate) fn start_api_service(addr: String) {
    // One extra for the outer thread (which is zero cost of course).
    let thread_count = std::cmp::min(std::cmp::min(THREAD_NUM,
                                                   ENCLAVE_DOORBELL.capacity() - 1),
                                     num_cpus::get() as u8);

    let enclave_access_token = ENCLAVE_DOORBELL
        .get_access_for(false, thread_count + 1)
        .unwrap();
    let enclave = enclave_access_token.unwrap();

    let c_addr: CString = CString::new(addr).unwrap();
    let result = unsafe {
        ecall_api_server_start(enclave.geteid(),
                               c_addr.as_bytes_with_nul().as_ptr() as *const c_char,
                               thread_count as uint8_t)
    };

    match result {
        sgx_status_t::SGX_SUCCESS => {}
        _ => {
            warn!("ECALL [ecall_api_service_start] failed {}!", result);
            return;
        }
    }
}
