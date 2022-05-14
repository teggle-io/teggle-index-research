use lazy_static::lazy_static;
use mio::{Token};
use mio::event::Event;
use mio::net::{TcpListener};
use net2::TcpBuilder;
use net2::unix::UnixTcpBuilderExt;
use std::sync::{Arc};
use std::collections::HashMap;
use std::io::ErrorKind;
use api::service::config::make_config;
use api::service::connection::Connection;

const LISTENER: Token = Token(0);

lazy_static! {
    static ref CONFIG: Arc<rustls::ServerConfig> = make_config();
}

struct Server {
    server: TcpListener,
    connections: HashMap<Token, Connection>,
    next_id: usize,
}

impl Server {
    fn new(server: TcpListener) -> Self {
        Self {
            server,
            connections: HashMap::new(),
            next_id: 2,
        }
    }

    fn accept(&mut self, poll: &mut mio::Poll) -> bool {
        match self.server.accept() {
            Ok((socket, _addr)) => {
                let session = rustls::ServerSession::new(&CONFIG.clone());

                let token = Token(self.next_id);

                if self.next_id + 1 >= usize::MAX {
                    self.next_id = 0;
                } else {
                    self.next_id += 1;
                }

                self.connections.insert(token, Connection::new(socket,
                                                               session,
                                                               token));
                self.connections.get_mut(&token).unwrap().register(poll);

                true
            }
            Err(e) => {
                match e.kind() {
                    // Ignore (happens sometimes because we're sharing the socket).
                    ErrorKind::WouldBlock => {}
                    _ => {
                        println!("encountered error while accepting connection; err={:?}", e);
                    }
                }

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
            .listen(1024).unwrap()).unwrap();

    let mut poll = mio::Poll::new().unwrap();
    poll.register(&listener,
                  LISTENER,
                  mio::Ready::readable(),
                  mio::PollOpt::level()).unwrap();

    let mut server = Server::new(listener);
    let mut events = mio::Events::with_capacity(1024);

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