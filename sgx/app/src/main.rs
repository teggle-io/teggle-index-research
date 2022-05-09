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

extern crate sgx_types;
extern crate sgx_urts;
extern crate rocksdb;

use std::ptr;
use sgx_types::*;
use sgx_urts::SgxEnclave;
use std::time::SystemTime;
use rocksdb::{DB, DBCompactionStyle, Options};

static ENCLAVE_FILE: &'static str = "enclave.signed.so";
static mut ROCKS_DB: Option<DB> = None;

extern {
    fn say_something(eid: sgx_enclave_id_t, retval: *mut sgx_status_t,
                     some_string: *const u8, len: usize) -> sgx_status_t;
}

#[no_mangle]
pub extern "C"
fn ocall_storage_set(key: *const u8,
                     key_len: usize,
                     value: *const u8,
                     value_len: usize) -> sgx_status_t {
    let key = unsafe { std::slice::from_raw_parts(key, key_len) };
    let value = unsafe { std::slice::from_raw_parts(value, value_len) };

    unsafe {
        if let Some(db) = ROCKS_DB.as_ref() {
            // TODO: Remove expect
            db.put(key, value).expect("failed to put");
        }
    }

    sgx_status_t::SGX_SUCCESS
}

#[no_mangle]
pub extern "C"
fn ocall_storage_get(key: *const u8,
                     key_len: usize,
                     value: *mut u8,
                     value_max_len: usize) -> sgx_status_t {
    let key = unsafe { std::slice::from_raw_parts(key, key_len) };

    unsafe {
        if let Some(db) = ROCKS_DB.as_ref() {
            // TODO: Remove expect
            if let Some(res) = db.get(key).expect("failed to get") {
                if value_max_len >= res.len() {
                    // TODO: Once we define a custom type.
                    //unsafe { *value = res };
                    ptr::copy_nonoverlapping(res.as_ptr(), value, res.len());
                } else {
                    // TODO:
                    return sgx_status_t::SGX_ERROR_UNEXPECTED;
                }
            } else {
                // TODO: None.
                return sgx_status_t::SGX_ERROR_UNEXPECTED;
            }
        }
    }

    sgx_status_t::SGX_SUCCESS
}

#[no_mangle]
pub extern "C"
fn ocall_storage_flush() -> sgx_status_t {
    unsafe {
        if let Some(db) = ROCKS_DB.as_ref() {
            // TODO: Remove expect
            db.flush().expect("failed to flush");
        }
    }

    sgx_status_t::SGX_SUCCESS
}

fn init_enclave() -> SgxResult<SgxEnclave> {
    let mut launch_token: sgx_launch_token_t = [0; 1024];
    let mut launch_token_updated: i32 = 0;
    // call sgx_create_enclave to initialize an enclave instance
    // Debug Support: set 2nd parameter to 1
    let debug = 1;
    let mut misc_attr = sgx_misc_attribute_t { secs_attr: sgx_attributes_t { flags: 0, xfrm: 0 }, misc_select: 0 };
    SgxEnclave::create(ENCLAVE_FILE,
                       debug,
                       &mut launch_token,
                       &mut launch_token_updated,
                       &mut misc_attr)
}

fn main() {
    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.set_compaction_style(DBCompactionStyle::Level);
    opts.set_write_buffer_size(67_108_864); // 64mb
    opts.set_max_write_buffer_number(3);
    opts.set_target_file_size_base(67_108_864); // 64mb
    opts.set_level_zero_file_num_compaction_trigger(8);
    opts.set_level_zero_slowdown_writes_trigger(17);
    opts.set_level_zero_stop_writes_trigger(24);
    opts.set_num_levels(4);
    opts.set_max_bytes_for_level_base(536_870_912); // 512mb
    opts.set_max_bytes_for_level_multiplier(8.0);

    unsafe {
        ROCKS_DB = Some(DB::open(&opts, "./rocks.db")
            .expect("failed to open rocks db"));
    }

    let enclave = match init_enclave() {
        Ok(r) => {
            println!("[+] Init Enclave Successful {}!", r.geteid());
            r
        }
        Err(x) => {
            println!("[-] Init Enclave Failed {}!", x.as_str());
            return;
        }
    };

    let input_string = String::from("This is a normal world string passed into Enclave!\n");
    let mut retval = sgx_status_t::SGX_SUCCESS;

    let start = SystemTime::now();

    let result = unsafe {
        say_something(enclave.geteid(),
                      &mut retval,
                      input_string.as_ptr() as *const u8,
                      input_string.len())
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
    println!("[+] say_something success (taken: {}ms)", taken_ms);
    enclave.destroy();
}
