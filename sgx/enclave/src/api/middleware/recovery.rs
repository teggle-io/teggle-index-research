use std::panic;
use std::panic::AssertUnwindSafe;

use api::handler::request::{Context, Request};
use api::handler::response::Response;
use api::handler::router::Handler;
use api::results::Error;

pub(crate) fn middleware_recovery(
    req: &Request,
    res: &mut Response,
    ctx: &mut Context,
    next: Handler
) -> Result<(), Error> {
    match panic::catch_unwind(AssertUnwindSafe(|| {
        next(req, res, ctx)
    })) {
        Ok(r) => r,
        Err(_e) => {
            // TODO: How do you log the error?
            warn!("recovered from panic during request");

            res.fault()
        }
    }
}