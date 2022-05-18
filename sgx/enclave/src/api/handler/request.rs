use alloc::string::String;
use alloc::vec::Vec;
use core::str::FromStr;

use bytes::BytesMut;
use http::{Extensions, HeaderMap, HeaderValue, Method, Uri, Version};
use http::header::AsHeaderName;
use log::warn;
use std::collections::HashMap;

use api::handler::codec::GLOBAL_CODEC;
use api::handler::response::Response;
use api::handler::router::route_request;
use api::results::{EncodedResponseResult, Error};

static CONN_KEEPALIVE: &str = "keep-alive";

pub(crate) fn process_raw_request(req: RawRequest) -> EncodedResponseResult {
    match req.request() {
        Some(req) => {
            // Wrap request in handler::Request
            let mut req = Request::new(req);
            let mut res = Response::from_request(&req);

            route_request(&mut req, &mut res)?;

            res.encode()
        }
        None => {
            warn!("failed to expand raw request from builder");
            Response::encode_fault()
        }
    }
}

pub(crate) struct RawRequest {
    request: Option<http::request::Builder>,
    data: BytesMut,
}

impl RawRequest {
    #[inline]
    pub(crate) fn new(data: Vec<u8>) -> Result<Self, Error> {
        let mut req = Self {
            request: None,
            data: BytesMut::from(data.as_slice()),
        };
        req.try_decode()?;

        Ok(req)
    }

    #[inline]
    pub(crate) fn next(&mut self, data: Vec<u8>) -> Result<(), Error> {
        self.push(data);
        self.try_decode()
    }

    #[inline]
    pub(crate) fn ready(&self) -> bool {
        if self.request.is_none() {
            return false;
        }

        if let Some(content_length) = self.content_length() {
            if content_length > 0 {
                if self.data.len() < content_length {
                    return false;
                }
            }
        }

        return true;
    }

    #[inline]
    pub(crate) fn request(self) -> Option<http::Request<()>> {
        match self.request {
            Some(req) => {
                req.body(()).ok()
            }
            None => None,
        }
    }

    // private

    #[inline]
    fn try_decode(&mut self) -> Result<(), Error> {
        if self.request.is_none() {
            self.request = GLOBAL_CODEC.decode(&mut self.data)?;
        }

        Ok(())
    }

    #[inline]
    fn push(&mut self, body: Vec<u8>) {
        self.data.extend_from_slice(body.as_slice());
    }

    #[inline]
    fn content_length(&self) -> Option<usize> {
        let content_length = self.request
            .as_ref()?
            .headers_ref()?
            .get(http::header::CONTENT_LENGTH)?
            .to_str().ok()?;

        str::parse::<usize>(content_length).ok()
    }
}

pub(crate) struct Request {
    req: http::Request<()>,
    path_vars: Option<HashMap<String, String>>,
}

impl Request {
    #[inline]
    pub(crate) fn new(req: http::Request<()>) -> Self {
        Self { req, path_vars: None }
    }

    #[inline]
    pub(crate) fn path_vars(&mut self, vars: HashMap<String, String>) {
        self.path_vars = Some(vars)
    }

    #[inline]
    pub fn path_var<R, S>(&self, key: S) -> Option<R>
        where
            R: FromStr,
            S: Into<String>,
    {
        let key = key.into();
        self.path_vars.as_ref()?
            .get(key.as_str())?
            .parse()
            .ok()
    }

    #[inline]
    pub fn has_header_value<K: AsHeaderName>(&self, key: K, val: &str) -> bool {
        if let Some(conn) = self.headers().get(key) {
            if let Ok(conn) = conn.to_str() {
                if conn.eq_ignore_ascii_case(val) {
                    return true;
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
}
