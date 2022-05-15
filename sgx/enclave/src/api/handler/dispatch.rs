use http::{Request, StatusCode};

use api::handler::response::{error_response, ok_response};
use api::handler::types::ResponseResult;

pub(crate) fn dispatch_request(req: &Request<()>) -> ResponseResult {
    match req.uri().path() {
        "/ping" => {
            ok_response(Some(req), "PONG")
        }
        _ => {
            error_response(Some(req),StatusCode::NOT_FOUND, "Not Found")
        }
    }
}
