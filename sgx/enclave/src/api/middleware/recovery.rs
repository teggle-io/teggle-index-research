use std::panic;
use std::panic::AssertUnwindSafe;

use api::handler::request::Request;
use api::handler::response::Response;
use api::handler::router::Handler;
use api::handler::types::ApiError;

pub(crate) fn middleware_recovery(req: &Request, res: &mut Response, next: Handler) -> Result<(), ApiError> {
    match panic::catch_unwind(AssertUnwindSafe(|| {
        next(req, res)
    })) {
        Ok(r) => r,
        Err(_e) => {
            // TODO: How do you log the error?
            warn!("recovered from panic during request");
            res.fault();

            Ok(())
        }
    }
}