use alloc::string::{String, ToString};
use alloc::vec::Vec;

use bytes::BytesMut;
use http::{Request, Response, StatusCode};
use http::response::Builder;
use log::warn;
use serde::Serialize;

use api::handler::codec::GLOBAL_CODEC;
use api::handler::request::{should_keep_alive};
use api::handler::types::{ApiError, ResponseBody, ResponseBodyResult, ResponseResult};

pub(crate) fn encode_response(
    req: Option<&Request<()>>,
    res: Response<Vec<u8>>
) -> ResponseBodyResult {
    let mut close = false;
    if !should_keep_alive(req) {
        close = true;
    }

    encode_response_with_close(req, res, close)
}

pub(crate) fn encode_response_with_close(
    _req: Option<&Request<()>>,
    res: Response<Vec<u8>>,
    close: bool
) -> ResponseBodyResult {
    let mut encoded = BytesMut::new();

    match GLOBAL_CODEC.encode(res, &mut encoded) {
        Ok(_) => Ok(ResponseBody::new_with_close(encoded.to_vec(), close)),
        Err(e) => Err(ApiError::new(e.to_string()))
    }
}

pub(crate) fn encode_response_server_fault(req: Option<&Request<()>>) -> ResponseBodyResult {
    match error_response(req,
        StatusCode::INTERNAL_SERVER_ERROR,
        "Server Fault") {
        Ok(res) => encode_response_with_close(req, res, true),
        Err(e) => {
            warn!("api: failed to encode server fault response - {:?}", e);
            Err(e)
        }
    }
}

pub(crate) fn error_response(
    req: Option<&Request<()>>,
    status: StatusCode,
    msg: &str
) -> ResponseResult {
    json_response(req, status, &ErrorMsg { status: u16::from(status), message: msg.to_string() })
}

pub(crate) fn ok_response(
    req: Option<&Request<()>>,
    msg: &str
) -> ResponseResult {
    json_response(req, StatusCode::OK, &Msg { message: msg.to_string() })
}

pub(crate) fn json_response<T: ?Sized + Serialize>(
    req: Option<&Request<()>>,
    status: StatusCode,
    data: &T
) -> ResponseResult {
    match serde_json::to_vec(data) {
        Ok(res_body) => {
            match default_response(req)
                .status(status)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(res_body) {
                Ok(res) => {
                    Ok(res)
                }
                Err(e) => {
                    Err(ApiError::new(
                        format!("failed to encode json response: {:?}", e)
                    ))
                }
            }
        }
        Err(e) => {
            Err(ApiError::new(
                format!("failed to serialise json response: {:?}", e)
            ))
        }
    }
}

pub(crate) fn default_response(req: Option<&Request<()>>) -> Builder {
    let mut builder = Response::builder()
        .status(StatusCode::OK);

    if let Some(req) = req {
        builder = builder.version(req.version());
    }

    builder
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
