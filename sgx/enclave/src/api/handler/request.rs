use alloc::string::String;
use alloc::vec::Vec;

use bytes::BytesMut;
use log::warn;
use api::handler::codec::GLOBAL_CODEC;

use api::handler::dispatch::dispatch_request;
use api::handler::response::{encode_response, encode_response_server_fault};

pub(crate) fn process_raw_request(request_body: Vec<u8>) -> Result<Vec<u8>, String> {
    match GLOBAL_CODEC.decode(&mut BytesMut::from(request_body.as_slice())) {
        Ok(Some(req)) => {
            match dispatch_request(req) {
                Ok(res) => {
                    encode_response(res)
                }
                Err(e) => {
                    warn!("api: failed to dispatch request - {:?}", e);
                    encode_response_server_fault()
                }
            }
        }
        Ok(None) => {
            warn!("api: failed to decode request");
            encode_response_server_fault()
        }
        Err(e) => {
            warn!("api: failed to decode request - {:?}", e);
            encode_response_server_fault()
        }
    }
}

