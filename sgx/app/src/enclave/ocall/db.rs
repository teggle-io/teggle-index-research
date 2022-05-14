use std::ptr;

use log::warn;

use db::GLOBAL_DB;
use enclave::allocate::allocate_enclave_buffer;
use enclave_ffi_types::{EnclaveBuffer, OcallReturn};
use traits::Db;

#[no_mangle]
pub extern "C"
fn ocall_db_get(
    value: *mut EnclaveBuffer,
    key: *const u8,
    key_len: usize,
) -> OcallReturn {
    let mut ret = OcallReturn::None;

    let key = unsafe { std::slice::from_raw_parts(key, key_len) };

    match GLOBAL_DB.get(key) {
        Ok(res) => {
            if res.is_some() {
                match allocate_enclave_buffer(res.unwrap().as_slice()) {
                    Ok(enclave_buffer) => {
                        unsafe { *value = enclave_buffer };
                    }
                    Err(e) => {
                        warn!("ocall_db_get failed to allocate enclave buffer {:?}", e);
                        ret = OcallReturn::Failure
                    }
                }
            }
        }
        Err(e) => {
            warn!("ocall_db_get failed {:?}", e);
            ret = OcallReturn::Failure
        }
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

    match GLOBAL_DB.get(key) {
        Ok(res) => {
            if res.is_some() {
                let res = res.unwrap();
                if res.len() > value_max_len {
                    warn!("ocall_db_get_fixed fetch too big ({} vs {})", res.len(), value_max_len);
                    ret = OcallReturn::TooBig
                } else {
                    unsafe {
                        ptr::copy_nonoverlapping(res.as_ptr(), value, res.len());

                        *value_len = res.len();
                    }
                }
            }
        }
        Err(e) => {
            warn!("ocall_db_get_fixed failed {:?}", e);
            ret = OcallReturn::Failure
        }
    }

    ret
}

#[no_mangle]
pub extern "C"
fn ocall_db_delete(
    key: *const u8,
    key_len: usize,
) -> OcallReturn {
    let mut ret = OcallReturn::Success;

    let key = unsafe { std::slice::from_raw_parts(key, key_len) };

    match GLOBAL_DB.delete(key) {
        Err(e) => {
            warn!("ocall_db_delete failed {:?}", e);
            ret = OcallReturn::Failure
        }
        _ => {}
    }

    ret
}

#[no_mangle]
pub extern "C"
fn ocall_db_put(
    key: *const u8,
    key_len: usize,
    value: *const u8,
    value_len: usize,
) -> OcallReturn {
    let mut ret = OcallReturn::Success;

    let key = unsafe { std::slice::from_raw_parts(key, key_len) };
    let value = unsafe { std::slice::from_raw_parts(value, value_len) };

    match GLOBAL_DB.put(key, value) {
        Err(e) => {
            warn!("ocall_db_put failed {:?}", e);
            ret = OcallReturn::Failure
        }
        _ => {}
    }

    ret
}

#[no_mangle]
pub extern "C"
fn ocall_db_flush() -> OcallReturn
{
    let mut ret = OcallReturn::Success;

    match GLOBAL_DB.flush() {
        Err(e) => {
            warn!("ocall_db_flush failed {:?}", e);
            ret = OcallReturn::Failure
        }
        _ => {}
    }

    ret
}
