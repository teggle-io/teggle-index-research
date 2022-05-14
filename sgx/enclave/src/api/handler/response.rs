use alloc::string::{String, ToString};
use alloc::vec::Vec;
use bytes::BytesMut;
use http::{Response, StatusCode};
use log::warn;
use serde::Serialize;
use api::handler::codec::GLOBAL_CODEC;

pub(crate) type HttpResponse = Result<Response<Vec<u8>>, String>;

pub(crate) fn encode_response(res: Response<Vec<u8>>) -> Result<Vec<u8>, String> {
    let mut encoded = BytesMut::new();

    GLOBAL_CODEC.encode(res, &mut encoded)
        .map_err(|e| {
            e.to_string()
        })?;

    Ok(encoded.to_vec())
}

pub(crate) fn encode_response_server_fault() -> Result<Vec<u8>, String> {
    match error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Server Fault") {
        Ok(res) => encode_response(res),
        Err(e) => {
            warn!("api: failed to encode server fault response - {:?}", e);
            Err(e.to_string())
        }
    }
}

pub(crate) fn error_response(status: StatusCode, msg: &str) -> HttpResponse {
    json_response(status, &ErrorMsg { status: u16::from(status), message: msg.to_string() })
}

pub(crate) fn ok_response(msg: &str) -> HttpResponse {
    json_response(StatusCode::OK, &Msg { message: msg.to_string() })
}

pub(crate) fn json_response<T: ?Sized + Serialize>(status: StatusCode, data: &T) -> HttpResponse {
    match serde_json::to_vec(data) {
        Ok(res_body) => {
            match Response::builder()
                .status(status)
                .header("Content-Type", "application/json")
                .body(res_body) {
                Ok(res) => {
                    Ok(res)
                }
                Err(e) => {
                    Err(format!("failed to encode json response: {:?}", e))
                }
            }
        }
        Err(e) => {
            Err(format!("failed to serialise json response: {:?}", e))
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