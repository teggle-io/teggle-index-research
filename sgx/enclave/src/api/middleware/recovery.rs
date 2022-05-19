use std::panic;
use std::panic::AssertUnwindSafe;

use crate::api::handler::request::{Context, Request};
use crate::api::handler::response::Response;
use crate::api::handler::router::Handler;
use crate::api::results::Error;

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