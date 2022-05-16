use alloc::vec::Vec;

use http::StatusCode;

use api::handler::request::Request;
use api::handler::response::{Response};
use api::handler::types::{ApiError};

type Handler = fn(&Request, &mut Response, Vec<(&str, &str)>) -> Result<(), ApiError>;

pub(crate) fn route_request(req: &Request, res: &mut Response) -> Result<(), ApiError> {
    let route = format!("{}{}", req.method(), req.uri().path());
    let mut routes: path_router::Tree<Handler> = path_router::Tree::new();
    routes.add("GET/ping", |_req, res, _captures| {
        res.ok("PONG");
        Ok(())
    });

    match routes.find(&route) {
        Some((handler, captures)) => {
            handler(&req, res, captures)
        },
        None => {
            res.error(StatusCode::NOT_FOUND, "Not Found");

            Ok(())
        }
    }
}
