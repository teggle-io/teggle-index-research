use alloc::string::String;
use alloc::vec::Vec;
use http::Response;

pub(crate) type ResponseResult = Result<Response<Vec<u8>>, ApiError>;
pub(crate) type ResponseBodyResult = Result<ResponseBody, ApiError>;

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
pub(crate) struct ApiError {
    message: String
}

impl ApiError {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}