use alloc::sync::Arc;
use alloc::vec::Vec;

use futures::future::BoxFuture;

use crate::api::handler::context::Context;
use crate::api::results::Error;

pub(crate) type SubscriptionHandler = Arc<dyn Send + Sync + for<'a> Fn(&'a mut Context) -> BoxFuture<'a, Result<(), Error>>>;
pub(crate) type SubscriptionHandlerFn = for<'a> fn(&'a mut Context) -> BoxFuture<'a, Result<(), Error>>;

pub(crate) struct WebSocket {
    subscriptions: Vec<SubscriptionHandler>,
}

impl WebSocket {
    #[inline]
    pub(crate) fn new() -> Self {
        Self { subscriptions: Vec::new() }
    }

    #[inline]
    pub(crate) fn subscribe(&mut self, handler: SubscriptionHandler) -> Result<(), Error> {
        self.subscriptions.push(handler);

        Ok(())
    }
}