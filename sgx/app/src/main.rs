// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License..

extern crate enclave_ffi_types;
extern crate lazy_static;
extern crate log;
extern crate parking_lot;
extern crate rocksdb;
extern crate sgx_types;
extern crate sgx_urts;

use std::time::SystemTime;

use log::trace;
use sgx_types::*;
use db::DB;

use enclave::ENCLAVE_DOORBELL;
use enclave_ffi_types::{EnclaveBuffer, OcallReturn};
use traits::Db;

pub mod traits;
pub mod db;
pub mod enclave;
pub mod exports;

// TODO: Move all of this.

extern {
    pub fn ecall_allocate(
        eid: sgx_enclave_id_t,
        retval: *mut EnclaveBuffer,
        buffer: *const u8,
        length: usize,
    ) -> sgx_status_t;

    pub fn perform_test(
        eid: sgx_enclave_id_t,
        retval: *mut sgx_status_t
    ) -> sgx_status_t;
}

/// This is a safe wrapper for allocating buffers inside the enclave.
fn allocate_enclave_buffer(buffer: &[u8]) -> SgxResult<EnclaveBuffer> {
    let ptr = buffer.as_ptr();
    let len = buffer.len();
    let mut enclave_buffer = EnclaveBuffer::default();

    // Bind the token to a local variable to ensure its
    // destructor runs in the end of the function
    let enclave_access_token = ENCLAVE_DOORBELL
        // This is always called from an ocall contxt
        .get_access(true)
        .ok_or(sgx_status_t::SGX_ERROR_BUSY)?;

    let enclave_id = enclave_access_token
        .expect("If we got here, surely the enclave has been loaded")
        .geteid();

    trace!(
        target: module_path!(),
        "allocate_enclave_buffer() called with len: {:?} enclave_id: {:?}",
        len,
        enclave_id,
    );

    match unsafe { ecall_allocate(enclave_id, &mut enclave_buffer, ptr, len) } {
        sgx_status_t::SGX_SUCCESS => Ok(enclave_buffer),
        failure_status => Err(failure_status),
    }
}


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
    if let Some(res) = DB.get(key).expect("failed to get") {
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
fn ocall_db_delete(
    key: *const u8,
    key_len: usize,
) -> OcallReturn {
    let key = unsafe { std::slice::from_raw_parts(key, key_len) };

    // TODO: Remove expect
    DB.delete(key).expect("failed to delete");

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
    DB.put(key, value).expect("failed to put");

    OcallReturn::Success
}

#[no_mangle]
pub extern "C"
fn ocall_db_flush() -> OcallReturn
{
    // TODO: Remove expec
    DB.flush().expect("failed to flush");

    OcallReturn::Success
}

fn main() {
    let enclave_access_token = ENCLAVE_DOORBELL
        .get_access(false) // This can never be recursive
        .expect("failed to get access token (1)"); // TODO: remove expect
    let enclave = enclave_access_token.expect("failed to get access token (2)");

    let mut retval = sgx_status_t::SGX_SUCCESS;

    let start = SystemTime::now();

    let result = unsafe {
        perform_test(enclave.geteid(),
                      &mut retval)
    };

    let end = SystemTime::now();
    let elapsed = end.duration_since(start);
    let taken_ms = elapsed.unwrap_or_default().as_millis();

    match result {
        sgx_status_t::SGX_SUCCESS => {}
        _ => {
            println!("[-] ECALL Enclave Failed {}!", result.as_str());
            return;
        }
    }
    println!("[+] perform_test success (taken: {}ms)", taken_ms);
}
