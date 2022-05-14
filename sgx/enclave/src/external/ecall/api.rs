use sgx_types::*;
use api::session::{HandleResult, SessionManager};

#[no_mangle]
pub extern "C" fn ecall_api_server_new(fd: c_int) -> usize {
    match SessionManager::create_session(fd) {
        Some(s) => s,
        None => 0xFFFF_FFFF_FFFF_FFFF,
    }
}

#[no_mangle]
pub extern "C" fn ecall_api_server_handle(session_id: size_t) -> c_int {
    if let Some(session_ptr) = SessionManager::get_session(session_id) {
        let session = unsafe { &mut *(session_ptr) };
        match session.handle() {
            HandleResult::EOF => { -1 }
            HandleResult::Error => { -1 }
            HandleResult::Continue => { 0 }
            HandleResult::Close => { 1 }
        }
    } else { -1 }
}

#[no_mangle]
pub extern "C" fn ecall_api_server_wants_read(session_id: usize) -> c_int {
    if let Some(session_ptr) = SessionManager::get_session(session_id) {
        let session = unsafe { &mut *(session_ptr) };
        let result = session.wants_read() as c_int;
        result
    } else { -1 }
}

#[no_mangle]
pub extern "C" fn ecall_api_server_wants_write(session_id: usize)  -> c_int {
    if let Some(session_ptr) = SessionManager::get_session(session_id) {
        let session = unsafe { &mut *(session_ptr) };
        let result = session.wants_write() as c_int;
        result
    } else { -1 }
}

#[no_mangle]
pub extern "C" fn ecall_api_server_close(session_id: usize) {
    SessionManager::remove_session(session_id)
}