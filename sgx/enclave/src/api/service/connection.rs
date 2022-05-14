use alloc::vec::Vec;

use log::{warn};
use mio::event::Event;
use mio::net::{TcpStream};
use rustls::Session;
use std::io;
use std::io::{Read, Write};
use std::net::Shutdown;
use api::handler::request::process_raw_request;

pub(crate) struct Connection {
    socket: TcpStream,
    session: rustls::ServerSession,
    token: mio::Token,
    closing: bool,
    closed: bool,
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
            closed: false,
        }
    }

    fn read_and_process_request(&mut self) {
        if self.closing || self.closed {
            return;
        }

        let mut request_body = Vec::new();
        let r = self.read(&mut request_body);
        if r == -1 {
            self.closing = true;
            return;
        }

        if request_body.len() > 0 {
            match process_raw_request(request_body) {
                Ok(res) => {
                    self.write(&res[..]);
                }
                Err(err) => {
                    warn!("ApiSession: failed to handle request: {:?}", err);
                    return;
                }
            }
        }
    }

    pub(crate) fn ready(&mut self, poll: &mut mio::Poll, ev: &Event) {
        if ev.readiness().is_readable() {
            self.read_tls();
            self.read_and_process_request();
        }

        if ev.readiness().is_writable() {
            self.write_tls_and_handle_error();
        }

        if self.closing {
            self.session.send_close_notify();
            let _ = self.socket.shutdown(Shutdown::Both);
            self.closed = true;
            self.deregister(poll);
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

    fn deregister(&self, poll: &mut mio::Poll) {
        poll.deregister(&self.socket)
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

    fn read(&mut self, plaintext: &mut Vec<u8>) -> isize {
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
        plaintext.len() as isize
    }

    fn read_tls(&mut self) {
        // Read some TLS data.
        match self.session.read_tls(&mut self.socket) {
            Err(err) => {
                if let io::ErrorKind::WouldBlock = err.kind() {
                    return;
                }

                warn!("API Connection: TLS read error: {:?}", err);
                self.closing = true;
                return;
            }
            Ok(0) => {
                // EOF
                self.closing = true;
                return;
            }
            Ok(_) => {}
        };

        // Process newly-received TLS messages.
        if let Err(err) = self.session.process_new_packets() {
            warn!("API Connection: TLS error: {:?}", err);

            // last gasp write to send any alerts
            self.write_tls_and_handle_error();

            self.closing = true;
            return ;
        }
    }

    fn write(&mut self, plaintext: &[u8]) -> isize {
        match self.session.write(plaintext) {
            Err(err) => {
                warn!("API Connection: TLS plain write error: {:?}", err);
                self.closing = true;

                -1
            }
            Ok(len) => len as isize
        }
    }

    fn write_tls(&mut self) -> io::Result<usize> {
        self.session
            .write_tls(&mut self.socket)
    }

    fn write_tls_and_handle_error(&mut self) {
        let rc = self.write_tls();
        if rc.is_err() {
            warn!("API Connection: TLS write failed {:?}", rc);
            self.closing = true;
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closing
    }
}