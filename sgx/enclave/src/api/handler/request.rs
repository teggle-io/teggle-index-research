use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::str::FromStr;

use bytes::BytesMut;
use http::{Extensions, HeaderMap, HeaderValue, Method, Uri, Version};
use http::header::AsHeaderName;
use log::warn;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::SgxMutex;
use std::time::Instant;

use crate::api::handler::codec::GLOBAL_CODEC;
use crate::api::handler::context::Context;
use crate::api::handler::response::Response;
use crate::api::handler::router::route_request;
use crate::api::results::{EncodedResponseResult, Error, ErrorKind};
use crate::api::server::connection::{UPGRADE_OPT_KEEPALIVE};
use crate::api::server::httpc::HttpcReactor;

static CONN_KEEPALIVE: &str = "keep-alive";

pub(crate) async fn process_raw_request(
    httpc: Arc<SgxMutex<HttpcReactor>>,
    raw_req: RawRequest
) -> EncodedResponseResult {
    match raw_req.extract() {
        Some(mut req) => {
            let mut res = Response::from_request(&req);
            let mut ctx: Context = Context::new(httpc);

            route_request(&mut req, &mut res, &mut ctx).await?;

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
    bytes: usize, // Total bytes read.
    timeout: Option<Instant>,
}

impl RawRequest {
    #[inline]
    pub(crate) fn new(data: Vec<u8>, timeout: Instant) -> Result<Self, Error> {
        let mut req = Self {
            request: None,
            bytes: data.len(),
            data: BytesMut::from(data.as_slice()),
            timeout: Some(timeout)
        };
        req.try_decode()?;

        Ok(req)
    }

    #[inline]
    pub(crate) fn next(&mut self, data: Vec<u8>) -> Result<(), Error> {
        if data.len() > 0 {
            self.bytes += data.len();
            self.push(data);
        }

        self.try_decode()
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.bytes
    }

    #[inline]
    pub fn content_length(&self) -> Option<usize> {
        str::parse::<usize>(self.request
            .as_ref()?
            .headers_ref()?
            .get(http::header::CONTENT_LENGTH)?
            .to_str().ok()?).ok()
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
    pub fn upgrade_opts(&self) -> u8 {
        let mut opts = 0_u8;

        if self.has_header_value(http::header::CONNECTION, CONN_KEEPALIVE) {
            // Not sure this is really needed (I think this happens anyway).
            opts |= UPGRADE_OPT_KEEPALIVE;
        }

        opts
    }

    #[inline]
    pub(crate) fn extract(self) -> Option<Request> {
        match self.request {
            Some(req) => {
                let body = self.data.to_vec();
                let req = req.body(()).ok()?;

                Some(Request::new(req, body))
            }
            None => None,
        }
    }

    #[inline]
    pub fn check_timeout(&self, now: &Instant) -> bool {
        if let Some(timeout) = self.timeout.as_ref() {
            if now.gt(timeout) {
                return true;
            }
        }

        false
    }

    #[inline]
    pub fn has_header_value<K: AsHeaderName>(&self, key: K, val: &str) -> bool {
        if let Some(req) = self.request.as_ref() {
            if let Some(headers) = req.headers_ref() {
                return has_header(headers, key, val);
            }
        }

        false
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
}

pub(crate) struct Request {
    req: http::Request<()>,
    body: Vec<u8>,
    vars: Option<HashMap<String, String>>,
}

impl Request {
    #[inline]
    pub(crate) fn new(req: http::Request<()>, body: Vec<u8>) -> Self {
        Self { req, body, vars: None }
    }

    #[inline]
    pub(crate) fn vars(&mut self, vars: HashMap<String, String>) {
        self.vars = Some(vars)
    }

    #[inline]
    pub fn var<R, S>(&self, key: S) -> Option<R>
        where
            R: FromStr,
            S: Into<String>,
    {
        let key = key.into();
        self.vars.as_ref()?
            .get(key.as_str())?
            .parse()
            .ok()
    }

    #[inline]
    pub fn header<R, K>(&self, key: K) -> Option<R>
        where
            R: FromStr,
            K: AsHeaderName,
    {
        self.headers()
            .get(key.as_str())?
            .to_str().ok()?
            .parse().ok()
    }

    #[inline]
    pub fn has_header_value<K: AsHeaderName>(&self, key: K, val: &str) -> bool {
        has_header(self.headers(), key, val)
    }

    #[inline]
    pub(crate) fn should_keep_alive(&self) -> bool {
        return self.version().ne(&Version::HTTP_10)
            || self.has_header_value(http::header::CONNECTION, CONN_KEEPALIVE);
    }

    #[inline]
    pub(crate) fn json<T>(&self) -> Result<T, Error>
    where
        T: DeserializeOwned
    {
        let res: serde_json::Result<T>  = serde_json::from_reader(self.body.as_slice());
        match res {
            Ok(res) => {
                Ok(res)
            }
            Err(err) => {
                Err(Error::new_with_kind(ErrorKind::DecodeFault, err.to_string()))
            }
        }
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

fn has_header<K: AsHeaderName>(headers: &HeaderMap<HeaderValue>, key: K, val: &str) -> bool {
    if let Some(conn) = headers.get(key) {
        if let Ok(conn) = conn.to_str() {
            if conn.eq_ignore_ascii_case(val) {
                return true;
            }
        }
    }

    false
}
