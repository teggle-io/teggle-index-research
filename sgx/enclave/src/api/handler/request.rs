use alloc::vec::Vec;

use bytes::BytesMut;
use http::header::AsHeaderName;
use http::{Extensions, HeaderMap, HeaderValue, Method, Uri, Version};
use http::request::Parts;
use log::warn;

use api::handler::codec::GLOBAL_CODEC;
use api::handler::router::route_request;
use api::handler::response::{Response};
use api::handler::types::EncodedResponseResult;

static CONN_KEEPALIVE: &str = "keep-alive";

pub(crate) fn process_raw_request(request_body: Vec<u8>) -> EncodedResponseResult {
    return match GLOBAL_CODEC.decode(&mut BytesMut::from(request_body.as_slice())) {
        Ok(Some(req)) => {
            // Wrap request in handler::Request
            let req = Request::new(req);
            let mut res = Response::from_request(&req);

            return match route_request(&req, &mut res) {
                Ok(()) => {
                    res.encode()
                }
                Err(e) => {
                    warn!("failed to dispatch request - {:?}", e);
                    res.fault();
                    res.encode()
                }
            }
        }
        Ok(None) => {
            warn!("failed to decode request");
            Response::encode_fault()
        }
        Err(e) => {
            warn!("failed to decode request - {:?}", e);
            Response::encode_fault()
        }
    }
}

pub(crate) struct Request {
    req: http::Request<Vec<u8>>,
}

impl Request {
    #[inline]
    pub(crate) fn new(req: http::Request<Vec<u8>>) -> Self {
        Self { req }
    }

    #[inline]
    pub fn has_header_value<K: AsHeaderName>(&self, key: K, val: &str) -> bool {
        if let Some(conn) = self.headers().get(key) {
            if let Ok(conn) = conn.to_str() {
                if conn.eq_ignore_ascii_case(val) {
                    return true
                }
            }
        }

        false
    }

    #[inline]
    pub(crate) fn should_keep_alive(&self) -> bool {
        return self.version().ne(&Version::HTTP_10)
            || self.has_header_value(http::header::CONNECTION, CONN_KEEPALIVE);
    }

    // Proxies
    #[allow(dead_code)]
    #[inline]
    pub fn method(&self) -> &Method {
        self.req.method()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn method_mut(&mut self) -> &mut Method {
        self.req.method_mut()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn uri(&self) -> &Uri {
        self.req.uri()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn uri_mut(&mut self) -> &mut Uri {
        self.req.uri_mut()
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn version(&self) -> Version {
        self.req.version()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn version_mut(&mut self) -> &mut Version {
        self.req.version_mut()
    }

    #[allow(dead_code)]
    #[inline]
    pub(crate) fn headers(&self) -> &HeaderMap<HeaderValue> {
        self.req.headers()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap<HeaderValue> {
        self.req.headers_mut()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn extensions(&self) -> &Extensions {
        self.req.extensions()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn extensions_mut(&mut self) -> &mut Extensions {
        self.req.extensions_mut()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn body(&self) -> &Vec<u8> {
        self.req.body()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn body_mut(&mut self) -> &mut Vec<u8> {
        self.req.body_mut()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn into_body(self) -> Vec<u8> {
        self.req.into_body()
    }

    #[allow(dead_code)]
    #[inline]
    pub fn into_parts(self) -> (Parts, Vec<u8>) {
        self.req.into_parts()
    }
}
