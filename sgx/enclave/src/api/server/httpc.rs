use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

use mio::{PollOpt, Ready, Registration, SetReadiness, Token};
use mio::event::{Event, Evented};
use mio_httpc::{CallBuilder, CallRef, Httpc, HttpcCfg, Response, SimpleCall};
use std::collections::HashMap;
use std::io;
use std::sync::SgxMutex;

use crate::api::results::{Error, ErrorKind};

pub(crate) struct HttpcReactor {
    httpc: Httpc,
    calls: HashMap<CallRef, Arc<SgxMutex<HttpcCall>>>,
    pending: Vec<Arc<SgxMutex<HttpcCall>>>,
    waker: HttpcReactorWaker,
}

impl HttpcReactor {
    pub(crate) fn new(
        con_offset: usize,
        cfg: Option<HttpcCfg>,
    ) -> Self {
        Self {
            httpc: Httpc::new(con_offset + 1, cfg),
            calls: HashMap::new(),
            waker: HttpcReactorWaker::new(Token(con_offset)),
            pending: Vec::new(),
        }
    }

    pub(crate) fn register(&mut self, poll: &mut mio::Poll) -> std::io::Result<()> {
        self.waker.register(poll)
    }

    pub(crate) fn call(&mut self, builder: CallBuilder) -> HttpcCallFuture {
        let call = Arc::new(SgxMutex::new(
            HttpcCall::new(builder)
        ));

        self.pending.push(call.clone());
        if let Err(err) = self.waker.trigger() {
            warn!("HTTPC reactor failed to trigger waker: {:?}", err)
        }

        HttpcCallFuture::new(call)
    }

    pub(crate) fn handle_event(&mut self, poll: &mut mio::Poll, event: &Event) {
        let token = event.token();
        if token.eq(&self.waker.token) {
            trace!("handle_event[{:?}]: CHECK PENDING", token.clone());

            // Clear the waker readiness state prior to removing pending items.
            if let Err(err) = self.waker.clear() {
                warn!("HTTPC reactor failed to clear waker: {:?}", err)
            }

            let pending = std::mem::take(&mut self.pending);
            for p in pending {
                trace!("handle_event[{:?}]: SPAWN", token.clone());
                self.spawn(poll, p);
            }
        } else {
            if let Some(cref) = self.httpc.event(&event) {
                if self.calls.contains_key(&cref) {
                    trace!("handle_event[{:?}]: READY", token.clone());

                    if self.calls.get_mut(&cref)
                        .unwrap()
                        .lock()
                        .unwrap()
                        .ready(&mut self.httpc, poll) {
                        // Remove finished call.
                        trace!("handle_event[{:?}]: REMOVED", token.clone());
                        self.calls.remove(&cref);
                    }

                    return;
                }
            }
        }
    }

    pub(crate) fn check_timeouts(&mut self, _poll: &mut mio::Poll) {
        for cref in self.httpc.timeout().into_iter() {
            trace!("check_timeouts: time out for {:?}", cref);

            if self.calls.contains_key(&cref) {
                self.calls.remove(&cref)
                    .unwrap()
                    .lock()
                    .unwrap()
                    .abort(&mut self.httpc);
            }
        }
    }

    // private
    fn spawn(&mut self, poll: &mut mio::Poll, call: Arc<SgxMutex<HttpcCall>>) {
        match call.lock() {
            Ok(mut lock) => {
                if lock.err.is_some() {
                    return;
                }
                if lock.builder.is_none() {
                    lock.err = Some(
                        Error::new_with_kind(ErrorKind::HttpClientError,
                                             format!("failed to spawn HTTPC call, missing builder"))
                    );
                    return;
                }

                match lock.builder
                    .take()
                    .unwrap()
                    .simple_call(&mut self.httpc, poll) {
                    Ok(inner_call) => {
                        let cref = inner_call.call().get_ref().clone();
                        lock.call = Some(inner_call);

                        self.calls.insert(cref, call.clone());
                    }
                    Err(err) => {
                        lock.err = Some(
                            Error::new_with_kind(ErrorKind::HttpClientError,
                                                 format!("failed to construct simple call: {:?}", err))
                        );
                    }
                }
            }
            Err(err) => {
                error!("failed to lock pending call for spawn, dropped: {:?}", err)
            }
        }
    }
}

