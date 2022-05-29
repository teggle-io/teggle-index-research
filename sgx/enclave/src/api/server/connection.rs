use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;
use core::ops::Add;

use futures::future::BoxFuture;
use futures::FutureExt;
use log::{trace, warn};
use mio::event::{Event, Evented};
use mio::net::TcpStream;
use mio::Token;
use std::io;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::sync::SgxMutex;
use std::time::Instant;
use tungstenite::Message;

use crate::api::{
    handler::request::{process_raw_request, RawRequest},
    handler::response::Response,
    reactor::exec::ExecReactor,
    reactor::httpc::HttpcReactor,
    reactor::waker::ReactorWaker,
    results::{Error, ErrorKind, ResponseBody, too_many_bytes_err},
    server::config::Config,
};
use crate::api::handler::context::Context;
use crate::api::handler::request::process_ws_raw_request;
use crate::api::server::websocket::WebSocket;

pub(crate) struct Connection {
    token: mio::Token,
    socket: TcpStream,
    tls_conn: rustls::ServerConnection,
    config: Arc<Config>,
    deferral: Arc<SgxMutex<Deferral>>,
    exec: Arc<SgxMutex<ExecReactor>>,
    httpc: Arc<SgxMutex<HttpcReactor>>,
    request: Option<RawRequest>,
    closing: bool,
    closed: bool,
    close_notify_sent: bool,
    ws: Option<Arc<SgxMutex<WebSocket>>>,
}

