use std::{net, thread};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::Shutdown;
use std::os::unix::io::{AsRawFd, FromRawFd};

use log::warn;
use mio::{Events, Interest, Poll, Token};
use mio::event::Event;
use mio::net::{TcpListener, TcpStream};
use sgx_types::*;

use enclave::ecall::api::{ecall_api_server_close, ecall_api_server_handle, ecall_api_server_new,
                          ecall_api_server_wants_read, ecall_api_server_wants_write};
use ENCLAVE_DOORBELL;

// Token for our listening socket.
const LISTENER: Token = Token(0);

const THREAD_NUM: u8 = 10;

struct ApiServer {
    enclave_id: sgx_enclave_id_t,
    server: TcpListener,
    connections: HashMap<Token, Connection>,
    next_id: usize,
}

impl ApiServer {
    fn new(enclave_id: sgx_enclave_id_t, server: TcpListener) -> Self {
        Self {
            enclave_id,
            server,
            connections: HashMap::new(),
            next_id: 2,
        }
    }

    fn accept(&mut self, poll: &mut mio::Poll) -> bool {
        match self.server.accept() {
            Ok((socket, _addr)) => {
                let mut tlsserver_id: usize = 0xFFFF_FFFF_FFFF_FFFF;
                let retval = unsafe {
                    ecall_api_server_new(self.enclave_id,
                                         &mut tlsserver_id,
                                         socket.as_raw_fd())
                };

                if retval != sgx_status_t::SGX_SUCCESS {
                    warn!("ECALL [ecall_api_server_new] failed {}!", retval);
                    return false;
                }

                if tlsserver_id == 0xFFFF_FFFF_FFFF_FFFF {
                    warn!("ECALL [ecall_api_server_new] failed (no id returned)");
                    return false;
                }

                let token = Token(self.next_id);
                self.next_id += 1;

                self.connections.insert(token, Connection::new(self.enclave_id,
                                                               socket,
                                                               token,
                                                               tlsserver_id));
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

struct Connection {
    enclave_id: sgx_enclave_id_t,
    socket: TcpStream,
    token: mio::Token,
    closing: bool,
    tlsserver_id: usize,
}

impl Connection {
    fn new(enclave_id: sgx_enclave_id_t,
           socket: TcpStream,
           token: mio::Token,
           tlsserver_id: usize)
           -> Self {
        Self {
            enclave_id: enclave_id,
            socket: socket,
            token: token,
            closing: false,
            tlsserver_id: tlsserver_id,
        }
    }

    fn handle(&self) -> isize {
        let mut retval = -1;
        let result = unsafe {
            ecall_api_server_handle(self.enclave_id,
                                    &mut retval,
                                    self.tlsserver_id)
        };
        match result {
            sgx_status_t::SGX_SUCCESS => { retval as isize }
            _ => {
                warn!("ECALL [ecall_api_server_handle] failed {}!", result);
                return -1;
            }
        }
    }

    fn do_handle(&mut self) {
        if self.handle() != 0 {
            // EOF (-1) or response sent (>0)
            self.closing = true;
        }
    }

    fn wants_read(&self) -> bool {
        let mut retval = -1;
        let result = unsafe {
            ecall_api_server_wants_read(self.enclave_id,
                                        &mut retval,
                                        self.tlsserver_id)
        };
        match result {
            sgx_status_t::SGX_SUCCESS => {}
            _ => {
                warn!("ECALL [ecall_api_server_wants_read] failed {}!", result);
                return false;
            }
        }

        match retval {
            0 => false,
            _ => true
        }
    }

    fn wants_write(&self) -> bool {
        let mut retval = -1;
        let result = unsafe {
            ecall_api_server_wants_write(self.enclave_id,
                                         &mut retval,
                                         self.tlsserver_id)
        };

        match result {
            sgx_status_t::SGX_SUCCESS => {}
            _ => {
                warn!("ECALL [ecall_api_server_wants_write] failed {}!", result);
                return false;
            }
        }

        match retval {
            0 => false,
            _ => true
        }
    }

    fn tls_close(&self) {
        unsafe {
            ecall_api_server_close(self.enclave_id, self.tlsserver_id)
        };
    }

    fn ready(&mut self, poll: &mut mio::Poll, ev: &Event) {
        if ev.is_readable() {
            self.do_handle();
        }

        if self.closing {
            self.tls_close();
            let _ = self.socket.shutdown(Shutdown::Both);
        } else {
            self.reregister(poll);
        }
    }

    fn register(&mut self, poll: &mut mio::Poll) {
        let interest = self.event_interest();
        poll.registry()
            .register(&mut self.socket, self.token, interest)
            .unwrap();
    }

    fn reregister(&mut self, poll: &mut mio::Poll) {
        let interest = self.event_interest();
        poll.registry()
            .reregister(&mut self.socket, self.token, interest)
            .unwrap();
    }

    fn event_interest(&mut self) -> mio::Interest {
        let rd = self.wants_read();
        let wr = self.wants_write();

        if rd && wr {
            Interest::READABLE | Interest::WRITABLE
        } else if wr {
            Interest::WRITABLE
        } else {
            Interest::READABLE
        }
    }

    fn is_closed(&self) -> bool {
        self.closing
    }
}

pub(crate) fn run_api_server() {
    let addr: net::SocketAddr = "0.0.0.0:8443".parse().unwrap();
    let listener = TcpListener::bind(addr).unwrap();

    let mut children = vec![];
    let thread_count = std::cmp::min(std::cmp::min(THREAD_NUM,
                                                   ENCLAVE_DOORBELL.capacity()),
                                     num_cpus::get() as u8);
    for _ in 0..thread_count {
        let mut listener = unsafe { TcpListener::from_raw_fd(listener.as_raw_fd()) };

        children.push(thread::spawn(move || {
            let enclave_access_token = ENCLAVE_DOORBELL
                .get_access(false) // This can never be recursive
                .unwrap();
            let enclave = enclave_access_token.unwrap();

            let mut poll = Poll::new().unwrap();
            let mut events = Events::with_capacity(128);

            poll.registry()
                .register(&mut listener, LISTENER, Interest::READABLE)
                .unwrap();

            let mut tlsserv = ApiServer::new(enclave.geteid(), listener);

            // TODO: Remove
            println!("[+] ApiServer started");

            loop {
                poll.poll(&mut events, None).unwrap();

                for event in events.iter() {
                    match event.token() {
                        LISTENER => {
                            tlsserv.accept(&mut poll);
                        }
                        _ => {
                            tlsserv.conn_event(&mut poll, &event)
                        }
                    }
                }
            }
        }));
    }

    for child in children {
        // Wait for the thread to finish. Returns a result.
        let _ = child.join();
    }
}