struct HttpcReactorWaker {
    token: Token,
    registration: Registration,
    set_readiness: SetReadiness,
}

impl HttpcReactorWaker {
    fn new(token: Token) -> Self {
        let (registration, set_readiness) = Registration::new2();

        Self { token, registration, set_readiness }
    }

    fn register(&self, poll: &mio::Poll) -> std::io::Result<()> {
        poll.register(self, self.token.clone(),
                      Ready::readable(), mio::PollOpt::level())
    }

    fn trigger(&self) -> io::Result<()> {
        self.set_readiness.set_readiness(Ready::readable())
    }

    fn clear(&self) -> io::Result<()> {
        self.set_readiness.set_readiness(Ready::empty())
    }
}

impl Evented for HttpcReactorWaker {
    fn register(&self, poll: &mio::Poll, token: Token, interest: Ready, opts: PollOpt) -> std::io::Result<()> {
        self.registration.register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &mio::Poll, token: Token, interest: Ready, opts: PollOpt) -> std::io::Result<()> {
        self.registration.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> std::io::Result<()> {
        self.registration.deregister(poll)
    }
}

pub(crate) struct HttpcCall {
    builder: Option<CallBuilder>,
    call: Option<SimpleCall>,
    err: Option<Error>,
    waker: Option<Waker>,
}

impl HttpcCall {
    fn new(builder: CallBuilder) -> Self {
        Self {
            builder: Some(builder),
            call: None,
            err: None,
            waker: None,
        }
    }

    pub(crate) fn from_error(err: Error) -> Self {
        Self {
            builder: None,
            call: None,
            err: Some(err),
            waker: None,
        }
    }

    fn ready(&mut self, htp: &mut Httpc, poll: &mut mio::Poll) -> bool {
        let mut completed = true;
        if let Some(call) = self.call.as_mut() {
            match call.perform(htp, poll) {
                Ok(true) => {
                    // Handled by Future.
                }
                Ok(false) => {
                    completed = false
                }
                Err(err) => {
                    self.err = Some(
                        Error::new_with_kind(ErrorKind::HttpClientError,
                                             format!("failed to perform HTTP request: {:?}", err)));
                }
            }
        }

        if completed {
            if let Some(waker) = self.waker.take() {
                waker.wake();
            }
        }

        completed
    }

    fn abort(&mut self, htp: &mut Httpc) {
        if self.err.is_none() {
            self.err = Some(
                Error::new_with_kind(ErrorKind::HttpClientTimedOut,
                                     "HTTP request aborted/timed out".to_string()));
        }

        if self.call.is_some() {
            self.call.take().unwrap().abort(htp);
        }

        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

#[derive(Clone)]
pub(crate) struct HttpcCallFuture {
    state: Arc<SgxMutex<HttpcCall>>,
}

impl HttpcCallFuture {
    fn new(state: Arc<SgxMutex<HttpcCall>>) -> Self {
        Self { state }
    }

    pub(crate) fn from_error(err: Error) -> Self {
        Self {
            state: Arc::new(SgxMutex::new(
                HttpcCall::from_error(err)
            ))
        }
    }
}

impl Future for HttpcCallFuture {
    type Output = Result<Option<(Response, Vec<u8>)>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().unwrap();

        if let Some(err) = state.err.take() {
            return Poll::Ready(Err(err));
        }
        if state.builder.is_none() {
            if let Some(call) = state.call.as_ref() {
                if call.is_done() {
                    return Poll::Ready(Ok(state.call.take().unwrap().finish()));
                }
            }
        }

        state.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}