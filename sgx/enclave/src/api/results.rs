use alloc::string::String;
use core::fmt::{Display, Formatter};
use alloc::vec::Vec;

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

#[derive(Debug)]
pub(crate) struct Error {
    message: String
}

impl Error {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {
    
}