use alloc::boxed::Box;
use alloc::string::String;

use futures::future::FutureExt;
use futures::future::BoxFuture;
use std::panic::AssertUnwindSafe;

use crate::api::handler::context::Context;
use crate::api::handler::response::Response;
use crate::api::handler::router::Handler;
use crate::api::results::Error;

pub(crate) fn middleware_recovery<'a>(
    ctx: &'a mut Context,
    res: &'a mut Response,
    next: Handler
) -> BoxFuture<'a, Result<(), Error>> {
    Box::pin(async move {
        match AssertUnwindSafe(next(ctx, res)).catch_unwind().await {
            Ok(r) => r,
            Err(err) => {
                let mut err_msg = "**UNKNOWN**";
                if let Some(err) = err.downcast_ref::<String>() {
                    err_msg = err;
                } else if let Some(err) = err.downcast_ref::<&'static str>() {
                    err_msg = err;
                }

                warn!("recovered from panic during request: {}", err_msg);
                res.fault()
            }
        }
    })
}