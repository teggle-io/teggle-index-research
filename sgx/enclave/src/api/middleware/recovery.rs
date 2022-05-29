use alloc::boxed::Box;

use futures::future::BoxFuture;
use futures::future::FutureExt;
use std::panic::AssertUnwindSafe;

use crate::api::handler::context::Context;
use crate::api::handler::response::Response;
use crate::api::handler::router::Handler;
use crate::api::results::{caught_err_to_str, Error, ErrorKind};

pub(crate) fn middleware_recovery<'a>(
    ctx: &'a mut Context,
    res: &'a mut Response,
    next: Handler,
) -> BoxFuture<'a, Result<(), Error>> {
    Box::pin(async move {
        match AssertUnwindSafe(next(ctx, res)).catch_unwind().await {
            Ok(r) => r,
            Err(err) => {
                Err(Error::new_with_kind(
                    ErrorKind::ServerFault,
                    format!("recovered from panic during request: {}",
                            caught_err_to_str(err)),
                ))
            }
        }
    })
}