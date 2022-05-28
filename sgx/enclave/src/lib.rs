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

#![feature(try_reserve)]

#![cfg_attr(not(target_env = "sgx"), no_std)]
#![cfg_attr(target_env = "sgx", feature(rustc_private))]

#[cfg(not(target_env = "sgx"))]
#[macro_use]
extern crate sgx_tstd as std;
extern crate sgx_types;
extern crate sgx_trts;
extern crate enclave_ffi_types;

extern crate pretty_env_logger;
#[macro_use] extern crate log;

extern crate alloc;
extern crate blake2;
extern crate digest;
#[macro_use] extern crate lazy_static;
extern crate ring;
extern crate sha2;
extern crate uuid;
extern crate webpki;
extern crate rustls;
extern crate rustls_pemfile;
extern crate mio;
extern crate mio_httpc;
extern crate net2;
extern crate futures;
extern crate http;
extern crate httparse;
extern crate httpdate;
extern crate tungstenite;
extern crate bytes;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use blake2::VarBlake2b;
use blake2::digest::{Input, VariableOutput};
use digest::FixedOutput;
use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey as Key, Nonce, UnboundKey};
use ring::hkdf;
use sgx_types::*;
use std::vec::Vec;
use uuid::Uuid;

mod api;
mod utils;
pub mod external;

#[no_mangle]
pub extern "C" fn ecall_perform_test() -> sgx_status_t {
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

    // 450ms (overhead for producing test keys).

    //// Key Scrambling

    // Test with blake2b (1350ms, so 900ms)
    //scramble_with_blake2b(keys);

    // Test with sha256 (1133ms, so 683ms)
    //scramble_with_sha256(keys);

    // Test with hkdf (1425ms, so 975ms)
    //scramble_with_hkdf(keys);

    // Test with uuid v5 (700ms, so 250ms)
    //scramble_with_uuid_v5(keys);

    //// Encryption
    // Test with chacha20poly1305 (3150ms, so 2700ms)
    encrypt_with_chacha20poly1305(keys);

    sgx_status_t::SGX_SUCCESS
}

//// Key Scrambling

// Key scrambling tests.
#[allow(dead_code)]
fn scramble_with_hkdf(keys: Vec<[u8; 32]>) {
    let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, b"test salt");
    let pkr = salt.extract(b"test secret");

    for k in keys.iter() {
        let ScrambledKey(_out) = pkr.expand(&[k], ScrambledKey(k.len()))
            .unwrap()
            .into();

        //print_result(k, out.as_slice());
    }
}

#[allow(dead_code)]
fn scramble_with_blake2b(keys: Vec<[u8; 32]>) {
    let my_key = b"test secret";

    for k in keys.iter() {
        let mut hash = VarBlake2b::new_keyed(my_key, 32);
        hash.input(k);

        hash.variable_result(|_res| {
            // TODO:
            //print_result(k, res);
        });
    }
}

#[allow(dead_code)]
fn scramble_with_sha256(keys: Vec<[u8; 32]>) {
    let my_key = b"test secret";

    for k in keys.iter() {
        let mut hash = sha2::Sha256::default();
        hash.input(my_key);
        hash.input(k);

        let _res = hash.fixed_result();

        //print_result(k, res.as_slice());
    }
}

#[allow(dead_code)]
fn scramble_with_uuid_v5(keys: Vec<[u8; 32]>) {
    let priv_ns = Uuid::parse_str("6042dc53-9d3d-424f-8437-26c0e5abf043").unwrap();

    for k in keys.iter() {
        let _new_key = Uuid::new_v5(&priv_ns, k);
    }
}

// Length overhead:
//   AEAD = 16 bytes
//   NOONCE = 12 bytes
//
fn encrypt_with_chacha20poly1305(keys: Vec<[u8; 32]>) {
    let unbound_key = UnboundKey::new(&CHACHA20_POLY1305,
                                      b"an example very very secret key.")
        .expect("failed to make key");
    let key = Key::new(unbound_key);

    let seed = 1234_u32;
    let mut count = 0_u64;
    for k in keys.iter() {
        let adata = k; // TODO:
        let mut buffer = Vec::from(k.to_vec());
        //buffer.extend_from_slice(&k[..]);

        let mut nonce_val = [0u8; 12];
        nonce_val[0..4].copy_from_slice(&seed.to_be_bytes());
        nonce_val[4..12].copy_from_slice(&count.to_be_bytes());

        let nonce = Nonce::try_assume_unique_for_key(&nonce_val[..])
            .expect("failed to make noonce"); // 12-bytes; unique per message

        key.seal_in_place_append_tag(nonce, Aad::from(&adata), &mut buffer)
            .expect("failed to seal");

        let mut res: Vec<u8> = Vec::with_capacity(nonce_val.len() + buffer.len());
        res.extend(&nonce_val[..]);
        res.extend(&buffer);

        // Get it back ...
        // let (a, b) = res.split_at_mut(12);

        //buffer.splice(0..0, nonce_val.into_iter());

        //println!("NOONCE: {}, OLD: {}, NEW: {}", nonce_val.len(), k.len(), buffer.len());
        //print_result(k, res.as_slice());
        //print_result(k, &nonce_val[..]);

        count = count + 1;
    }
}

#[allow(dead_code)]
fn print_result(orig: &[u8], sum: &[u8]) {
    for byte in orig {
        print!("{:02x}", byte);
    }
    print!(" => ");
    for byte in sum {
        print!("{:02x}", byte);
    }
    println!("");
}

// HKDF

#[derive(Debug, PartialEq)]
struct ScrambledKey<T: core::fmt::Debug + PartialEq>(T);

impl hkdf::KeyType for ScrambledKey<usize> {
    fn len(&self) -> usize {
        self.0
    }
}

impl From<hkdf::Okm<'_, ScrambledKey<usize>>> for ScrambledKey<Vec<u8>> {
    fn from(okm: hkdf::Okm<ScrambledKey<usize>>) -> Self {
        let mut r = vec![0u8; okm.len().0];
        okm.fill(&mut r).unwrap();
        Self(r)
    }
}
