use alloc::vec::Vec;

use bytes::BytesMut;
use http::header::AsHeaderName;
use http::{Request, Version};
use log::warn;

use api::handler::codec::GLOBAL_CODEC;
use api::handler::dispatch::dispatch_request;
use api::handler::response::{encode_response, encode_response_server_fault};
use api::handler::types::ResponseBodyResult;

static CONN_KEEPALIVE: &str = "keep-alive";

pub(crate) fn process_raw_request(request_body: Vec<u8>) -> ResponseBodyResult {
    return match GLOBAL_CODEC.decode(&mut BytesMut::from(request_body.as_slice())) {
        Ok(Some(req)) => {
            return match dispatch_request(&req) {
                Ok(res) => {
                    encode_response(Some(&req), res)
                }
                Err(e) => {
                    warn!("failed to dispatch request - {:?}", e);
                    encode_response_server_fault(Some(&req))
                }
            }
        }
        Ok(None) => {
            warn!("failed to decode request");
            encode_response_server_fault(None)
        }
        Err(e) => {
            warn!("failed to decode request - {:?}", e);
            encode_response_server_fault(None)
        }
    }
}

pub(crate) fn request_has_header_value<K: AsHeaderName>(
    req: Option<&Request<()>>,
    key: K,
    val: &str,
) -> bool {
    if let Some(req) = req {
        if let Some(conn) = req.headers().get(key) {
            if let Ok(conn) = conn.to_str() {
                if conn.eq_ignore_ascii_case(val) {
                    return true
                }
            }
        }
    }

    false
}

pub(crate) fn should_keep_alive(req: Option<&Request<()>>) -> bool {
    if let Some(r) = req {
        return r.version().ne(&Version::HTTP_10)
            || request_has_header_value(req, http::header::CONNECTION, CONN_KEEPALIVE);
    }

    false
}