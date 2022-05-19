use sgx_types::*;
use alloc::string::ToString;
use std::string::String;
use std::vec::Vec;

use crate::enclave_ffi_types::{EnclaveBuffer, OcallReturn};
use crate::external::ecall::allocate::recover_buffer;
use crate::external::ocall::db::{ocall_db_flush, ocall_db_get, ocall_db_get_fixed, ocall_db_put};

#[allow(dead_code)]
fn db_put(key: &[u8], value: &[u8]) -> Result<(), String> {
    let mut ocall_return = OcallReturn::Success;

    let result = unsafe {
        ocall_db_put(
            (&mut ocall_return) as *mut _,
            key.as_ptr(),
            key.len(),
            value.as_ptr(),
            value.len())
    };

    if result != sgx_status_t::SGX_SUCCESS {
        return Err(result.to_string());
    }

    return match ocall_return {
        OcallReturn::Success => Ok(()),
        _ => {
            return Err(format!("ocall_db_put returned {:?}", ocall_return));
        }
    };
}

#[allow(dead_code)]
fn db_get(key: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let mut ocall_return = OcallReturn::Success;

    let mut enclave_buffer = std::mem::MaybeUninit::<EnclaveBuffer>::uninit();

    let result = unsafe {
        ocall_db_get(
            (&mut ocall_return) as *mut _,
            enclave_buffer.as_mut_ptr(),
            key.as_ptr(),
            key.len(),
        )
    };

    if result != sgx_status_t::SGX_SUCCESS {
        return Err(result.to_string());
    }
    return match ocall_return {
        OcallReturn::Success => {
            let value = unsafe {
                let enclave_buffer = enclave_buffer.assume_init();
                // TODO: not sure why map_err isn't working.
                match recover_buffer(enclave_buffer) {
                    Ok(v) => Ok(v),
                    Err(_err) => Err("Failed to recover enclave buffer")
                }
            }?;

            Ok(value)
        }
        OcallReturn::None => Ok(None),
        _ => {
            return Err(format!("ocall_db_get returned {:?}", ocall_return));
        }
    };
}

#[allow(dead_code)]
fn db_get_fixed(key: &[u8], max_bytes: usize) -> Result<Option<Vec<u8>>, String> {
    let mut ocall_return = OcallReturn::Success;
    let mut value = vec![0; max_bytes];
    let mut value_len = 0 as usize;

    let result = unsafe {
        ocall_db_get_fixed(
            (&mut ocall_return) as *mut _,
            key.as_ptr(),
            key.len(),
            value.as_mut_ptr(),
            max_bytes,
            (&mut value_len) as *mut _,
        )
    };

    if result != sgx_status_t::SGX_SUCCESS {
        return Err(result.to_string());
    }
    return match ocall_return {
        OcallReturn::Success => {
            value.truncate(value_len);

            Ok(Some(value))
        }
        OcallReturn::None => Ok(None),
        _ => {
            return Err(format!("ocall_db_get_fixed returned {:?}", ocall_return));
        }
    };
}

#[allow(dead_code)]
fn db_flush() -> Result<(), String> {
    let mut ocall_return = OcallReturn::Success;

    let result = unsafe {
        ocall_db_flush(
            (&mut ocall_return) as *mut _,
        )
    };

    if result != sgx_status_t::SGX_SUCCESS {
        return Err(result.to_string());
    }
    return match ocall_return {
        OcallReturn::Success => Ok(()),
        _ => {
            return Err(format!("ocall_db_flush returned {:?}", ocall_return));
        }
    };
}