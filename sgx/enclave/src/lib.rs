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

#![crate_name = "index_enclave"]
#![crate_type = "staticlib"]

#![cfg_attr(not(target_env = "sgx"), no_std)]
#![cfg_attr(target_env = "sgx", feature(rustc_private))]

#[cfg(not(target_env = "sgx"))]
#[macro_use]
extern crate sgx_tstd as std;
extern crate sgx_types;
extern crate sgx_trts;
extern crate uuid;
extern crate enclave_ffi_types;
extern crate lazy_static;
extern crate alloc;

use alloc::string::ToString;
use sgx_types::*;
use std::io::{self, Write};
use std::slice;
use std::string::String;
//use std::untrusted::fs::File;
//use std::sgxfs::SgxFile;
use std::vec::Vec;
use uuid::Uuid;
use enclave_ffi_types::{Ctx, EnclaveBuffer, OcallReturn};
use external::ecalls::recover_buffer;
use external::ocalls::{ocall_db_flush, ocall_db_get, ocall_db_put};

mod utils;
pub mod external;

// TODO: Move these.
fn db_put(context: Ctx, key: &[u8], value: &[u8]) -> Result<(), String> {
    let mut ocall_return = OcallReturn::Success;

    let result = unsafe {
        ocall_db_put(
            (&mut ocall_return) as *mut _,
            context.unsafe_clone(),
            key.as_ptr(),
            key.len(),
            value.as_ptr(),
            value.len())
    };

    if result != sgx_status_t::SGX_SUCCESS {
        return Err(result.to_string());
    }
    if ocall_return != OcallReturn::Success {
        // TODO:
        println!("ocall_db_put returned {}", ocall_return);
        return Err("Ocall Fail (TODO ERR)".into_string());
    }

    Ok(())
}

fn db_get(context: Ctx, key: &[u8]) -> Result<(), String> {
    let mut ocall_return = OcallReturn::Success;

    let mut enclave_buffer = std::mem::MaybeUninit::<EnclaveBuffer>::uninit();

    let result = unsafe {
        ocall_db_get(
            (&mut ocall_return) as *mut _,
            context.unsafe_clone(),
            value.as_mut_ptr(),
            key.as_ptr(),
            key.len(),
        )
    };

    if result != sgx_status_t::SGX_SUCCESS {
        return Err(result.to_string());
    }
    if ocall_return != OcallReturn::Success {
        // TODO:
        println!("ocall_db_get returned {}", ocall_return);
        return Err("Ocall Fail (TODO ERR)".into_string());
    }

    let value = unsafe {
        let enclave_buffer = enclave_buffer.assume_init();
        recover_buffer(enclave_buffer)
    }.expect("failed to recover buffer").unwrap(); // TODO: Remove expect

    println!("VALUE: {:?}", value);

    Ok(())
}

fn db_flush(context: Ctx) -> Result<(), String> {
    let mut ocall_return = OcallReturn::Success;

    let result = unsafe {
        ocall_db_flush(
            (&mut ocall_return) as *mut _,
            context.unsafe_clone()
        )
    };

    if result != sgx_status_t::SGX_SUCCESS {
        return Err(result.to_string());
    }
    if ocall_return != OcallReturn::Success {
        // TODO:
        println!("ocall_db_flush returned {}", ocall_return);
        return Err("Ocall Fail (TODO ERR)".into_string());
    }

    Ok(())
}

#[no_mangle]
pub extern "C" fn perform_test(context: Ctx) -> sgx_status_t {
    /*
    let mut f = std::untrusted::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open("/tmp/test")
        .expect("Failed to open");

    f.write_all(b"Hello World");

     */

    //let enc_key = b"0234567890123456";
    //let opt = Options::new_disk_db_with(*enc_key);
    //let mut db = DB::open("/tmp/level.db", opt)
    //    .expect("failed to open sled db");

    let total_keys = 2000000_u64;

    let mut keys: Vec<[u8; 32]> = Vec::new();

    let key_ns = Uuid::parse_str("21a117c5-8ec5-417f-974a-9ff9441f754d").unwrap();

    for i in 0..total_keys {
        let cur_key = Uuid::new_v5(&key_ns, &i.to_be_bytes());

        let mut val: [u8; 32] = Default::default();
        val.copy_from_slice(format!("{}", cur_key.to_simple()).as_bytes());

        keys.push(val);
    }

    for k in keys.iter() {
        db_put(ctx, k, k).expect("failed to set storage");
    }

    db_flush(ctx).expect("failed to flush storage");

    /*
    for k in keys.iter() {
        storage_get(k).expect("failed to get storage");
    }
     */

    sgx_status_t::SGX_SUCCESS
}
