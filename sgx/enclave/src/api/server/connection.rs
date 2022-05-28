use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;

use futures::future::BoxFuture;
use futures::FutureExt;
use log::{trace, warn};
use mio::event::{Event, Evented};
use mio::net::TcpStream;
use mio::Token;
use rustls::Session;
use std::io;
use std::io::{ErrorKind as StdErrorKind, Read, Write};
use std::net::Shutdown;
use std::sync::SgxMutex;
use std::time::Instant;

use crate::api::{
    handler::request::{process_raw_request, RawRequest},
    handler::response::Response,
    reactor::exec::ExecReactor,
    reactor::httpc::HttpcReactor,
    reactor::waker::ReactorWaker,
    results::{Error, ErrorKind, ResponseBody, too_many_bytes_err},
    server::config::Config,
};
use crate::api::handler::context::SubscriptionHandler;

pub(crate) struct Connection {
    token: mio::Token,
    socket: TcpStream,
    session: rustls::ServerSession,
    config: Arc<Config>,
    deferral: Arc<SgxMutex<Deferral>>,
    exec: Arc<SgxMutex<ExecReactor>>,
    httpc: Arc<SgxMutex<HttpcReactor>>,
    request: Option<RawRequest>,
    upgraded: bool,
    closing: bool,
    closed: bool,
    close_notify_sent: bool,
}

impl Connection {
    pub(crate) fn new(
        conn_id: usize,
        socket: TcpStream,
        session: rustls::ServerSession,
        config: Arc<Config>,
        exec: Arc<SgxMutex<ExecReactor>>,
        httpc: Arc<SgxMutex<HttpcReactor>>,
    ) -> Self {
        Self {
            token: Token(conn_id),
            socket,
            session,
            config,
            exec,
            httpc,
            deferral: Arc::new(SgxMutex::new(Deferral::new(Token(conn_id + 1)))),
            request: None,
            upgraded: false,
            closing: false,
            closed: false,
            close_notify_sent: false,
        }
    }

    #[inline]
    pub(crate) fn ready(&mut self, poll: &mut mio::Poll, ev: &Event, is_wakeup: bool) {
        if is_wakeup {
            self.wake(poll);
        } else {
            if ev.readiness().is_readable() {
                trace!("ready[{:?}]: READ", self.token);
                self.read_tls();
                self.handle_request(poll);
            }
        }

        if ev.readiness().is_writable() {
            trace!("ready[{:?}]: WRITE", self.token);
            self.write_tls_and_handle_error();
        }

        if self.is_closing() {
            trace!("ready[{:?}]: CLOSE", self.token);
            self.close();
            self.deregister(poll);
        } else {
            trace!("ready[{:?}]: CONTINUE", self.token);

            self.reregister(poll);
        }
    }

    #[inline]
    fn wake(&mut self, poll: &mut mio::Poll) {
        let pending = match self.deferral.lock() {
            Ok(mut deferral) => {
                Some(deferral.take_pending())
            }
            Err(err) => {
                error!("failed to acquire lock on 'deferral' when waking: {:?}", err);
                None
            }
        };

        if let Some((deferrals, futures)) = pending {
            for defer in deferrals {
                trace!("wake[{:?}]: RUN", self.token);
                match defer(self) {
                    Ok(_) => {}
                    Err(err) => {
                        self.handle_error(&err, true);
                    }
                }
            }
            if futures.len() > 0 {
                match self.exec.lock() {
                    Ok(mut exec) => {
                        for future in futures {
                            trace!("wake[{:?}]: SPAWN", self.token);
                            exec.spawn_boxed(poll, future);
                        }
                    }
                    Err(err) => {
                        error!("failed to acquire lock on 'exec' when waking: {:?}", err);
                    }
                }
            }
        }
    }

