use alloc::string::{String};
use core::fmt::{Display, Formatter};
use alloc::vec::Vec;
use http::StatusCode;

pub(crate) type EncodedResponseResult = Result<ResponseBody, Error>;

#[derive(Clone)]
pub(crate) struct ResponseBody {
    body: Vec<u8>,
    close: bool
}

impl ResponseBody {
    #[allow(dead_code)]
    pub fn new(body: Vec<u8>) -> Self {
        Self { body, close: false }
    }

    pub fn new_with_close(body: Vec<u8>, close: bool) -> Self {
        Self { body, close }
    }

    pub fn body(&self) -> &Vec<u8> {
        &self.body
    }

    pub fn close(&self) -> bool {
        self.close
    }
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum ErrorKind {
    // General fault.
    ServerFault,
    // Too big.
    PayloadTooLarge,
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            ErrorKind::ServerFault => write!(f, "ServerFault"),
            ErrorKind::PayloadTooLarge => write!(f, "PayloadTooLarge"),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Error {
    message: String,
    kind: ErrorKind,
}

impl Error {
    pub fn new(message: String) -> Self {
        Self::new_with_kind(message, ErrorKind::ServerFault)
    }

    pub fn new_with_kind(message: String, kind: ErrorKind) -> Self {
        Self {
            message,
            kind
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self.kind {
            ErrorKind::ServerFault => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorKind::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}[{}]: {}", self.kind, self.http_status(), self.message)
    }
}

impl std::error::Error for Error {

}
