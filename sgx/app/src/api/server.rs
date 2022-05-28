use std::thread;
use std::ffi::CString;

use log::warn;
use sgx_types::*;

use crate::enclave::ecall::api::ecall_api_server_start;
use crate::ENCLAVE_DOORBELL;

const THREAD_NUM: u8 = 1;

pub(crate) fn start_api_service(addr: String) {
    let mut children = vec![];
    let thread_count = std::cmp::min(std::cmp::min(THREAD_NUM,
                                                   ENCLAVE_DOORBELL.capacity()),
                                     num_cpus::get() as u8);

    for _ in 0..thread_count {
        let addr = addr.clone();

        children.push(thread::spawn(move || {
            let enclave_access_token = ENCLAVE_DOORBELL
                .get_access(false) // This can never be recursive
                .unwrap();
            let enclave = enclave_access_token.unwrap();

            let c_addr: CString = CString::new(addr).unwrap();
            let result = unsafe {
                ecall_api_server_start(enclave.geteid(),
                                       c_addr.as_bytes_with_nul().as_ptr() as *const c_char)
            };

            match result {
                sgx_status_t::SGX_SUCCESS => {}
                _ => {
                    warn!("ECALL [ecall_api_service_start] failed {}!", result);
                    return;
                }
            }
        }));
    }

    for child in children {
        // Wait for the thread to finish. Returns a result.
        let _ = child.join();
    }
}