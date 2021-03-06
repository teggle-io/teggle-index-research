use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::str::FromStr;

use bytes::BytesMut;
use http::{Extensions, HeaderMap, HeaderValue, Method, Uri, Version};
use http::header::AsHeaderName;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::SgxMutex;
use std::time::Instant;
use tungstenite::handshake::server::create_response;

use crate::api::handler::codec::GLOBAL_CODEC;
use crate::api::handler::context::Context;
use crate::api::handler::response::Response;
use crate::api::handler::router::route_request;
use crate::api::reactor::httpc::HttpcReactor;
use crate::api::results::{Error, ErrorKind, too_many_bytes_err};
use crate::api::server::config::Config;
use crate::api::server::connection::Deferral;
use crate::api::server::websocket::WebSocket;

static HEADER_CONNECTION_KEEPALIVE: &str = "keep-alive";
static HEADER_CONNECTION_UPGRADE: &str = "upgrade";

static HEADER_UPGRADE_WEBSOCKET: &str = "websocket";

pub(crate) async fn process_raw_request(
    deferral: Arc<SgxMutex<Deferral>>,
    httpc: Arc<SgxMutex<HttpcReactor>>,
    raw_req: RawRequest,
) {
    let result = match raw_req.extract() {
        Some(req) => {
            let mut res = Response::from_request(&req);
            let mut ctx: Context = Context::new(req, httpc, None);

            match route_request(&mut ctx, &mut res).await {
                Ok(_) => res.encode(),
                Err(err) => Err(err)
            }
        }
        None => {
            Err(Error::new_with_kind(
                ErrorKind::ServerFault,
                "failed to extract request from raw request".to_string(),
            ))
        }
    };

    match deferral.lock() {
        Ok(mut deferral) => {
            if let Err(err) = deferral.defer(Box::new(move |conn| {
                match &result {
                    Ok(res) => {
                        conn.send_response(res);
                    }
                    Err(err) => {
                        conn.handle_error(&err);
                    }
                }

                Ok(())
            })) {
                warn!("failed to submit 'defer' \
                    during process_raw_request: {:?}", err);
            }
        }
        Err(err) => {
            warn!("failed to acquire lock on 'deferral' \
                    during process_raw_request: {:?}", err);
        }
    }
}

pub(crate) async fn process_ws_raw_request(
    deferral: Arc<SgxMutex<Deferral>>,
    httpc: Arc<SgxMutex<HttpcReactor>>,
    raw_req: RawRequest,
) {
    let ws = Arc::new(SgxMutex::new(WebSocket::new(
        deferral.clone()
    )));
    let (result, ctx) = match raw_req.extract() {
        Some(req) => {
            match create_response(req.request().into()) {
                Ok(res) => {
                    let (parts, _) = res.into_parts();
                    let mut res = Response::from_request_and_parts(&req, parts);
                    let mut ctx: Context = Context::new(req, httpc, Some(ws.clone()));

                    (
                        match route_request(&mut ctx, &mut res).await {
                            Ok(_) => res.encode(),
                            Err(err) => Err(err)
                        },
                        Some(ctx)
                    )
                }
                Err(err) => {
                    (Err(Error::new_with_kind(
                        ErrorKind::WSFault,
                        format!("failed to extract ws request - {:?}", err),
                    )), None)
                }
            }
        }
        None => {
            (Err(Error::new_with_kind(
                ErrorKind::ServerFault,
                "failed to extract ws request from raw request".to_string(),
            )), None)
        }
    };

    match deferral.lock() {
        Ok(mut deferral) => {
            if let Err(err) = deferral.defer(Box::new(move |conn| {
                match &result {
                    Ok(res) => {
                        if let Some(ctx) = ctx {
                            conn.send_response(res);
                            conn.websocket(ws.clone(), ctx)?;
                        } else {
                            conn.handle_error(&Error::new_with_kind(
                                ErrorKind::ServerFault,
                                "illegal state during process_ws_raw_request \
                                (no context)".to_string(),
                            ));
                        }
                    }
                    Err(err) => {
                        conn.handle_error(&err);
                    }
                }

                Ok(())
            })) {
                warn!("failed to submit 'defer' \
                    during process_ws_raw_request: {:?}", err);
            }
        }
        Err(err) => {
            warn!("failed to acquire lock on 'deferral' \
                    during process_ws_raw_request: {:?}", err);
        }
    }
}

