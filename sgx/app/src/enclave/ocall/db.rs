use enclave_ffi_types::{EnclaveBuffer, OcallReturn};
use std::ptr;
use db::GLOBAL_DB;
use enclave::allocate::allocate_enclave_buffer;
use traits::Db;

#[no_mangle]
pub extern "C"
fn ocall_db_get(
    value: *mut EnclaveBuffer,
    key: *const u8,
    key_len: usize,
) -> OcallReturn {
    let mut ret = OcallReturn::Success;

    let key = unsafe { std::slice::from_raw_parts(key, key_len) };

    // TODO: Remove expect
    if let Some(res) = GLOBAL_DB.get(key).expect("failed to get") {
        let enclave_buffer = allocate_enclave_buffer(res.as_slice())
            .expect("failed to allocate buffer"); // TODO: REMOVE EXPECT

        unsafe { *value = enclave_buffer };
    } else {
        ret = OcallReturn::None
    }

    ret
}

#[no_mangle]
pub extern "C"
fn ocall_db_get_fixed(
    key: *const u8,
    key_len: usize,
    value: *mut u8,
    value_max_len: usize,
    value_len: *mut usize
) -> OcallReturn {
    let mut ret = OcallReturn::Success;

    let key = unsafe { std::slice::from_raw_parts(key, key_len) };

    // TODO: Remove expect
    if let Some(res) = GLOBAL_DB.get(key).expect("failed to get") {
        if res.len() > value_max_len {
            ret = OcallReturn::TooBig
        } else {
            unsafe {
                ptr::copy_nonoverlapping(res.as_ptr(), value, res.len());

                *value_len = res.len();
            }
        }
    } else {
        ret = OcallReturn::None
    }

    ret
}

#[no_mangle]
pub extern "C"
fn ocall_db_delete(
    key: *const u8,
    key_len: usize,
) -> OcallReturn {
    let key = unsafe { std::slice::from_raw_parts(key, key_len) };

    // TODO: Remove expect
    GLOBAL_DB.delete(key).expect("failed to delete");

    OcallReturn::Success
}

#[no_mangle]
pub extern "C"
fn ocall_db_put(
    key: *const u8,
    key_len: usize,
    value: *const u8,
    value_len: usize,
) -> OcallReturn {
    let key = unsafe { std::slice::from_raw_parts(key, key_len) };
    let value = unsafe { std::slice::from_raw_parts(value, value_len) };

    // TODO: Remove expect
    GLOBAL_DB.put(key, value).expect("failed to put");

    OcallReturn::Success
}

#[no_mangle]
pub extern "C"
fn ocall_db_flush() -> OcallReturn
{
    // TODO: Remove expec
    GLOBAL_DB.flush().expect("failed to flush");

    OcallReturn::Success
}