impl Connection {
    pub(crate) fn new(
        conn_id: usize,
        socket: TcpStream,
        tls_conn: rustls::ServerConnection,
        config: Arc<Config>,
        exec: Arc<SgxMutex<ExecReactor>>,
        httpc: Arc<SgxMutex<HttpcReactor>>,
    ) -> Self {
        let deferral = Arc::new(SgxMutex::new(
            Deferral::new(
                Token(conn_id + 1),
                config.max_defers_queue(),
                config.max_futures_queue(),
            )));

        Self {
            token: Token(conn_id),
            socket,
            tls_conn,
            config,
            exec,
            httpc,
            deferral,
            request: None,
            closing: false,
            closed: false,
            close_notify_sent: false,
            ws: None,
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
                if self.is_websocket() {
                    self.handle_ws_request(poll);
                } else {
                    self.handle_request(poll);
                }
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
                        self.handle_error(&err);
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
            //trace!("req body: {:?}", String::from_utf8(request_body.clone()));

            // Consume request body.
            if let Some(req) = &mut self.request {
                if let Err(err) = req.next(request_body) {
                    self.handle_error(&err);
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
                        self.handle_error(&err);
                        return;
                    }
                }
            }

            if let Some(req) = self.request.take() {
                if let Err(err) = req.validate(config) {
                    self.handle_error(&err);
                    return;
                }

                // Ready?
                if req.ready() {
                    self.process_request(poll, req);
                    //self.send_mock_response();
                } else {
                    self.request = Some(req);
                }
            }
        }
    }

    #[inline]
    fn handle_ws_request(&mut self, _poll: &mut mio::Poll) {
        if let Ok(io_state) = self.tls_conn.process_new_packets() {
            if io_state.plaintext_bytes_to_read() > 0 {
                let err: Option<Error> = match self.ws.as_ref().unwrap().lock() {
                    Ok(mut ws) => {
                        let mut tls_stream =
                            mut_tls_stream(&mut self.tls_conn, &mut self.socket);
                        if let Err(e) = ws.handle(&mut tls_stream) {
                            Some(e)
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        Some(Error::new_with_kind(
                            ErrorKind::WSFault,
                            format!("failed to acquire lock on 'ws' \
                            during handle_ws_request: {:?}", e).to_string(),
                        ))
                    }
                };
                if let Some(err) = err {
                    self.handle_error(&err);
                }
            }
        }
    }

    #[inline]
    #[allow(dead_code)]
    fn send_mock_response(&mut self) {
        let response =
            b"HTTP/1.1 200 OK\r\nContent-Length: 68\r\n\r\nHello world from rustls tlsserverHello world from rustls tlsserver\r\n";

        self.write(&response[..]);

        self.write_tls_and_handle_error();
        if self.is_closing() {
            self.close();
        }
    }

    #[inline]
    fn process_request(&mut self, poll: &mut mio::Poll, req: RawRequest) {
        let deferral = self.deferral.clone();
        let httpc = self.httpc.clone();

        if let Err(err) = self.spawn(poll, async move {
            if req.is_upgrade_websocket() {
                process_ws_raw_request(deferral, httpc, req).await
            } else {
                process_raw_request(deferral, httpc, req).await
            }
        }) {
            self.handle_error(&err);
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

    pub(crate) fn send_response(&mut self, res: &ResponseBody) {
        if self.is_closed() {
            // Abort, stale connection.
            return;
        }

        let body = res.body();

        /*
        if body.len() > 0 {
            trace!("res body: {:?}", String::from_utf8(body.clone()));
        }
        */

        self.write(&body[..]);

        if res.close() {
            self.send_close_notify();
        }
    }

    pub fn check_timeout(&mut self, poll: &mut mio::Poll, now: &Instant) {
        if let Some(req) = self.request.as_ref() {
            if req.check_timeout(now) {
                self.handle_error(
                    &Error::new_with_kind(
                        ErrorKind::TimedOut,
                        "request timed out".to_string(),
                    ),
                );
                self.write_tls_and_handle_error();
                self.close();
                self.deregister(poll);
            }
        }
    }

    // Web Socket
    #[inline]
    pub(crate) fn is_websocket(&self) -> bool {
        self.ws.is_some()
    }

    #[inline]
    pub(crate) fn websocket(
        &mut self,
        websocket: Arc<SgxMutex<WebSocket>>,
        context: Context
    ) -> Result<(), Error> {
        self.ws = Some(websocket);

        return match self.ws.as_ref().unwrap().lock() {
            Ok(mut websocket) => {
                let mut tls_stream =
                    mut_tls_stream(&mut self.tls_conn, &mut self.socket);
                websocket.activate(&mut tls_stream, context)
            }
            Err(err) => {
                Err(Error::new_with_kind(
                    ErrorKind::WSFault,
                    format!("failed to acquire lock on 'ws' \
                    during preparation of websocket: {:?}", err).to_string(),
                ))
            }
        }
    }

    #[inline]
    pub(crate) fn ws_send(
        &mut self,
        msg: Message
    ) -> Result<(), Error> {
        return match self.ws.as_ref().unwrap().lock() {
            Ok(mut websocket) => {
                let mut tls_stream =
                    mut_tls_stream(&mut self.tls_conn, &mut self.socket);
                websocket.send_with_tls_stream(msg, &mut tls_stream)
            }
            Err(err) => {
                Err(Error::new_with_kind(
                    ErrorKind::WSFault,
                    format!("failed to acquire lock on 'ws' \
                    during ws_send: {:?}", err).to_string(),
                ))
            }
        }
    }

    // Tls Session Related
    #[inline]
    fn handle_io_error(&mut self, err: io::Error) {
        if let Some(err) = err.into_inner() {
            let inner: Option<&Box<Error>> = err.as_ref().downcast_ref();
            if inner.is_some() {
                self.handle_error(inner.unwrap());
            }
        }
    }

    #[inline]
    pub(crate) fn handle_error(&mut self, err: &Error) {
        if self.is_websocket() {
            if let ErrorKind::WSClosed = err.kind() {
                self.closing = true;
                return;
            }

            // Not a standard request.
            warn!("error occurred during websocket request: {}", err);
            return;
        }

        warn!("failed to handle request: {}", err);

        if self.is_closed() {
            // Abort early, stale connection.
            return;
        }

        match Response::from_error(err).encode() {
            Ok(res) => {
                self.send_response(&res);
            }
            Err(err) => {
                warn!("failed to encode response while handling error: {:?}", err)
            }
        }
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
            self.tls_conn.send_close_notify();
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
        let rd = self.tls_conn.wants_read();
        let wr = self.tls_conn.wants_write();

        if rd && wr {
            mio::Ready::readable() | mio::Ready::writable()
        } else if wr {
            mio::Ready::writable()
        } else {
            mio::Ready::readable()
        }
    }

    fn read(&mut self, plaintext: &mut Vec<u8>, bytes_read: usize) -> isize {
        if let Ok(io_state) = self.tls_conn.process_new_packets() {
            if io_state.plaintext_bytes_to_read() > 0 {
                if io_state.plaintext_bytes_to_read() + bytes_read > self.config.max_bytes_received() {
                    self.handle_error(&too_many_bytes_err(
                        io_state.plaintext_bytes_to_read() + bytes_read,
                        self.config.max_bytes_received()));
                    return 0;
                }

                plaintext.resize(io_state.plaintext_bytes_to_read(), 0u8);

                return match self.tls_conn.reader().read_exact(plaintext) {
                    Err(err) => {
                        if let io::ErrorKind::ConnectionAborted = err.kind() {
                            trace!("TLS plain read error: ConnectionAborted");
                            self.handle_io_error(err);
                            return 0;
                        }

                        warn!("plaintext read error: {:?}", err);
                        return -1;
                    }
                    Ok(_) => {
                        plaintext.len() as isize
                    }
                };
            }
        }

        0
    }

    fn read_tls(&mut self) {
        // Read some TLS data.
        match self.tls_conn.read_tls(&mut self.socket) {
            Err(err) => {
                if let io::ErrorKind::WouldBlock = err.kind() {
                    return;
                }
                if let io::ErrorKind::ConnectionAborted = err.kind() {
                    trace!("TLS read error: ConnectionAborted");
                    self.closing = true;
                    return;
                }

                warn!("TLS read error: {:?}", err);
                self.closing = true;
                return;
            }
            Ok(0) => {
                // EOF
                trace!("TLS read error: EOF");
                self.closing = true;
                return;
            }
            Ok(_) => {}
        };

        // Process newly-received TLS messages.
        if let Err(err) = self.tls_conn.process_new_packets() {
            warn!("TLS error: {:?}", err);

            // last gasp write to send any alerts
            self.write_tls_and_handle_error();

            self.closing = true;
            return;
        }
    }

    fn write(&mut self, plaintext: &[u8]) {
        match self.tls_conn.writer().write_all(plaintext) {
            Err(err) => {
                if let io::ErrorKind::ConnectionAborted = err.kind() {
                    trace!("TLS plain write error: ConnectionAborted");
                    self.closing = true;
                    return;
                }

                warn!("TLS plain write error: {:?}", err);
                self.closing = true;
            }
            Ok(_) => {}
        }
    }

    #[inline]
    fn write_tls(&mut self) -> io::Result<usize> {
        self.tls_conn
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
}

fn mut_tls_stream<'a>(
    conn: &'a mut rustls::ServerConnection,
    sock: &'a mut TcpStream
) -> rustls::Stream<'a, rustls::ServerConnection, TcpStream> {
    rustls::Stream { conn, sock }
}

pub(crate) struct Deferral {
    waker: ReactorWaker,
    defers: Vec<Box<dyn Send + Sync + for<'a> FnOnce(&'a mut Connection) -> Result<(), Error>>>,
    futures: Vec<BoxFuture<'static, ()>>,
    // Options
    max_defers_queue: Option<usize>,
    max_futures_queue: Option<usize>,
}

