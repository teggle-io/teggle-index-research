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
extern crate enclave_ffi_types;
extern crate lazy_static;
extern crate log;
extern crate parking_lot;
extern crate rocksdb;
extern crate mio;
extern crate net2;

use std::time::SystemTime;

use sgx_types::*;
use api::server::run_api_server;

use enclave::doorbell::ENCLAVE_DOORBELL;

pub(crate) mod traits;
pub(crate) mod db;
pub(crate) mod api;
pub(crate) mod enclave;

extern {
    #[allow(dead_code)]
    pub fn ecall_perform_test(
        eid: sgx_enclave_id_t,
        retval: *mut sgx_status_t
    ) -> sgx_status_t;
}

#[allow(dead_code)]
fn run_perform_test() {
    let enclave_access_token = ENCLAVE_DOORBELL
        .get_access(false) // This can never be recursive
        .expect("failed to get access token (1)"); // TODO: remove expect
    let enclave = enclave_access_token.expect("failed to get access token (2)");

    let mut retval = sgx_status_t::SGX_SUCCESS;

    let start = SystemTime::now();

    let result = unsafe {
        ecall_perform_test(enclave.geteid(),
                           &mut retval)
    };

    let end = SystemTime::now();
    let elapsed = end.duration_since(start);
    let taken_ms = elapsed.unwrap_or_default().as_millis();

    match result {
        sgx_status_t::SGX_SUCCESS => {}
        _ => {
            println!("[-] perform_test failed {}!", result.as_str());
            return;
        }
    }
    println!("[+] perform_test success (taken: {}ms)", taken_ms);
}

fn main() {
    //run_perform_test();
    run_api_server();
}
