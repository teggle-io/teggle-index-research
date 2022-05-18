use alloc::string::ToString;
use sgx_trts::c_str::CStr;
use sgx_types::*;
use api::server::server::start_api_server;

#[no_mangle]
pub extern "C" fn ecall_api_server_start(addr: * const c_char, thread_count: uint8_t) {
    let addr = unsafe { CStr::from_ptr(addr).to_str() }.unwrap();

    start_api_server(addr.to_string(), thread_count as u8)
}
