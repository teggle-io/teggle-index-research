use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::any::Any;
use core::fmt::{Display, Formatter};

use http::StatusCode;

pub(crate) type EncodedResponseResult = Result<ResponseBody, Error>;

#[derive(Clone)]
pub struct ResponseBody {
    body: Vec<u8>,
    close: bool,
}

impl ResponseBody {
    #[allow(dead_code)]
    pub fn new(body: Vec<u8>) -> Self {
        Self { body, close: false }
    }

    #[allow(dead_code)]
    pub fn dummy() -> Self {
        Self {
            body: b"HTTP/1.1 200 OK\r\nServer: index.teggle.io/v1beta1\r\nContent-Length: 18\r\nDate: TODO\r\ncontent-type: application/json\r\n\r\n{\"message\":\"PONG\"}".to_vec(),
            close: true,
        }
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
pub enum ErrorKind {
    // Encode fault.
    EncodeFault,
    // Decode fault.
    DecodeFault,
    // General fault.
    ServerFault,
    // Web Socket fault.
    WSFault,
    // Web Socket closed.
    WSClosed,
    // Timed out.
    TimedOut,
    // Too big.
    PayloadTooLarge,
    // Exec Error.
    ExecError,
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
            ErrorKind::WSFault => write!(f, "WSFault"),
            ErrorKind::WSClosed => write!(f, "WSClosed"),
            ErrorKind::TimedOut => write!(f, "TimedOut"),
            ErrorKind::PayloadTooLarge => write!(f, "PayloadTooLarge"),
            ErrorKind::ExecError => write!(f, "ExecError"),
            ErrorKind::HttpClientError => write!(f, "HttpClientError"),
            ErrorKind::HttpClientTimedOut => write!(f, "HttpClientTimedOut"),
        }
    }
}

#[derive(Debug)]
pub struct Error {
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

    pub fn new_ws_closed() -> Self {
        Self {
            kind: ErrorKind::WSClosed,
            message: "".to_string(),
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self.kind {
            ErrorKind::EncodeFault => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorKind::DecodeFault => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorKind::ServerFault => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorKind::WSFault => StatusCode::BAD_REQUEST,
            ErrorKind::WSClosed => StatusCode::IM_USED,
            ErrorKind::TimedOut => StatusCode::REQUEST_TIMEOUT,
            ErrorKind::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            ErrorKind::ExecError => StatusCode::BAD_GATEWAY,
            ErrorKind::HttpClientError => StatusCode::BAD_GATEWAY,
            ErrorKind::HttpClientTimedOut => StatusCode::GATEWAY_TIMEOUT,
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}[{}]: {}",
               self.kind, self.http_status().to_string(), self.message)
    }
}

impl std::error::Error for Error {}

pub(crate) fn too_many_bytes_err(bytes: usize, max_bytes: usize) -> Error {
    Error::new_with_kind(
        ErrorKind::PayloadTooLarge,
        format!("too many bytes sent ({} > {})",
                bytes, max_bytes).to_string())
}

pub(crate) fn caught_err_to_str(err: Box<dyn Any + Send>) -> String {
    let mut err_msg = "**UNKNOWN**";
    if let Some(err) = err.downcast_ref::<String>() {
        err_msg = err;
    } else if let Some(err) = err.downcast_ref::<&'static str>() {
        err_msg = err;
    }

    err_msg.to_string()
}