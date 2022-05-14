use alloc::vec::Vec;

use log::warn;
use mio::event::Event;
use mio::net::{TcpStream};
use rustls::Session;
use sgx_types::*;
use std::io::{Read, Write};
use std::net::Shutdown;
use api::handler::process_raw_request;

pub enum HandleResult {
    EOF,
    Error,
    Continue,
    Close
}

pub(crate) struct Connection {
    socket: TcpStream,
    session: rustls::ServerSession,
    token: mio::Token,
    closing: bool,
}

impl Connection {
    pub(crate) fn new(socket: TcpStream,
           session: rustls::ServerSession,
           token: mio::Token)
           -> Self {
        Self {
            socket,
            session,
            token,
            closing: false,
        }
    }

    fn handle(&mut self) -> HandleResult {
        let r = self.read_tls();
        if r == -1 {
            return HandleResult::EOF;
        }

        let mut request_body = Vec::new();
        let r = self.read(&mut request_body);
        if r == -1 {
            return HandleResult::EOF;
        }

        let mut finalize = false;
        if request_body.len() > 0 {
            match process_raw_request(request_body) {
                Ok(res) => {
                    let r = self.write(&res[..]);
                    if r > 0 {
                        finalize = true
                    }
                }
                Err(err) => {
                    warn!("ApiSession: failed to handle request: {:?}", err);
                    return HandleResult::Error;
                }
            }
        }

        // Flush buffer (anything written will be sent now).
        self.write_tls();

        if finalize {
            return HandleResult::Close
        }

        HandleResult::Continue
    }

    fn do_handle(&mut self) {
        match self.handle() {
            HandleResult::Continue => {}
            _ => {
                self.closing = true
            }
        }
    }

    pub(crate) fn ready(&mut self, poll: &mut mio::Poll, ev: &Event) {
        if ev.readiness().is_readable() {
            self.do_handle();
        }

        if self.closing {
            self.session.send_close_notify();
            let _ = self.socket.shutdown(Shutdown::Both);
        } else {
            self.reregister(poll);
        }
    }

    pub(crate) fn register(&self, poll: &mut mio::Poll) {
        poll.register(&self.socket,
                      self.token,
                      self.event_set(),
                      mio::PollOpt::level() | mio::PollOpt::oneshot())
            .unwrap();
    }

    fn reregister(&self, poll: &mut mio::Poll) {
        poll.reregister(&self.socket,
                        self.token,
                        self.event_set(),
                        mio::PollOpt::level() | mio::PollOpt::oneshot())
            .unwrap();
    }

    fn event_set(&self) -> mio::Ready {
        let rd = self.session.wants_read();
        let wr = self.session.wants_write();

        if rd && wr {
            mio::Ready::readable() | mio::Ready::writable()
        } else if wr {
            mio::Ready::writable()
        } else {
            mio::Ready::readable()
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closing
    }

    fn read_tls(&mut self) -> c_int {
        // Read TLS data.  This fails if the underlying TCP connection
        // is broken.
        let rc = self.session.read_tls(&mut self.socket);
        if rc.is_err() {
            warn!("API Connection: TLS read error: {:?}", rc);
            return -1;
        }

        // If we're ready but there's no data: EOF.
        if rc.unwrap() == 0 {
            // EOF.
            return -1;
        }

        // Reading some TLS data might have yielded new TLS
        // messages to process.  Errors from this indicate
        // TLS protocol problems and are fatal.
        let processed = self.session.process_new_packets();
        if processed.is_err() {
            warn!("API Connection: TLS error: {:?}", processed.unwrap_err());
            return -1;
        }
        return 0;
    }

    fn read(&mut self, plaintext: &mut Vec<u8>) -> c_int {
        // Having read some TLS data, and processed any new messages,
        // we might have new plaintext as a result.
        //
        // Read it and then write it to stdout.
        let rc = self.session.read_to_end(plaintext);

        // If that fails, the peer might have started a clean TLS-level
        // session closure.
        if rc.is_err() {
            let err = rc.unwrap_err();
            warn!("API Connection: Plaintext read error: {:?}", err);
            return -1;
        }
        plaintext.len() as c_int
    }

    fn write(&mut self, plaintext: &[u8]) -> c_int{
        self.session.write(plaintext).unwrap() as c_int
    }

    fn write_tls(&mut self) {
        self.session.write_tls(&mut self.socket).unwrap();
    }
}