pub(crate) struct RawRequest {
    request: Option<http::request::Builder>,
    data: BytesMut,
    // Total bytes read.
    bytes: usize,
    timeout: Option<Instant>,
    // Cached
    upgrade_websocket: bool,
    content_length: usize,
}

impl RawRequest {
    #[inline]
    pub(crate) fn new(data: Vec<u8>, timeout: Instant) -> Result<Self, Error> {
        let mut req = Self {
            request: None,
            bytes: data.len(),
            data: BytesMut::from(data.as_slice()),
            timeout: Some(timeout),
            upgrade_websocket: false,
            content_length: 0,
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
    pub(crate) fn is_upgrade_websocket(&self) -> bool {
        self.upgrade_websocket
    }

    #[inline]
    pub(crate) fn ready(&self) -> bool {
        if self.request.is_none() {
            return false;
        }

        if self.content_length > 0 {
            if self.data.len() < self.content_length {
                return false;
            }
        }

        return true;
    }

    #[inline]
    pub(crate) fn extract(self) -> Option<Request> {
        match self.request {
            Some(req) => {
                let body = self.data.to_vec();
                let req = req.body(()).ok()?;

                Some(Request::new(req, body, self.upgrade_websocket))
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
    pub fn validate(&self, config: Arc<Config>) -> Result<(), Error> {
        if self.request.is_none() {
            return Err(Error::new_with_kind(
                ErrorKind::ServerFault,
                "request validation failed - no request object".to_string(),
            ));
        }

        // Check payload size.
        if self.content_length > 0 {
            if self.content_length > config.max_bytes_received() {
                return Err(too_many_bytes_err(self.content_length,
                                              config.max_bytes_received()));
            }
        }

        Ok(())
    }

    // private

    #[inline]
    fn try_decode(&mut self) -> Result<(), Error> {
        if self.request.is_none() {
            self.request = GLOBAL_CODEC.decode(&mut self.data)?;
        }

        self.extract_upgrade_opts();
        self.extract_content_length();

        Ok(())
    }

    #[inline]
    fn push(&mut self, body: Vec<u8>) {
        self.data.extend_from_slice(body.as_slice());
    }

    #[inline]
    fn extract_upgrade_opts(&mut self) {
        if let Some(req) = self.request.as_ref() {
            if let Some(headers) = req.headers_ref() {
                if has_header(headers, http::header::CONNECTION, HEADER_CONNECTION_UPGRADE)
                    && has_header(headers, http::header::UPGRADE, HEADER_UPGRADE_WEBSOCKET) {
                    self.upgrade_websocket = true;
                }
            }
        }
    }

    #[inline]
    fn extract_content_length(&mut self) {
        self.content_length = 0;

        if let Some(req) = self.request.as_ref() {
            if let Some(headers) = req.headers_ref() {
                if let Some(val) = headers.get(http::header::CONTENT_LENGTH) {
                    if let Ok(val) = val.to_str() {
                        if let Ok(val) = str::parse::<usize>(val) {
                            self.content_length = val;
                        }
                    }
                }
            }
        }
    }
}

pub struct Request {
    req: http::Request<()>,
    body: Vec<u8>,
    vars: Option<HashMap<String, String>>,
    websocket: bool,
}

impl Request {
    #[inline]
    pub(crate) fn new(
        req: http::Request<()>,
        body: Vec<u8>,
        websocket: bool,
    ) -> Self {
        Self { req, body, vars: None, websocket }
    }

    #[inline]
    pub fn is_websocket(&self) -> bool {
        self.websocket
    }

    #[inline]
    fn request(&self) -> &http::Request<()> {
        &self.req
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
            || self.has_header_value(http::header::CONNECTION, HEADER_CONNECTION_KEEPALIVE);
    }

    #[inline]
    pub(crate) fn json<T>(&self) -> Result<T, Error>
        where
            T: DeserializeOwned
    {
        let res: serde_json::Result<T> = serde_json::from_reader(self.body.as_slice());
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
