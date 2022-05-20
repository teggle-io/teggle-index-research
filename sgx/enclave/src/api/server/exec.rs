use alloc::sync::Arc;
use core::future::Future;
use core::ops::Add;
use core::task::Context;
use core::time::Duration;

use futures::{FutureExt, poll};
use futures::future::BoxFuture;
use futures::task::{ArcWake, waker_ref};
use mio::{Poll, PollOpt, Ready, Registration, SetReadiness, Token};
use mio::event::{Event, Evented};
use std::collections::HashMap;
use std::sync::SgxMutex;
use std::time::Instant;
use crate::api::server::config::Config;

pub(crate) struct ExecManager {
    tasks: HashMap<Token, Arc<Task>>,
    config: Arc<Config>,
    offset: usize,
    next_id: usize,
}

impl ExecManager {
    pub(crate) fn new(offset: usize, config: Arc<Config>) -> Self {
        Self {
            tasks: HashMap::new(),
            config,
            offset,
            next_id: offset,
        }
    }

    pub(crate) fn spawn(&mut self, poll: &mut mio::Poll, future: impl Future<Output=()> + 'static + Send) {
        let future = future.boxed();
        let token = Token(self.next_id);

        if self.next_id + 1 >= (self.offset + u32::MAX as usize) {
            self.next_id = self.offset;
        } else {
            self.next_id += 1;
        }

        self.tasks.insert(token, Arc::new(Task::new(
            token, SgxMutex::new(Some(future)),
            Instant::now().add(self.config.exec_timeout())
        )));
        self.tasks.get_mut(&token)
            .unwrap()
            .register(poll, token,
                      Ready::readable(),
                      mio::PollOpt::level() | mio::PollOpt::oneshot())
            .unwrap();
    }

    pub(crate) fn handle_event(&mut self, poll: &mut mio::Poll, event: &Event) {
        let token = event.token();
        if self.tasks.contains_key(&token) {
            let task = self.tasks.remove(&token)
                .unwrap();

            let mut future_slot = task.future.lock().unwrap();
            if let Some(mut future) = future_slot.take() {
                let waker = waker_ref(&task);
                let context = &mut Context::from_waker(&*waker);
                if future.as_mut().poll(context).is_pending() {
                    *future_slot = Some(future);

                    self.tasks.insert(token.clone(), task.clone());
                    task.reregister(poll, token, Ready::readable(),
                                    mio::PollOpt::level() | mio::PollOpt::oneshot())
                        .unwrap();
                }
            }
        }
    }

    pub(crate) fn check_timeouts(&mut self, _poll: &mut mio::Poll) {
        // TODO:
    }
}

struct Task {
    token: mio::Token,
    future: SgxMutex<Option<BoxFuture<'static, ()>>>,
    registration: Registration,
    set_readiness: SetReadiness,
    deadline: Instant,
}

impl Task {
    fn new(
        token: mio::Token,
        future: SgxMutex<Option<BoxFuture<'static, ()>>>,
        deadline: Instant
    ) -> Self {
        let (registration, set_readiness) = Registration::new2();

        Self { token, future, registration, set_readiness, deadline }
    }
}

impl ArcWake for Task {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.set_readiness
            .set_readiness(Ready::readable())
            .unwrap();
    }
}

impl Evented for Task {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> std::io::Result<()> {
        self.registration.register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> std::io::Result<()> {
        self.registration.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> std::io::Result<()> {
        self.registration.deregister(poll)
    }
}