impl Deferral {
    fn new(
        waker_token: Token,
        max_defers_queue: Option<usize>,
        max_futures_queue: Option<usize>,
    ) -> Self {
        Self {
            waker: ReactorWaker::new(waker_token),
            defers: Vec::new(),
            futures: Vec::new(),
            max_defers_queue,
            max_futures_queue,
        }
    }

    #[inline]
    pub(crate) fn defer(
        &mut self,
        defer: Box<dyn Send + Sync + for<'a> FnOnce(&'a mut Connection) -> Result<(), Error>>,
    ) -> Result<(), Error> {
        if let Some(max_defers_queue) = self.max_defers_queue {
            if self.defers.len() >= max_defers_queue {
                return Err(Error::new_with_kind(
                    ErrorKind::ServerFault,
                    format!("unable to queue deferral, limit exceeded: {}", max_defers_queue).to_string(),
                ));
            }
        }

        self.defers.push(defer);
        if let Err(err) = self.waker.trigger() {
            warn!("Deferral->defer failed to trigger waker: {:?}", err)
        }

        Ok(())
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn spawn(&mut self, future: impl Future<Output=()> + 'static + Send) -> Result<(), Error> {
        if let Some(max_futures_queue) = self.max_futures_queue {
            if self.defers.len() >= max_futures_queue {
                return Err(Error::new_with_kind(
                    ErrorKind::ServerFault,
                    format!("unable to queue future, limit exceeded: {}", max_futures_queue).to_string(),
                ));
            }
        }

        self.futures.push(future.boxed());
        if let Err(err) = self.waker.trigger() {
            warn!("Deferral->spawn failed to trigger waker: {:?}", err)
        }

        Ok(())
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
        Vec<Box<dyn Send + Sync + for<'a> FnOnce(&'a mut Connection) -> Result<(), Error>>>,
        Vec<BoxFuture<'static, ()>>
    ) {
        // Clear the waker readiness state prior to removing pending items.
        if let Err(err) = self.waker.clear() {
            warn!("Deferral failed to clear waker: {:?}", err)
        }

        (std::mem::take(&mut self.defers), std::mem::take(&mut self.futures))
    }
}