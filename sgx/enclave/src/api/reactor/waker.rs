use mio::{PollOpt, Ready, Registration, SetReadiness, Token};
use mio::event::Evented;
use std::io;

pub(crate) struct ReactorWaker {
    token: Token,
    registration: Registration,
    set_readiness: SetReadiness,
}

impl ReactorWaker {
    pub(crate) fn new(token: Token) -> Self {
        let (registration, set_readiness) = Registration::new2();

        Self { token, registration, set_readiness }
    }

    pub(crate) fn register(&self, poll: &mio::Poll) -> std::io::Result<()> {
        poll.register(self, self.token.clone(),
                      Ready::readable(), mio::PollOpt::level())
    }

    pub(crate) fn trigger(&self) -> io::Result<()> {
        self.set_readiness.set_readiness(Ready::readable())
    }

    pub(crate) fn clear(&self) -> io::Result<()> {
        self.set_readiness.set_readiness(Ready::empty())
    }

    pub(crate) fn token(&self) -> Token {
        self.token
    }
}

impl Evented for ReactorWaker {
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