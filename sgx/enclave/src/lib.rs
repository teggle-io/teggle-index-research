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

//extern crate rusty_leveldb;
#[cfg(not(target_env = "sgx"))]
#[macro_use]
extern crate sgx_tstd as std;
extern crate sgx_types;
extern crate uuid;

//use rusty_leveldb::{DB, Options};
use sgx_types::*;
use std::io::{self, Write};
use std::slice;
use std::string::String;
//use std::untrusted::fs::File;
//use std::sgxfs::SgxFile;
use std::vec::Vec;
use uuid::Uuid;

extern "C" {
    pub fn ocall_storage_set(
        ret_val: *mut sgx_status_t,
        p_key: *const u8,
        key_len: usize,
        p_value: *const u8,
        value_len: usize,
    ) -> sgx_status_t;

    pub fn ocall_storage_get(
        ret_val: *mut sgx_status_t,
        p_key: *const u8,
        key_len: usize,
        value: *mut u8,
        value_max_len: usize
    ) -> sgx_status_t;

    pub fn ocall_storage_flush(
        ret_val: *mut sgx_status_t
    ) -> sgx_status_t;
}

fn storage_set(key: &[u8], value: &[u8]) -> Result<(), sgx_status_t> {
    let mut rt : sgx_status_t = sgx_status_t::SGX_ERROR_UNEXPECTED;

    let result = unsafe {
        ocall_storage_set(&mut rt as *mut sgx_status_t,
                    key.as_ptr(),
                    key.len(),
                    value.as_ptr(),
                    value.len())
    };

    if result != sgx_status_t::SGX_SUCCESS {
        return Err(result);
    }
    if rt != sgx_status_t::SGX_SUCCESS {
        println!("ocall_storage_set returned {}", rt);
        return Err(rt);
    }

    Ok(())
}

fn storage_get(key: &[u8]) -> Result<(), sgx_status_t> {
    let mut rt : sgx_status_t = sgx_status_t::SGX_ERROR_UNEXPECTED;

    let value_max_len: usize = 32;
    // TODO:
    // let mut enclave_buffer = std::mem::MaybeUninit::<EnclaveBuffer>::uninit();
    let mut value: Vec<u8> = vec![0; value_max_len];

    let result = unsafe {
        ocall_storage_get(&mut rt as *mut sgx_status_t,
                          key.as_ptr(),
                          key.len(),
                          value.as_mut_ptr(),
                          value_max_len)
    };

    if result != sgx_status_t::SGX_SUCCESS {
        return Err(result);
    }
    if rt != sgx_status_t::SGX_SUCCESS {
        println!("ocall_storage_set returned {}", rt);
        return Err(rt);
    }

    println!("VALUE: {:?}", value);

    Ok(())
}

fn storage_flush() -> Result<(), sgx_status_t> {
    let mut rt : sgx_status_t = sgx_status_t::SGX_ERROR_UNEXPECTED;

    let result = unsafe {
        ocall_storage_flush(&mut rt as *mut sgx_status_t)
    };

    if result != sgx_status_t::SGX_SUCCESS {
        return Err(result);
    }
    if rt != sgx_status_t::SGX_SUCCESS {
        println!("ocall_storage_flush returned {}", rt);
        return Err(rt);
    }

    Ok(())
}

#[no_mangle]
pub extern "C" fn say_something(some_string: *const u8, some_len: usize) -> sgx_status_t {
    let str_slice = unsafe { slice::from_raw_parts(some_string, some_len) };
    let _ = io::stdout().write(str_slice);

    // A sample &'static string
    let rust_raw_string = "This is a in-Enclave ";
    // An array
    let word: [u8; 4] = [82, 117, 115, 116];
    // An vector
    let word_vec: Vec<u8> = vec![32, 115, 116, 114, 105, 110, 103, 33];

    // Construct a string from &'static string
    let mut hello_string = String::from(rust_raw_string);

    // Iterate on word array
    for c in word.iter() {
        hello_string.push(*c as char);
    }

    // Rust style convertion
    hello_string += String::from_utf8(word_vec).expect("Invalid UTF-8")
        .as_str();

    // Ocall to normal world for output
    println!("{}", &hello_string);
    println!("HERE");

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

    /*
    for k in keys.iter() {
        storage_set(k, k).expect("failed to set storage");
    }

    storage_flush().expect("failed to flush storage");
     */

    for k in keys.iter() {
        storage_get(k).expect("failed to get storage");
    }

    sgx_status_t::SGX_SUCCESS
}
