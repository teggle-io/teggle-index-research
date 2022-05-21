use alloc::string::{String, ToString};
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
    // Encode fault.
    EncodeFault,
    // Decode fault.
    DecodeFault,
    // General fault.
    ServerFault,
    // Timed out.
    TimedOut,
    // Too big.
    PayloadTooLarge,
    // Exec Error.
    ExecError,
    // Exec Timed out.
    ExecTimedOut,
    // Http Client Error.
    HttpClientError,
    // Http Client Timed out.
    HttpClientTimedOut,
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            ErrorKind::EncodeFault => write!(f, "EncodeFault"),
            ErrorKind::DecodeFault => write!(f, "DecodeFault"),
            ErrorKind::ServerFault => write!(f, "ServerFault"),
            ErrorKind::TimedOut => write!(f, "TimedOut"),
            ErrorKind::PayloadTooLarge => write!(f, "PayloadTooLarge"),
            ErrorKind::ExecError => write!(f, "ExecError"),
            ErrorKind::ExecTimedOut => write!(f, "ExecTimedOut"),
            ErrorKind::HttpClientError => write!(f, "HttpClientError"),
            ErrorKind::HttpClientTimedOut => write!(f, "HttpClientTimedOut"),
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
        Self::new_with_kind(ErrorKind::ServerFault, message)
    }

    pub fn new_with_kind(kind: ErrorKind, message: String) -> Self {
        Self {
            kind,
            message,
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self.kind {
            ErrorKind::EncodeFault => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorKind::DecodeFault => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorKind::ServerFault => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorKind::TimedOut => StatusCode::REQUEST_TIMEOUT,
            ErrorKind::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            ErrorKind::ExecError => StatusCode::BAD_GATEWAY,
            ErrorKind::ExecTimedOut => StatusCode::GATEWAY_TIMEOUT,
            ErrorKind::HttpClientError => StatusCode::BAD_GATEWAY,
            ErrorKind::HttpClientTimedOut => StatusCode::GATEWAY_TIMEOUT,
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}[{}]: {}",
               self.kind, self.http_status().to_string(), self.message)
    }
}

impl std::error::Error for Error {

}
