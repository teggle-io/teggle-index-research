use alloc::sync::Arc;
use core::future::Future;
use core::ops::Add;
use core::task::Context;

use futures::{FutureExt};
use futures::future::BoxFuture;
use futures::task::{ArcWake, waker_ref};
use mio::{Poll, PollOpt, Ready, Registration, SetReadiness, Token};
use mio::event::{Evented};
use std::collections::HashMap;
use std::sync::SgxMutex;
use std::time::Instant;
use crate::api::server::config::Config;

pub(crate) struct ExecReactor {
    tasks: HashMap<Token, Arc<Task>>,
    config: Arc<Config>,
    offset: usize,
    next_id: usize,
}

impl ExecReactor {
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
        self.tasks.get_mut(&token).unwrap().start(poll, token.clone());

        trace!("spawn[{:?}]: SPAWNED", token.clone());
    }

    pub(crate) fn ready(&mut self, poll: &mut mio::Poll, token: mio::Token) {
        if !self.tasks.contains_key(&token) {
            trace!("ready[{:?}]: TASK MISSING", token);
        }

        let task = self.tasks.remove(&token)
            .unwrap();
        task.reset_readiness();

        let mut future_slot = task.future.lock().unwrap();
        match future_slot.take() {
            Some(mut future) => {
                let waker = waker_ref(&task);
                let context = &mut Context::from_waker(&*waker);
                if future.as_mut().poll(context).is_pending() {
                    trace!("ready[{:?}]: PENDING", token);
                    *future_slot = Some(future);

                    self.tasks.insert(token.clone(), task.clone());
                    task.reregister(poll, token, Ready::readable(),
                                    mio::PollOpt::level() | mio::PollOpt::oneshot())
                        .unwrap();
                } else {
                    trace!("ready[{:?}]: COMPLETE", token);
                }
            }
            None => {
                trace!("ready[{:?}]: NO FUTURE", token);
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

    fn start(&self, poll: &Poll, token: mio::Token) {
        self.register(poll, token, Ready::readable(),
                        mio::PollOpt::level() | mio::PollOpt::oneshot())
            .unwrap();
        self.set_ready();
    }

    fn set_ready(&self) {
        if let Err(err) = self.set_readiness.set_readiness(Ready::readable()) {
            warn!("Task->set_ready->set_readiness failed: {:?}", err);
        }
    }

    fn reset_readiness(&self) {
        if let Err(err) = self.set_readiness.set_readiness(Ready::empty()) {
            warn!("Task->reset_readiness failed: {:?}", err);
        }
    }
}

impl ArcWake for Task {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.set_ready();
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