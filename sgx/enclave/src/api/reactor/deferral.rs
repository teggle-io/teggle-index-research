use alloc::sync::Arc;
use alloc::vec::Vec;

use mio::Token;

use crate::api::reactor::waker::ReactorWaker;
use crate::api::results::Error;
use crate::api::server::connection::Connection;

pub(crate) struct DeferralReactor {
    pending: Vec<(Token, Arc<dyn Send + Sync + for<'a> Fn(&'a mut Connection) -> Result<(), Error>>)>,
    waker: ReactorWaker,
}

impl DeferralReactor {
    pub(crate) fn new(waker_token: Token) -> Self {
        Self {
            waker: ReactorWaker::new(waker_token),
            pending: Vec::new(),
        }
    }

    pub(crate) fn defer(&mut self, conn_id: Token, defer: Arc<dyn Send + Sync + for<'a> Fn(&'a mut Connection) -> Result<(), Error>>) {
        self.pending.push((conn_id, defer));
        if let Err(err) = self.waker.trigger() {
            warn!("DeferralReactor failed to trigger waker: {:?}", err)
        }
    }

    pub(crate) fn register(&mut self, poll: &mut mio::Poll) -> std::io::Result<()> {
        self.waker.register(poll)
    }

    pub(crate) fn take_pending(&mut self) -> Vec<(Token, Arc<dyn Send + Sync + for<'a> Fn(&'a mut Connection) -> Result<(), Error>>)> {
        trace!("handle_event[{:?}]: CHECK PENDING", self.waker.token());

        // Clear the waker readiness state prior to removing pending items.
        if let Err(err) = self.waker.clear() {
            warn!("DeferralReactor failed to clear waker: {:?}", err)
        }

        std::mem::take(&mut self.pending)
    }
}