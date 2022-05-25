use alloc::string::{String};
use alloc::sync::Arc;
use core::str::FromStr;
use mio_httpc::CallBuilder;
use std::collections::HashMap;
use std::sync::SgxMutex;
use crate::api::results::{Error, ErrorKind};
use crate::api::reactor::httpc::{HttpcCallFuture, HttpcReactor};

pub(crate) struct Context {
    data: HashMap<String, String>,
    httpc: Arc<SgxMutex<HttpcReactor>>
}

#[allow(dead_code)]
impl Context {
    #[inline]
    pub(crate) fn new(httpc: Arc<SgxMutex<HttpcReactor>>) -> Self {
        Self {
            data: HashMap::new(),
            httpc
        }
    }

    // DUMMY FOR NOW
    pub fn test(&self) -> HttpcCallFuture {
        match self.httpc.lock() {
            Ok(mut lock) => {
                let mut builder = CallBuilder::get();

                builder.timeout_ms(5000).host("172.17.0.1");

                /*
                  builder.timeout_ms(5000).https()
                    .host("catfact.ninja")
                    .path_segm("fact");

                  builder.timeout_ms(120000).https()
                    .host("gorest.co.in")
                    .path_segm("public")
                    .path_segm("v2")
                    .path_segm("posts")
                    .path_segm("1902");

                 */

                lock.call(builder)
            }
            Err(err) => {
                HttpcCallFuture::from_error(
                    Error::new_with_kind(ErrorKind::HttpClientError,
                    format!("failed to get lock on 'httpc' during Context->call: {:?}", err))
                )
            }
        }
    }

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