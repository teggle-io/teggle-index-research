use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use mio::event::Event;
use mio_httpc::{CallBuilder, CallRef, Httpc, HttpcCfg, Response, SimpleCall};
use std::collections::HashMap;
use std::sync::SgxMutex;

use crate::api::results::{Error, ErrorKind};

pub(crate) struct HttpcManager {
    httpc: Httpc,
    calls: HashMap<CallRef, Arc<SgxMutex<HttpcCall>>>,
}

impl HttpcManager {
    pub(crate) fn new(con_offset: usize, cfg: Option<HttpcCfg>) -> Self {
        Self {
            httpc: Httpc::new(con_offset, cfg),
            calls: HashMap::new(),
        }
    }

    pub(crate) fn call(&mut self, builder: &mut CallBuilder, poll: &mut mio::Poll) -> HttpcCallFuture {
        let (call, cref) = {
            match builder.simple_call(&mut self.httpc, poll) {
                Ok(call) => {
                    let cref  = call.call().get_ref().clone();

                    (HttpcCall::new(call), Some(cref))
                }
                Err(err) => {
                    (HttpcCall::new_for_error(
                        Error::new_with_kind(ErrorKind::HttpClientError,
                                             format!("failed to construct simple call: {:?}", err))
                    ), None)
                }
            }
        };

        let carc = Arc::new(SgxMutex::new(call));
        if let Some(cref) = cref {
            self.calls.insert(cref, carc.clone());
        }

        HttpcCallFuture::new(carc)
    }

    pub(crate) fn handle_event(&mut self, poll: &mut mio::Poll, event: &Event) {
        if let Some(cref) = self.httpc.event(&event) {
            if self.calls.contains_key(&cref) {
                if self.calls.get_mut(&cref)
                    .unwrap()
                    .lock()
                    .unwrap()
                    .ready(&mut self.httpc, poll) {
                    // Remove finished call.
                    self.calls.remove(&cref);
                }
            }
        }
    }

    pub(crate) fn check_timeouts(&mut self, _poll: &mut mio::Poll) {
        for cref in self.httpc.timeout().into_iter() {
            if self.calls.contains_key(&cref) {
                self.calls.remove(&cref)
                    .unwrap()
                    .lock()
                    .unwrap()
                    .abort(&mut self.httpc);
            }
        }
    }
}

pub(crate) struct HttpcCall {
    call: Option<SimpleCall>,
    err: Option<Error>,
    waker: Option<Waker>,
}

impl HttpcCall {
    pub(crate) fn new(call: SimpleCall) -> Self {
        Self { call: Some(call), err: None, waker: None }
    }

    pub(crate) fn new_for_error(err: Error) -> Self {
        Self { call: None, err: Some(err), waker: None }
    }

    pub(crate) fn ready(&mut self, htp: &mut Httpc, poll: &mut mio::Poll) -> bool {
        let mut completed = true;

        if let Some(call) = self.call.as_mut() {
            match call.perform(htp, poll) {
                Ok(true) => {
                    // Handled by Future.
                }
                Ok(false) => {
                    completed = false
                },
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

    pub(crate) fn abort(&mut self, htp: &mut Httpc) {
        if self.err.is_none() {
            self.err = Some(
                Error::new_with_kind(ErrorKind::HttpClientTimedOut,
                                     "HTTP request aborted/timed out".to_string()));
        }

        if self.call.is_some() {
            self.call.take().unwrap().abort(htp);
        }
    }
}

#[derive(Clone)]
pub(crate) struct HttpcCallFuture {
    state: Arc<SgxMutex<HttpcCall>>
}

impl HttpcCallFuture {
    fn new(state: Arc<SgxMutex<HttpcCall>>) -> Self {
        Self { state }
    }
}

impl Future for HttpcCallFuture {
    type Output = Result<Option<(Response, Vec<u8>)>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().unwrap();

        if let Some(err) = state.err.take() {
            return Poll::Ready(Err(err));
        } else if let Some(call) = state.call.as_ref() {
            if call.is_done() {
                return Poll::Ready(Ok(state.call.take().unwrap().finish()));
            }
        }

        state.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}