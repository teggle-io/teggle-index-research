use alloc::string::String;
use alloc::vec::Vec;

use log::{trace, warn};
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
            trace!("req body: {:?}", String::from_utf8(request_body.clone()));

            match process_raw_request(request_body) {
                Ok(res) => {
                    let body = res.body();

                    if body.len() > 0 {
                        trace!("res body: {:?}", String::from_utf8(body.clone()));
                    }

                    self.write(&body[..]);

                    if res.close() {
                        self.session.send_close_notify();
                    }
                }
                Err(err) => {
                    warn!("failed to handle request: {:?}", err);
                    self.closing = true;
                    return;
                }
            }
        }
    }

    pub(crate) fn ready(&mut self, poll: &mut mio::Poll, ev: &Event) {
        if ev.readiness().is_readable() {
            trace!("ready: READ");
            self.read_tls();
            self.read_and_process_request();
        }

        if ev.readiness().is_writable() {
            trace!("ready: WRITE");
            self.write_tls_and_handle_error();
        }

        if self.closing {
            trace!("ready: CLOSE");
            //self.session.send_close_notify();
            let _ = self.socket.shutdown(Shutdown::Both);
            self.closed = true;
            self.deregister(poll);
        } else {
            trace!("ready: CONTINUE");
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
        match self.session.read_to_end(plaintext) {
            Err(err) => {
                if let io::ErrorKind::ConnectionAborted = err.kind() {
                    self.closing = true;
                    return -1;
                }

                warn!("plaintext read error: {:?}", err);
                return -1;
            }
            Ok(_) => {
                plaintext.len() as isize
            }
        }
    }

    fn read_tls(&mut self) {
        // Read some TLS data.
        match self.session.read_tls(&mut self.socket) {
            Err(err) => {
                if let io::ErrorKind::WouldBlock = err.kind() {
                    return;
                }
                if let io::ErrorKind::ConnectionAborted = err.kind() {
                    self.closing = true;
                    return;
                }

                warn!("TLS read error: {:?}", err);
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
            warn!("TLS error: {:?}", err);

            // last gasp write to send any alerts
            self.write_tls_and_handle_error();

            self.closing = true;
            return ;
        }
    }

    fn write(&mut self, plaintext: &[u8]) -> isize {
        match self.session.write(plaintext) {
            Err(err) => {
                if let io::ErrorKind::ConnectionAborted = err.kind() {
                    self.closing = true;
                    return -1;
                }

                warn!("TLS plain write error: {:?}", err);
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
            warn!("TLS write failed {:?}", rc);
            self.closing = true;
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closing
    }
}