use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;

use mio_httpc::{CallBuilder, Method};
use std::collections::HashMap;
use std::sync::SgxMutex;

use crate::api::handler::request::Request;
use crate::api::reactor::httpc::{HttpcCallFuture, HttpcReactor};
use crate::api::results::{Error, ErrorKind};
use crate::api::server::websocket::{SubscriptionHandlerFn, WebSocket};

const FETCH_DEFAULT_TIMEOUT_MS: u64 = 2500;

type ContextValue = dyn Any + Sync + Send + 'static;

pub struct Context {
    request: Request,
    httpc: Arc<SgxMutex<HttpcReactor>>,
    ws: Option<Arc<SgxMutex<WebSocket>>>,
    data: HashMap<&'static str, Box<ContextValue>>,
}

#[allow(dead_code)]
impl Context {
    #[inline]
    pub(crate) fn new(
        request: Request,
        httpc: Arc<SgxMutex<HttpcReactor>>,
        ws: Option<Arc<SgxMutex<WebSocket>>>,
    ) -> Self {
        Self {
            request,
            httpc,
            ws,
            data: HashMap::new(),
        }
    }

    pub fn request(&self) -> &Request {
        &self.request
    }

    pub fn request_mut(&mut self) -> &mut Request {
        &mut self.request
    }

    // Web Sockets

    pub fn is_websocket(&self) -> bool {
        self.ws.is_some() && self.request.is_websocket()
    }

    pub fn subscribe(&self, handler: SubscriptionHandlerFn) -> Result<(), Error> {
        if !self.is_websocket() {
            return Err(Error::new_with_kind(
                ErrorKind::WSFault,
                format!("attempt to call Context->subscribe when request is not a web socket"),
            ));
        }

        let handler = Arc::new(handler);

        return match self.ws.as_ref().unwrap().lock() {
            Ok(mut ws) => {
                ws.subscribe(handler.clone())
            }
            Err(err) => {
                Err(Error::new_with_kind(
                    ErrorKind::WSFault,
                    format!("failed to acquire lock on 'ws' during Context->subscribe: {:?}", err),
                ))
            }
        };
    }

    pub fn send(&self, data: Vec<u8>) -> Result<(), Error> {
        if !self.is_websocket() {
            return Err(Error::new_with_kind(
                ErrorKind::WSFault,
                format!("attempt to call Context->send when request is not a web socket"),
            ));
        }

        return match self.ws.as_ref().unwrap().lock() {
            Ok(mut ws) => {
                ws.send(data)
            }
            Err(err) => {
                Err(Error::new_with_kind(
                    ErrorKind::WSFault,
                    format!("failed to acquire lock on 'ws' during Context->send: {:?}", err),
                ))
            }
        };
    }

    // HTTP Client

    #[inline]
    pub fn http(&self) -> HttpFetchBuilder {
        HttpFetchBuilder::http(self.httpc.clone())
    }

    #[inline]
    pub fn https(&self) -> HttpFetchBuilder {
        HttpFetchBuilder::https(self.httpc.clone())
    }

    // Context Data

    #[inline]
    pub fn insert(&mut self, key: &'static str, value: Box<ContextValue>) -> &mut Self {
        self.data.insert(key, value);
        self
    }

    #[inline]
    pub fn get<V>(&self, key: &'static str) -> Option<&V>
        where
            V: 'static
    {
        Some(
            self.data.get(&key)?
                .downcast_ref()?
        )
    }

    #[inline]
    pub fn contains_key(&mut self, key: &'static str) -> bool {
        self.data.contains_key(&key)
    }
}

pub struct HttpFetchBuilder {
    httpc: Arc<SgxMutex<HttpcReactor>>,
    builder: Option<CallBuilder>,
}

#[allow(dead_code)]
impl HttpFetchBuilder {
    #[inline]
    fn new(httpc: Arc<SgxMutex<HttpcReactor>>) -> Self {
        let mut builder = CallBuilder::new();
        builder.timeout_ms(FETCH_DEFAULT_TIMEOUT_MS);

        Self { httpc, builder: Some(builder) }
    }

