use alloc::string::{String, ToString};
use alloc::vec::Vec;
use http::{Response, StatusCode};
use serde::Serialize;

pub(crate) type HttpResponse = Result<Response<Vec<u8>>, String>;

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