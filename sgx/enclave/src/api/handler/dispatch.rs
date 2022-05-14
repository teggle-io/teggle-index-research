use http::{Request, StatusCode};

use api::handler::response::{error_response, HttpResponse, ok_response};

pub(crate) fn dispatch_request(req: Request<()>) -> HttpResponse {
    match req.uri().path() {
        "/ping" => {
            ok_response("PONG")
        }
        _ => {
            error_response(StatusCode::NOT_FOUND, "Not Found")
        }
    }
}
