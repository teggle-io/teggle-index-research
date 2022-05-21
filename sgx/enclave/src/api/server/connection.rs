use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Add;

use log::{trace, warn};
use mio::event::Event;
use mio::net::TcpStream;
use rustls::Session;
use std::io;
use std::io::{ErrorKind as StdErrorKind, Read, Write};
use std::net::Shutdown;
use std::sync::{SgxMutex, SgxMutexGuard};
use std::time::Instant;

use crate::api::{
    handler::request::{process_raw_request, RawRequest},
    handler::response::Response,
    results::{Error, ErrorKind, ResponseBody},
    server::config::Config,
    server::exec::ExecManager,
};

pub(crate) static UPGRADE_OPT_KEEPALIVE: u8 = 2;

pub(crate) struct Connection {
    session: Arc<SgxMutex<TlsSession>>,
    exec: Arc<SgxMutex<ExecManager>>,
}

impl Connection {
    pub(crate) fn new(
        session: Arc<SgxMutex<TlsSession>>,
        exec: Arc<SgxMutex<ExecManager>>,
    ) -> Self {
        Self {
            session,
            exec,
        }
    }

    fn handle_request(&self, session: &mut SgxMutexGuard<TlsSession>, poll: &mut mio::Poll) {
        let config = session.config.clone();

        if session.is_closed() || session.is_closing() {
            return;
        }

        let mut bytes_read: usize = 0;
        if let Some(req) = session.request.as_ref() {
            bytes_read = req.len();
        }

        let mut request_body = Vec::new();
        let r = session.read(&mut request_body, bytes_read);
        if r == -1 {
            session.set_closing(true);
            return;
        }

        if request_body.len() > 0 {
            trace!("req body: {:?}", String::from_utf8(request_body.clone()));

            // Consume request body.
            if let Some(req) = &mut session.request {
                if let Err(err) = req.next(request_body) {
                    session.handle_error(&err, false);
                    return;
                }
            } else {
                match RawRequest::new(request_body,
                                      Instant::now()
                                          .add(config.request_timeout())) {
                    Ok(req) => {
                        session.request = Some(req);
                    }
                    Err(err) => {
                        session.handle_error(&err, false);
                        return;
                    }
                }
            }

            if session.request.is_some() {
                let mut upgrade_opts = 0_u8;
                if let Some(req) = session.request.as_ref() {
                    // Check payload size.
                    if let Some(content_len) = req.content_length() {
                        if content_len > config.max_bytes_received() {
                            session.handle_error(&too_many_bytes_err(
                                content_len,
                                config.max_bytes_received()), false);
                            return;
                        }
                    }

                    upgrade_opts = req.upgrade_opts();
                }

                // Upgrade connection.
                if upgrade_opts > 0 {
                    session.upgrade(upgrade_opts);
                }

                if let Some(req) = session.request.as_ref() {
                    // Ready?
                    if req.ready() {
                        let req = session.request.take().unwrap();

                        match self.exec.lock() {
                            Ok(mut exec) => {
                                let session = self.session.clone();

                                exec.spawn(poll, async move {
                                    match process_raw_request(req).await {
                                        Ok(res) => {
                                            match session.lock() {
                                                Ok(mut session) => {
                                                    session.send_response(res, true);
                                                }
                                                Err(err) => {
                                                    error!("failed to acquire lock on 'session' when \
                                                    handling process_raw_request->send_response: {:?}", err);
                                                }
                                            }
                                        }
                                        Err(err) => {
                                            match session.lock() {
                                                Ok(mut session) => {
                                                    session.handle_error(&err, true);
                                                }
                                                Err(nested_err) => {
                                                    error!("failed to acquire lock on 'session' when \
                                                    handling process_raw_request->handle_error: {:?}, \
                                                    origin err: {:?}", nested_err, err);
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                            Err(err) => {
                                session.handle_error(&Error::new_with_kind(
                                    ErrorKind::ExecError, err.to_string()),
                                                     false);
                                return;
                            }
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn ready(&mut self, poll: &mut mio::Poll, ev: &Event) {
        match self.session.lock() {
            Ok(mut session) => {
                if ev.readiness().is_readable() {
                    trace!("ready[{:?}]: READ", ev.token());
                    session.read_tls();
                    self.handle_request(&mut session, poll);
                }

                if ev.readiness().is_writable() {
                    trace!("ready[{:?}]: WRITE", ev.token());
                    session.write_tls_and_handle_error();
                }

                if session.is_closing() {
                    trace!("ready[{:?}]: CLOSE", ev.token());
                    session.close();
                    session.deregister(poll);
                } else {
                    trace!("ready[{:?}]: CONTINUE", ev.token());
                    session.reregister(poll);
                }
            }
            Err(err) => {
                error!("failed to acquire lock on 'session' when handling event: {:?}", err);
            }
        }
    }

    pub fn register(&mut self, poll: &mut mio::Poll) {
        match self.session.lock() {
            Ok(session) => {
                session.register(poll);
            }
            Err(err) => {
                error!("failed to acquire lock on 'session' when calling 'register': {:?}", err);
            }
        }
    }

    pub fn check_timeout(&mut self, poll: &mut mio::Poll, now: &Instant) {
        match self.session.lock() {
            Ok(mut session) => {
                session.check_timeout(poll, now);
            }
            Err(err) => {
                error!("failed to acquire lock on 'session' when calling 'check_timeout': {:?}", err);
            }
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        match self.session.lock() {
            Ok(session) => {
                return session.is_closed();
            }
            Err(err) => {
                error!("failed to acquire lock on 'session' when calling 'is_closed': {:?}", err);
            }
        }

        false
    }
}

pub(crate) struct TlsSession {
    token: mio::Token,
    socket: TcpStream,
    session: rustls::ServerSession,
    config: Arc<Config>,
    request: Option<RawRequest>,
    upgraded: bool,
    closing: bool,
    closed: bool,
}

impl TlsSession {
    pub(crate) fn new(
        token: mio::Token,
        socket: TcpStream,
        session: rustls::ServerSession,
        config: Arc<Config>,
    ) -> Self {
        Self {
            token,
            socket,
            session,
            config,
            request: None,
            upgraded: false,
            closing: false,
            closed: false,
        }
    }

    fn send_response(&mut self, res: ResponseBody, push: bool) {
        let body = res.body();

        if body.len() > 0 {
            trace!("res body: {:?}", String::from_utf8(body.clone()));
        }

        self.write(&body[..]);

        if res.close() {
            self.session.send_close_notify();
        }
        if push {
            self.write_tls_and_handle_error();
            if self.is_closing() {
                self.close();
            }
        }
    }

    fn handle_io_error(&mut self, err: io::Error) {
        if let Some(err) = err.into_inner() {
            let inner: Option<&Box<Error>> = err.as_ref().downcast_ref();
            if inner.is_some() {
                self.handle_error(inner.unwrap(), false);
            }
        }
    }

    fn handle_error(&mut self, err: &Error, push: bool) {
        warn!("failed to handle request: {}", err);
        self.request = None;

        match Response::from_error(err).encode() {
            Ok(res) => {
                self.send_response(res, push);
            }
            Err(err) => {
                warn!("failed to encode response while handling error: {:?}", err)
            }
        }
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>, bytes_read: usize) -> std::io::Result<usize> {
        read_to_end(&mut self.session, buf,
                    self.config.max_bytes_received(), bytes_read)
    }

    fn close(&mut self) {
        self.session.send_close_notify();
        let _ = self.socket.shutdown(Shutdown::Both);
        self.closed = true;
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

    fn read(&mut self, plaintext: &mut Vec<u8>, bytes_read: usize) -> isize {
        match self.read_to_end(plaintext, bytes_read) {
            Err(err) => {
                if let io::ErrorKind::ConnectionAborted = err.kind() {
                    self.handle_io_error(err);
                    return 0;
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
            return;
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

    pub(crate) fn is_closing(&self) -> bool {
        self.closing
    }

    pub(crate) fn set_closing(&mut self, closing: bool) {
        self.closing = closing;
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed
    }

    pub(crate) fn upgrade(&mut self, opts: u8) {
        if self.upgraded {
            return;
        }

        if opts & UPGRADE_OPT_KEEPALIVE > 0 {
            self.set_keepalive();
        }

        self.upgraded = true;
    }

    pub(crate) fn set_keepalive(&self) {
        match self.socket.set_keepalive(Some(self.config.keep_alive_time())) {
            Ok(_) => {}
            Err(e) => {
                warn!("failed to set keepalive during socket upgrade: {:?}", e);
                return;
            }
        }

        trace!("upgraded socket with keepalive: {:?}", self.config.keep_alive_time());
    }

    pub fn check_timeout(&mut self, poll: &mut mio::Poll, now: &Instant) {
        if let Some(req) = self.request.as_ref() {
            if req.check_timeout(now) {
                self.handle_error(
                    &Error::new_with_kind(
                        ErrorKind::TimedOut,
                        "request timed out".to_string(),
                    ), true,
                );
                self.write_tls_and_handle_error();
                self.close();
                self.deregister(poll);
            }
        }
    }
}


fn too_many_bytes_io_err(bytes: usize, max_bytes: usize) -> std::io::Error {
    std::io::Error::new(
        StdErrorKind::ConnectionAborted,
        Box::new(
            too_many_bytes_err(bytes, max_bytes)),
    )
}

fn too_many_bytes_err(bytes: usize, max_bytes: usize) -> Error {
    Error::new_with_kind(
        ErrorKind::PayloadTooLarge,
        format!("too many bytes sent ({} > {})",
                bytes, max_bytes).to_string())
}

// Copied from std::io::Read::read_to_end to retain performance

struct Guard<'a> {
    buf: &'a mut Vec<u8>,
    len: usize,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        unsafe {
            self.buf.set_len(self.len);
        }
    }
}

// This uses an adaptive system to extend the vector when it fills. We want to
// avoid paying to allocate and zero a huge chunk of memory if the reader only
// has 4 bytes while still making large reads if the reader does have a ton
// of data to return. Simply tacking on an extra DEFAULT_BUF_SIZE space every
// time is 4,500 times (!) slower than a default reservation size of 32 if the
// reader has a very small amount of data to return.
//
// Because we're extending the buffer with uninitialized data for trusted
// readers, we need to make sure to truncate that if any of this panics.
fn read_to_end<R: Read + ?Sized>(
    r: &mut R,
    buf: &mut Vec<u8>,
    max_bytes: usize,
    bytes_read: usize,
) -> std::io::Result<usize> {
    read_to_end_with_reservation(r, buf, |_| 32, max_bytes, bytes_read)
}

fn read_to_end_with_reservation<R, F>(
    r: &mut R,
    buf: &mut Vec<u8>,
    mut reservation_size: F,
    max_bytes: usize,
    bytes_read: usize,
) -> std::io::Result<usize>
    where
        R: Read + ?Sized,
        F: FnMut(&R) -> usize,
{
    let start_len = buf.len();
    let mut g = Guard { len: buf.len(), buf };
    let ret;
    loop {
        if (g.len + bytes_read) >= max_bytes {
            return Err(too_many_bytes_io_err(g.len + bytes_read, max_bytes));
        }
        if g.len == g.buf.len() {
            unsafe {
                // FIXME(danielhenrymantilla): #42788
                //
                //   - This creates a (mut) reference to a slice of
                //     _uninitialized_ integers, which is **undefined behavior**
                //
                //   - Only the standard library gets to soundly "ignore" this,
                //     based on its privileged knowledge of unstable rustc
                //     internals;
                g.buf.reserve(reservation_size(r));
                let capacity = g.buf.capacity();
                g.buf.set_len(capacity);
                r.initializer().initialize(&mut g.buf[g.len..]);
            }
        }

        match r.read(&mut g.buf[g.len..]) {
            Ok(0) => {
                ret = Ok(g.len - start_len);
                break;
            }
            Ok(n) => g.len += n,
            Err(ref e) if e.kind() == StdErrorKind::Interrupted => {}
            Err(e) => {
                ret = Err(e);
                break;
            }
        }
    }

    ret
}