    #[inline]
    fn handle_request(&mut self, poll: &mut mio::Poll) {
        let config = self.config.clone();

        if self.is_closed() || self.is_closing() {
            return;
        }

        let mut bytes_read: usize = 0;
        if let Some(req) = self.request.as_ref() {
            bytes_read = req.len();
        }

        let mut request_body = Vec::new();
        let r = self.read(&mut request_body, bytes_read);
        if r == -1 {
            self.set_closing(true);
            return;
        }

        if request_body.len() > 0 {
            let response =
                b"HTTP/1.1 200 OK\r\nContent-Length: 68\r\n\r\nHello world from rustls tlsserverHello world from rustls tlsserver\r\n";

            self.write(&response[..]);

            self.write_tls_and_handle_error();
            if self.is_closing() {
                self.close();
            }
            // END TESTING

            /*
            trace!("req body: {:?}", String::from_utf8(request_body.clone()));

            // Consume request body.
            if let Some(req) = &mut self.request {
                if let Err(err) = req.next(request_body) {
                    self.handle_error(&err, false);
                    return;
                }
            } else {
                match RawRequest::new(request_body,
                                      Instant::now()
                                          .add(config.request_timeout())) {
                    Ok(req) => {
                        self.request = Some(req);
                    }
                    Err(err) => {
                        self.handle_error(&err, false);
                        return;
                    }
                }
            }

            if let Some(req) = self.request.take() {
                if let Err(err) = req.validate(config) {
                    self.handle_error(&err, false);
                    return;
                }

                self.upgrade(&req);

                // Ready?
                if req.ready() {
                    self.process_request(poll, req);
                } else {
                    self.request = Some(req);
                }
            }

             */
        }
    }

    #[inline]
    fn process_request(&mut self, poll: &mut mio::Poll, req: RawRequest) {
        let deferral = self.deferral.clone();
        let httpc = self.httpc.clone();

        if let Err(err) = self.spawn(poll, async move {
            process_raw_request(deferral, httpc, req).await
        }) {
            self.handle_error(&err, true);
        }
    }

    // Spawn an async function.
    #[inline]
    fn spawn(&mut self, poll: &mut mio::Poll, future: impl Future<Output=()> + 'static + Send) -> Result<(), Error> {
        match self.exec.lock() {
            Ok(mut exec) => {
                exec.spawn(poll, future);
            }
            Err(err) => {
                return Err(
                    Error::new_with_kind(ErrorKind::ExecError, err.to_string())
                );
            }
        }

        Ok(())
    }

    // Web Socket
    #[inline]
    pub(crate) fn subscribe(&self, _handler: SubscriptionHandler) -> Result<(), Error>  {
        Ok(())
    }

    // Tls Session Related
    pub(crate) fn send_response(&mut self, res: &ResponseBody, push: bool) {
        if self.is_closed() {
            // Abort, stale connection.
            return;
        }

        let body = res.body();

        if body.len() > 0 {
            trace!("res body: {:?}", String::from_utf8(body.clone()));
        }

        self.write(&body[..]);

        if res.close() {
            self.send_close_notify();
        }
        if push {
            self.write_tls_and_handle_error();
            if self.is_closing() {
                self.close();
            }
        }
    }

    #[inline]
    fn handle_io_error(&mut self, err: io::Error) {
        if let Some(err) = err.into_inner() {
            let inner: Option<&Box<Error>> = err.as_ref().downcast_ref();
            if inner.is_some() {
                self.handle_error(inner.unwrap(), false);
            }
        }
    }

    #[inline]
    pub(crate) fn handle_error(&mut self, err: &Error, push: bool) {
        warn!("failed to handle request: {}", err);
        self.request = None;

        if self.is_closed() {
            // Abort early, stale connection.
            return;
        }

        match Response::from_error(err).encode() {
            Ok(res) => {
                self.send_response(&res, push);
            }
            Err(err) => {
                warn!("failed to encode response while handling error: {:?}", err)
            }
        }
    }

    #[inline]
    fn read_to_end(&mut self, buf: &mut Vec<u8>, bytes_read: usize) -> std::io::Result<usize> {
        read_to_end(&mut self.session, buf,
                    self.config.max_bytes_received(), bytes_read)
    }

    #[inline]
    fn close(&mut self) {
        self.send_close_notify();
        let _ = self.socket.shutdown(Shutdown::Both);
        self.closed = true;
    }

    #[inline]
    fn send_close_notify(&mut self) {
        if !self.close_notify_sent {
            self.session.send_close_notify();
            self.close_notify_sent = true;
        }
    }

