use lazy_static::lazy_static;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use bytes::BytesMut;

use http::{Response, StatusCode};
use log::warn;
use api::handler::codec::HttpCodec;
use api::handler::dispatch::dispatch_request;
use api::handler::response::{error_response};

pub(crate) mod codec;
pub(crate) mod dispatch;
pub(crate) mod response;

lazy_static! {
    static ref GLOBAL_CODEC: HttpCodec = {
        // TODO: Use indexer public key
        HttpCodec::new("d42aa7c4-ee7c-4082-9e4c-525aac4057bc.idx.teggle.io")
    };
}

pub(crate) fn process_raw_request(request_body: Vec<u8>) -> Result<Vec<u8>, String> {
    match GLOBAL_CODEC.decode(&mut BytesMut::from(request_body.as_slice())) {
        Ok(req) => {
            match req {
                Some(req) => {
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
                _ => {
                    warn!("api: failed to decode request");
                    encode_response_server_fault()
                }
            }
        }
        Err(e) => {
            warn!("api: failed to decode request - {:?}", e);
            encode_response_server_fault()
        }
    }
}

pub(crate) fn encode_response(res: Response<Vec<u8>>) -> Result<Vec<u8>, String> {
    let mut encoded = BytesMut::new();

    GLOBAL_CODEC.encode(res, &mut encoded)
        .map_err(|e| {
            e.to_string()
        })?;

    Ok(encoded.to_vec())
}

pub(crate) fn encode_response_server_fault() -> Result<Vec<u8>, String> {
    match error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Server Fault") {
        Ok(res) => encode_response(res),
        Err(e) => {
            warn!("api: failed to encode server fault response - {:?}", e);
            Err(e.to_string())
        }
    }
}