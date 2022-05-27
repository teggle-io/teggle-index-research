use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;
use core::str::FromStr;
use futures::FutureExt;

use mio_httpc::{CallBuilder, Method};
use std::collections::HashMap;
use std::sync::SgxMutex;
use crate::api::handler::request::Request;

use crate::api::reactor::httpc::{HttpcCallFuture, HttpcReactor};
use crate::api::results::{Error, ErrorKind};
use crate::api::server::connection::Deferral;

const FETCH_DEFAULT_TIMEOUT_MS: u64 = 2500;

pub(crate) struct Context {
    request: Request,
    deferral: Arc<SgxMutex<Deferral>>,
    httpc: Arc<SgxMutex<HttpcReactor>>,
    data: HashMap<String, String>,
}

#[allow(dead_code)]
impl Context {
    #[inline]
    pub(crate) fn new(
        request: Request,
        deferral: Arc<SgxMutex<Deferral>>,
        httpc: Arc<SgxMutex<HttpcReactor>>,
    ) -> Self {
        Self {
            request,
            deferral,
            httpc,
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

    pub fn subscribe(&self, future: impl Future<Output=()> + 'static + Send + Sync) -> Result<(), Error> {
        let future = Arc::new(SgxMutex::new(future.boxed()));

        return match self.deferral.lock() {
            Ok(mut deferral) => {
                deferral.defer(Arc::new(move |conn| {
                    conn.subscribe(Arc::clone(&future))
                }));

                Ok(())
            }
            Err(err) => {
                Err(Error::new_with_kind(
                    ErrorKind::ExecError,
                    format!("failed to acquire lock on 'deferral' during Context->subscribe: {:?}", err),
                ))
            }
        };
    }

    pub fn send(&self) {}

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
    pub fn insert<S>(&mut self, key: S, value: S) -> &mut Self
        where
            S: Into<String>,
    {
        let key = key.into();
        let value = value.into();

        self.data.insert(key, value);
        self
    }

    #[inline]
    pub fn get<R, S>(&self, key: S) -> Option<R>
        where
            R: FromStr,
            S: Into<String>,
    {
        let key = key.into();
        self.data.get(&key)?
            .parse()
            .ok()
    }

    #[inline]
    pub fn contains_key<S>(&mut self, key: S) -> bool
        where
            S: Into<String>,
    {
        let key = key.into();

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