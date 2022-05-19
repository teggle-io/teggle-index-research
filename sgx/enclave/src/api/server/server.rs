use alloc::sync::Arc;
use core::time::Duration;

use mio::event::Event;
use mio::net::TcpListener;
use mio::Token;
use net2::TcpBuilder;
use net2::unix::UnixTcpBuilderExt;
use std::collections::HashMap;
use std::time::Instant;

use crate::api::server::config::Config;
use crate::api::server::connection::Connection;

const LISTENER: Token = Token(0);

// 50 Kb
const MAX_BYTES_RECEIVED: usize = 50 * 1024;
// System default for now.
const KEEPALIVE_DURATION: Duration = Duration::from_secs(7200);

const TCP_BACKLOG: i32 = 1024;

const MIO_EVENTS_CAPACITY: usize = TCP_BACKLOG as usize * 2;
const MIO_TIMEOUT_POLL: Duration = Duration::from_millis(1000);

struct Server {
    server: TcpListener,
    connections: HashMap<Token, Connection>,
    config: Arc<Config>,
    next_id: usize,
    last_timeout: Instant,
}

impl Server {
    fn new(server: TcpListener) -> Self {
        Self {
            server,
            connections: HashMap::new(),
            config: Arc::new(Config::new(
                MAX_BYTES_RECEIVED,
                KEEPALIVE_DURATION)),
            next_id: 2,
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

                if self.next_id + 1 >= usize::MAX {
                    self.next_id = 0;
                } else {
                    self.next_id += 1;
                }

                self.connections.insert(token, Connection::new(socket,
                                                               session,
                                                               token,
                                                               self.config.clone()));
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

        self.last_timeout = now;
        true
    }

    fn conn_event(&mut self, poll: &mut mio::Poll, event: &Event) {
        let token = event.token();

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

pub(crate) fn start_api_server(addr: &str) {
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

    let mut server = Server::new(listener);
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
                    server.conn_event(&mut poll, &event)
                }
            }
        }
    }
}