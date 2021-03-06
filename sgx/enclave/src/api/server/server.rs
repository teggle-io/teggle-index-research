use alloc::sync::Arc;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;

use mio::event::Event;
use mio::net::TcpListener;
use mio::Token;
use net2::TcpBuilder;
use net2::unix::UnixTcpBuilderExt;
use std::collections::HashMap;
use std::sync::SgxMutex;
use std::time::Instant;

use crate::api::reactor::exec::ExecReactor;
use crate::api::reactor::httpc::HttpcReactor;
use crate::api::server::config::Config;
use crate::api::server::connection::Connection;

lazy_static!(
    pub static ref SERVER_ID_SEQ: AtomicUsize = AtomicUsize::new(0);
);

const LISTENER_TOKEN: Token = Token(0);

// 50 Kb
const MAX_BYTES_RECEIVED: usize = 50 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
// Currently the exec deadlines cannot be surfaced to the future
// their main purpose is to release some system resources.
const EXEC_TIMEOUT: Duration = Duration::from_secs(7200);

// Per connection.
const DEFERRAL_BACKLOG: usize = 100;
const FUTURE_BACKLOG: usize = 100;

const TCP_BACKLOG: i32 = 250;

const MIO_EVENTS_CAPACITY: usize = 2048;
const MIO_TIMEOUT_POLL: Duration = Duration::from_millis(1000);

const MIO_SERVER_OFFSET: usize = 10;
const MIO_EXEC_OFFSET: usize = MIO_SERVER_OFFSET + u32::MAX as usize;
const MIO_HTTPC_OFFSET: usize = MIO_EXEC_OFFSET + u32::MAX as usize;

pub(crate) struct Server {
    id: usize,
    server: TcpListener,
    connections: HashMap<usize, Connection>,
    config: Arc<Config>,
    exec: Arc<SgxMutex<ExecReactor>>,
    httpc: Arc<SgxMutex<HttpcReactor>>,
    next_id: usize,
    last_timeout: Instant,
}

impl Server {
    fn new(server: TcpListener, config: Arc<Config>) -> Self {
        let exec = Arc::new(
            SgxMutex::new(ExecReactor::new(MIO_EXEC_OFFSET, config.clone())));
        let httpc = Arc::new(
            SgxMutex::new(HttpcReactor::new(
                MIO_HTTPC_OFFSET, None)));

        Self {
            id: SERVER_ID_SEQ.fetch_add(1, Ordering::SeqCst),
            server,
            connections: HashMap::new(),
            config: config.clone(),
            exec,
            httpc,
            next_id: MIO_SERVER_OFFSET,
            last_timeout: Instant::now(),
        }
    }

    pub(crate) fn register(&mut self, poll: &mut mio::Poll) -> std::io::Result<()> {
        match self.httpc.lock() {
            Ok(mut httpc) => {
                httpc.register(poll)?;
            }
            Err(err) => {
                warn!("failed to acquire lock on 'httpc' during server->register: {:?}", err);
            }
        }

        poll.register(&self.server,
                      LISTENER_TOKEN,
                      mio::Ready::readable(),
                      mio::PollOpt::level()).unwrap();

        Ok(())
    }

    fn accept(&mut self, poll: &mut mio::Poll) {
        match self.server.accept() {
            Ok((socket, addr)) => {
                debug!("[{}] accepted connection: {}", self.id, addr);

                let tls_conn = rustls::ServerConnection::new(
                    Arc::clone(&self.config.tls_config())).unwrap();

                let conn_id = self.next_id;

                if self.next_id + 2 >= (MIO_SERVER_OFFSET + u32::MAX as usize) {
                    self.next_id = MIO_SERVER_OFFSET;
                } else {
                    self.next_id += 2;
                }

                self.connections.insert(conn_id, Connection::new(conn_id,
                                                                 socket, tls_conn,
                                                                 self.config.clone(),
                                                                 self.exec.clone(),
                                                                 self.httpc.clone()));
                self.connections.get_mut(&conn_id).unwrap().register(poll);
            }
            Err(e) => {
                warn!("encountered error while accepting connection; err={:?}", e);
            }
        }
    }