    #[inline]
    pub(crate) fn register(&self, poll: &mut mio::Poll) {
        match self.deferral.lock() {
            Ok(deferral) => {
                if let Err(err) = deferral.register(poll) {
                    error!("failed to call register on 'deferral': {:?}", err);
                }
            }
            Err(err) => {
                error!("failed to acquire lock on 'deferral' during register: {:?}", err);
            }
        }

        poll.register(&self.socket,
                      self.token,
                      self.event_set(),
                      mio::PollOpt::level() | mio::PollOpt::oneshot())
            .unwrap();
    }

    #[inline]
    fn reregister(&self, poll: &mut mio::Poll) {
        poll.reregister(&self.socket,
                        self.token,
                        self.event_set(),
                        mio::PollOpt::level() | mio::PollOpt::oneshot())
            .unwrap();
    }

    #[inline]
    fn deregister(&self, poll: &mut mio::Poll) {
        poll.deregister(&self.socket)
            .unwrap();

        match self.deferral.lock() {
            Ok(deferral) => {
                if let Err(err) = deferral.deregister(poll) {
                    error!("failed to call deregister on 'deferral': {:?}", err);
                }
            }
            Err(err) => {
                error!("failed to acquire lock on 'deferral' during deregister: {:?}", err);
            }
        }
    }

    #[inline]
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

    #[inline]
    fn write_tls(&mut self) -> io::Result<usize> {
        self.session
            .write_tls(&mut self.socket)
    }

    #[inline]
    fn write_tls_and_handle_error(&mut self) {
        let rc = self.write_tls();
        if rc.is_err() {
            warn!("TLS write failed {:?}", rc);
            self.closing = true;
        }
    }

    #[inline]
    pub(crate) fn is_closing(&self) -> bool {
        self.closing
    }

    #[inline]
    pub(crate) fn set_closing(&mut self, closing: bool) {
        self.closing = closing;
    }

    #[inline]
    pub(crate) fn is_closed(&self) -> bool {
        self.closed
    }

    #[inline]
    pub(crate) fn upgrade(&mut self, req: &RawRequest) {
        if self.upgraded {
            return;
        }

        if req.is_upgrade_keepalive() {
            self.set_keepalive();
        }

        self.upgraded = true;
    }

    #[inline]
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

pub(crate) struct Deferral {
    waker: ReactorWaker,
    defers: Vec<Arc<dyn Send + Sync + for<'a> Fn(&'a mut Connection) -> Result<(), Error>>>,
    futures: Vec<BoxFuture<'static, ()>>,
}

impl Deferral {
    fn new(waker_token: Token) -> Self {
        Self {
            waker: ReactorWaker::new(waker_token),
            defers: Vec::new(),
            futures: Vec::new(),
        }
    }

    #[inline]
    pub(crate) fn defer(&mut self, defer: Arc<dyn Send + Sync + for<'a> Fn(&'a mut Connection) -> Result<(), Error>>) {
        self.defers.push(defer);
        if let Err(err) = self.waker.trigger() {
            warn!("Deferral->defer failed to trigger waker: {:?}", err)
        }
    }

    #[inline]
    pub(crate) fn spawn(&mut self, future: impl Future<Output=()> + 'static + Send) {
        self.futures.push(future.boxed());
        if let Err(err) = self.waker.trigger() {
            warn!("Deferral->spawn failed to trigger waker: {:?}", err)
        }
    }

    #[inline]
    pub(crate) fn register(&self, poll: &mut mio::Poll) -> std::io::Result<()> {
        self.waker.register(poll)
    }

    #[inline]
    pub(crate) fn deregister(&self, poll: &mut mio::Poll) -> std::io::Result<()> {
        self.waker.deregister(poll)
    }

    #[inline]
    fn take_pending(&mut self) -> (
        Vec<Arc<dyn Send + Sync + for<'a> Fn(&'a mut Connection) -> Result<(), Error>>>,
        Vec<BoxFuture<'static, ()>>
    ) {
        // Clear the waker readiness state prior to removing pending items.
        if let Err(err) = self.waker.clear() {
            warn!("Deferral failed to clear waker: {:?}", err)
        }

        (std::mem::take(&mut self.defers), std::mem::take(&mut self.futures))
    }
}

fn too_many_bytes_io_err(bytes: usize, max_bytes: usize) -> std::io::Error {
    std::io::Error::new(
        StdErrorKind::ConnectionAborted,
        Box::new(
            too_many_bytes_err(bytes, max_bytes)),
    )
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