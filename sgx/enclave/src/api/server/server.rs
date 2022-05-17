use alloc::sync::Arc;

use mio::Token;
use mio::event::Event;
use mio::net::TcpListener;
use net2::TcpBuilder;
use net2::unix::UnixTcpBuilderExt;
use std::collections::HashMap;

use api::server::config::Config;
use api::server::connection::Connection;

const LISTENER: Token = Token(0);

const MAX_BYTES_RECEIVED: usize = 50 * 1024; // 50 Kb
const TCP_BACKLOG: i32 = 1024;
const MIO_EVENTS_CAPACITY: usize = 1024;

struct Server {
    server: TcpListener,
    connections: HashMap<Token, Connection>,
    config: Arc<Config>,
    next_id: usize,
}

impl Server {
    fn new(server: TcpListener) -> Self {
        Self {
            server,
            connections: HashMap::new(),
            config: Arc::new(Config::new(
                MAX_BYTES_RECEIVED)),
            next_id: 2,
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
        poll.poll(&mut events, None)
            .unwrap();

        for event in events.iter() {
            match event.token() {
                LISTENER => {
                    if !server.accept(&mut poll) {
                        break 'outer;
                    }
                }
                _ => {
                    server.conn_event(&mut poll, &event)
                }
            }
        }
    }
}