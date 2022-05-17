use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::convert::TryFrom;

use bytes::BytesMut;
use http::{HeaderValue, StatusCode, Version};
use http::header::HeaderName;
use http::response::{Parts};
use serde::Serialize;

use api::handler::codec::GLOBAL_CODEC;
use api::handler::request::Request;
use api::results::{EncodedResponseResult, Error, ResponseBody};

pub(crate) struct Response {
    parts: Parts,
    body_bytes: Option<Vec<u8>>,
    close: bool,
}

impl Response {
    #[inline]
    pub(crate) fn new() -> Self {
        let mut parts = Parts::new();
        parts.status = StatusCode::OK;

        Self {
            parts,
            body_bytes: None,
            close: true,
        }
    }

    #[inline]
    pub(crate) fn from_request(req: &Request) -> Self {
        let mut res = Self::new();
        res.version(req.version());
        res.close = !req.should_keep_alive();
        res
    }

    #[inline]
    pub(crate) fn encode_fault() -> EncodedResponseResult {
        let mut res = Self::new();
        res.fault();
        res.encode()
    }

    #[inline]
    pub fn status<T>(&mut self, status: T) -> &mut Self
        where
            StatusCode: TryFrom<T>,
            <StatusCode as TryFrom<T>>::Error: Into<http::Error>,
    {
        self.parts.status = TryFrom::try_from(status).map_err(Into::into).unwrap();
        self
    }

    #[inline]
    pub fn version(&mut self, version: Version) -> &mut Self {
        self.parts.version = version;
        self
    }

    #[inline]
    pub fn header<K, V>(&mut self, key: K, value: V) -> &mut Self
        where
            HeaderName: TryFrom<K>,
            <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
            HeaderValue: TryFrom<V>,
            <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        let name = <HeaderName as TryFrom<K>>::try_from(key).map_err(Into::into).unwrap();
        let value = <HeaderValue as TryFrom<V>>::try_from(value).map_err(Into::into).unwrap();
        self.parts.headers.append(name, value);
        self
    }

    #[inline]
    pub fn body(&mut self, body: Vec<u8>) -> &mut Self {
        self.body_bytes = Some(body);
        self
    }

    #[inline]
    pub fn json<T: ?Sized + Serialize>(
        &mut self,
        data: &T,
    ) -> Result<(), serde_json::Error> {
        match serde_json::to_vec(data) {
            Ok(res_body) => {
                self.header(http::header::CONTENT_TYPE, "application/json")
                    .body(res_body);

                Ok(())
            }
            Err(e) => {
                Err(e)
            }
        }
    }

    pub fn ok(&mut self, msg: &str) -> &mut Self {
        self.json(&Msg { message: msg.to_string() }).unwrap();
        self.status(StatusCode::OK);
        self
    }

    pub fn error(&mut self, status: StatusCode, msg: &str) -> &mut Self {
        self.json(&ErrorMsg { status: u16::from(status), message: msg.to_string() }).unwrap();
        self.status(status);
        self
    }

    pub fn fault(&mut self) -> &mut Self {
        self.error(StatusCode::INTERNAL_SERVER_ERROR,
                   "Server Fault")
    }

    pub fn encode(self) -> EncodedResponseResult {
        match self.body_bytes {
            Some(body) => {
                let mut encoded = BytesMut::new();
                let res: http::Response<Vec<u8>> = http::Response::from_parts(self.parts, body);

                match GLOBAL_CODEC.encode(res, &mut encoded) {
                    Ok(_) => Ok(ResponseBody::new_with_close(encoded.to_vec(), self.close)),
                    Err(e) => Err(Error::new(e.to_string()))
                }
            }
            None => Err(Error::new(
                "encode called on response with no body".to_string()))
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ErrorMsg {
    status: u16,
    message: String,
}

#[derive(Serialize, Deserialize)]
struct Msg {
    message: String,
}