    fn handle_event(&mut self, poll: &mut mio::Poll, event: &Event) {
        let token = event.token();
        let token_us: usize = usize::from(token);

        if token_us >= MIO_HTTPC_OFFSET {
            match self.httpc.lock() {
                Ok(mut httpc) => httpc.handle_event(poll, event),
                Err(err) => {
                    error!("failed to acquire lock on 'httpc' when handling event: {:?}", err);
                }
            }
        } else if token_us >= MIO_EXEC_OFFSET {
            match self.exec.lock() {
                Ok(mut exec) => exec.ready(poll, event.token()),
                Err(err) => {
                    error!("failed to acquire lock on 'exec' when handling event: {:?}", err);
                }
            }
        } else if token_us >= MIO_SERVER_OFFSET {
            let mut conn_id = token_us;
            let mut is_wakeup = false;
            if token_us % 2 != 0 {
                conn_id = token_us - 1;
                is_wakeup = true;
            }

            if self.connections.contains_key(&conn_id) {
                self.connections
                    .get_mut(&conn_id)
                    .unwrap()
                    .ready(poll, event, is_wakeup);

                if self.connections[&conn_id].is_closed() {
                    self.connections.remove(&conn_id);
                }
            }
        } else {
            warn!("unhandled token: {}", token_us);
        }
    }

    pub fn check_timeouts(&mut self, poll: &mut mio::Poll) {
        let now = Instant::now();
        if now.saturating_duration_since(self.last_timeout).lt(&MIO_TIMEOUT_POLL) {
            return;
        }

        for (_, conn) in self.connections.iter_mut() {
            conn.check_timeout(poll, &now);
        }

        match self.httpc.lock() {
            Ok(mut httpc) => httpc.check_timeouts(poll),
            Err(err) => {
                error!("failed to acquire lock on 'httpc' when checking timeouts: {:?}", err);
            }
        }

        match self.exec.lock() {
            Ok(mut exec) => exec.check_timeouts(poll, &now),
            Err(err) => {
                error!("failed to acquire lock on 'exec' when checking timeouts: {:?}", err);
            }
        }

        self.last_timeout = now;
    }
}

#[inline]
fn create_net_listener(addr: &str) -> std::net::TcpListener {
    let listener = TcpBuilder::new_v4().unwrap()
        .reuse_address(true).unwrap()
        .reuse_port(true).unwrap()
        .bind(&addr).unwrap()
        .listen(TCP_BACKLOG).unwrap();
    listener.set_nonblocking(true).unwrap();
    listener
}

pub(crate) fn start_api_server(addr: &str) {
    let config = Arc::new(Config::new(
        MAX_BYTES_RECEIVED,
        REQUEST_TIMEOUT,
        EXEC_TIMEOUT,
    Some(DEFERRAL_BACKLOG),
    Some(FUTURE_BACKLOG)));

    let listener = TcpListener::from_std(
        create_net_listener(&addr)).unwrap();

    let mut poll = mio::Poll::new().unwrap();
    let mut server = Server::new(listener, config);
    let mut events = mio::Events::with_capacity(
        MIO_EVENTS_CAPACITY);

    server.register(&mut poll).unwrap();

    info!("???? [{}] starting API server ({})", server.id, &addr);

    loop {
        poll.poll(&mut events, Some(MIO_TIMEOUT_POLL))
            .unwrap();

        server.check_timeouts(&mut poll);

        for event in events.iter() {
            match event.token() {
                LISTENER_TOKEN => {
                    server.accept(&mut poll);
                }
                _ => {
                    server.handle_event(&mut poll, &event)
                }
            }
        }
    }
}