    #[inline]
    fn https(httpc: Arc<SgxMutex<HttpcReactor>>) -> Self {
        let mut new = Self::new(httpc);
        new.builder.as_mut().unwrap().https();
        new
    }

    #[inline]
    fn http(httpc: Arc<SgxMutex<HttpcReactor>>) -> Self {
        Self::new(httpc)
    }

    #[inline]
    pub fn host(&mut self, host: &str) -> &mut Self {
        self.builder.as_mut().unwrap().host(host);
        self
    }

    #[inline]
    pub fn port(&mut self, port: u16) -> &mut Self {
        self.builder.as_mut().unwrap().port(port);
        self
    }

    #[inline]
    pub fn method(&mut self, method: Method) -> &mut Self {
        self.builder.as_mut().unwrap().method_typed(method);
        self
    }

    #[inline]
    /// Set full path. No percent encoding is done. Will fail later if it contains invalid characters.
    pub fn path(&mut self, path: &str) -> &mut Self {
        self.builder.as_mut().unwrap().path(path);
        self
    }

    #[inline]
    /// Add a single segment of path. Parts are delimited by / which are added automatically.
    /// Any path unsafe characters are percent encoded.
    /// If part contains /, it will be percent encoded!
    pub fn path_segment(&mut self, path_segment: &str) -> &mut Self {
        self.builder.as_mut().unwrap().path_segm(path_segment);
        self
    }

    #[inline]
    /// Set full URL. If not valid it will return error. Be mindful of characters
    /// that need to be percent encoded. Using https, path_segm, query and auth functions
    /// to construct URL is much safer as those encode data automatically.
    pub fn url(&mut self, url: &str) -> Result<(), Error> {
        self.builder.as_mut().unwrap().url(url)
            .map_err(|_e| {
                Error::new_with_kind(
                    ErrorKind::HttpClientError,
                    "failed to set url".to_string())
            })?;
        Ok(())
    }

    #[inline]
    pub fn header(&mut self, key: &str, value: &str) -> &mut Self {
        self.builder.as_mut().unwrap().header(key, value);
        self
    }

    #[inline]
    pub fn body(&mut self, body: Vec<u8>) -> &mut Self {
        self.builder.as_mut().unwrap().body(body);
        self
    }

    #[inline]
    pub fn fetch(&mut self) -> HttpcCallFuture {
        if self.builder.is_none() {
            return HttpcCallFuture::from_error(
                Error::new_with_kind(ErrorKind::HttpClientError,
                                     "fetch() called with no builder.".to_string())
            );
        }

        match self.httpc.lock() {
            Ok(mut lock) => {
                let builder = self.builder.take().unwrap();
                //trace!("fetching: {}", builder.get_url());

                lock.call(builder)
            }
            Err(err) => {
                HttpcCallFuture::from_error(
                    Error::new_with_kind(ErrorKind::HttpClientError,
                                         format!("failed to get lock on 'httpc' during HttpFetchBuilder->fetch: {:?}", err))
                )
            }
        }
    }

    #[inline]
    pub fn get(&mut self) -> HttpcCallFuture {
        self.method(Method::GET);
        self.fetch()
    }

    #[inline]
    pub fn post(&mut self, body: Vec<u8>) -> HttpcCallFuture {
        self.method(Method::POST);
        self.body(body);
        self.fetch()
    }

    #[inline]
    pub fn put(&mut self, body: Vec<u8>) -> HttpcCallFuture {
        self.method(Method::PUT);
        self.body(body);
        self.fetch()
    }

    #[inline]
    pub fn patch(&mut self, body: Vec<u8>) -> HttpcCallFuture {
        self.method(Method::PATCH);
        self.body(body);
        self.fetch()
    }

    #[inline]
    pub fn delete(&mut self) -> HttpcCallFuture {
        self.method(Method::DELETE);
        self.fetch()
    }

    #[inline]
    pub fn options(&mut self) -> HttpcCallFuture {
        self.method(Method::OPTIONS);
        self.fetch()
    }

    #[inline]
    pub fn head(&mut self) -> HttpcCallFuture {
        self.method(Method::HEAD);
        self.fetch()
    }
}