use alloc::sync::Arc;
use core::time::Duration;

use mio::event::Event;
use mio::net::TcpListener;
use mio::Token;
use net2::TcpBuilder;
use net2::unix::UnixTcpBuilderExt;
use std::collections::HashMap;
use std::sync::SgxMutex;
use std::time::Instant;

use crate::api::server::config::Config;
use crate::api::server::connection::{Connection, TlsSession};
use crate::api::server::exec::ExecManager;
use crate::api::server::httpc::HttpcManager;

const LISTENER: Token = Token(0);

// 50 Kb
const MAX_BYTES_RECEIVED: usize = 50 * 1024;
// System default for now.
const KEEPALIVE_DURATION: Duration = Duration::from_secs(7200);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const EXEC_TIMEOUT: Duration = Duration::from_secs(10);

const TCP_BACKLOG: i32 = 1024;

const MIO_EVENTS_CAPACITY: usize = TCP_BACKLOG as usize * 2;
const MIO_TIMEOUT_POLL: Duration = Duration::from_millis(1000);

const MIO_SERVER_OFFSET: usize = 10;
const MIO_EXEC_OFFSET: usize = MIO_SERVER_OFFSET + u32::MAX as usize;
const MIO_HTTPC_OFFSET: usize = MIO_EXEC_OFFSET + u32::MAX as usize;

pub(crate) struct Server {
    server: TcpListener,
    connections: HashMap<Token, Connection>,
    config: Arc<Config>,
    exec: Arc<SgxMutex<ExecManager>>,
    httpc: Arc<SgxMutex<HttpcManager>>,
    next_id: usize,
    last_timeout: Instant,
}

impl Server {
    fn new(server: TcpListener, config: Arc<Config>) -> Self {
        let exec = Arc::new(
            SgxMutex::new(ExecManager::new(MIO_EXEC_OFFSET, config.clone())));
        let httpc = Arc::new(
            SgxMutex::new(HttpcManager::new(
                MIO_HTTPC_OFFSET, None)));

        Self {
            server,
            connections: HashMap::new(),
            config: config.clone(),
            exec,
            httpc,
            next_id: MIO_SERVER_OFFSET,
            last_timeout: Instant::now(),
        }
    }

    fn accept(&mut self, poll: &mut mio::Poll) -> bool {
        match self.server.accept() {
            Ok((socket, addr)) => {
                debug!("accepted connection: {}", addr);

                let session = rustls::ServerSession::new(
                    &self.config.tls_config().clone());

                let token = Token(self.next_id);

                if self.next_id + 1 >= (MIO_SERVER_OFFSET + u32::MAX as usize) {
                    self.next_id = MIO_SERVER_OFFSET;
                } else {
                    self.next_id += 1;
                }

                let conn_session = Arc::new(SgxMutex::new(
                    TlsSession::new(token.clone(), socket, session,
                                    self.config.clone())
                ));

                self.connections.insert(token, Connection::new(conn_session,
                                                               self.exec.clone(),
                                                               self.httpc.clone()));
                self.connections.get_mut(&token).unwrap().register(poll);

                true
            }
            Err(e) => {
                warn!("encountered error while accepting connection; err={:?}", e);
                false
            }
        }
    }

    pub fn check_timeouts(&mut self, poll: &mut mio::Poll) -> bool {
        let now = Instant::now();
        if now.saturating_duration_since(self.last_timeout).lt(&MIO_TIMEOUT_POLL) {
            return false;
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
            Ok(mut exec) => exec.check_timeouts(poll),
            Err(err) => {
                error!("failed to acquire lock on 'exec' when checking timeouts: {:?}", err);
            }
        }

        self.last_timeout = now;
        true
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
            if self.connections.contains_key(&token) {
                self.connections
                    .get_mut(&token)
                    .unwrap()
                    .ready(poll, event);

                if self.connections[&token].is_closed() {
                    self.connections.remove(&token);
                }
            }
        }
    }
}

pub(crate) fn start_api_server(addr: &str) {
    let config = Arc::new(Config::new(
        MAX_BYTES_RECEIVED,
        KEEPALIVE_DURATION,
        REQUEST_TIMEOUT,
        EXEC_TIMEOUT));

    let listener = TcpListener::from_std(
        TcpBuilder::new_v4().unwrap()
            .reuse_address(true).unwrap()
            .reuse_port(true).unwrap()
            .bind(&addr).unwrap()
            .listen(TCP_BACKLOG).unwrap()).unwrap();

    let mut poll = mio::Poll::new().unwrap();
    poll.register(&listener,
                  LISTENER,
                  mio::Ready::readable(),
                  mio::PollOpt::level()).unwrap();

    let mut server = Server::new(listener, config);
    let mut events = mio::Events::with_capacity(
        MIO_EVENTS_CAPACITY);

    'outer: loop {
        poll.poll(&mut events, Some(MIO_TIMEOUT_POLL))
            .unwrap();

        if server.check_timeouts(&mut poll) {
            continue 'outer;
        }

        for event in events.iter() {
            match event.token() {
                LISTENER => {
                    if !server.accept(&mut poll) {
                        continue 'outer;
                    }
                }
                _ => {
                    server.handle_event(&mut poll, &event)
                }
            }
        }
    }
}