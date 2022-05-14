use core::mem::ManuallyDrop;
use lazy_static::lazy_static;
use log::warn;
use sgx_types::*;

use std::untrusted::fs;
use std::io::BufReader;

use std::vec::Vec;
use std::boxed::Box;
use std::io::{Read, Write};
use std::sync::{Arc, SgxRwLock};
use std::net::TcpStream;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, AtomicPtr, Ordering};

use rustls::{Session, NoClientAuth};

static GLOBAL_CONTEXT_COUNT: AtomicUsize = AtomicUsize::new(0);

lazy_static! {
    static ref GLOBAL_CONTEXTS: SgxRwLock<HashMap<usize, AtomicPtr<ApiSession>>> = {
        SgxRwLock::new(HashMap::new())
    };

    static ref CONFIG: Arc<rustls::ServerConfig> = make_config();
}

pub enum HandleResult {
    EOF,
    Error,
    Continue,
    Close
}

pub(crate) struct ApiSession {
    socket: ManuallyDrop<TcpStream>,
    tls_session: rustls::ServerSession,
}

impl ApiSession {
    fn new(fd: c_int, cfg: Arc<rustls::ServerConfig>) -> Self {
        Self {
            socket: ManuallyDrop::new(TcpStream::new(fd).unwrap()),
            tls_session: rustls::ServerSession::new(&cfg),
        }
    }

    fn read_tls(&mut self) -> c_int {
        // Read TLS data.  This fails if the underlying TCP connection
        // is broken.
        let rc = self.tls_session.read_tls(&mut *self.socket);
        if rc.is_err() {
            warn!("ApiSession: TLS read error: {:?}", rc);
            return -1;
        }

        // If we're ready but there's no data: EOF.
        if rc.unwrap() == 0 {
            // EOF.
            return -1;
        }

        // Reading some TLS data might have yielded new TLS
        // messages to process.  Errors from this indicate
        // TLS protocol problems and are fatal.
        let processed = self.tls_session.process_new_packets();
        if processed.is_err() {
            warn!("ApiSession: TLS error: {:?}", processed.unwrap_err());
            return -1;
        }
        return 0;
    }

    fn read(&mut self, plaintext: &mut Vec<u8>) -> c_int {
        // Having read some TLS data, and processed any new messages,
        // we might have new plaintext as a result.
        //
        // Read it and then write it to stdout.
        let rc = self.tls_session.read_to_end(plaintext);

        // If that fails, the peer might have started a clean TLS-level
        // session closure.
        if rc.is_err() {
            let err = rc.unwrap_err();
            warn!("ApiSession: Plaintext read error: {:?}", err);
            return -1;
        }
        plaintext.len() as c_int
    }

    fn write(&mut self, plaintext: &[u8]) -> c_int{
        self.tls_session.write(plaintext).unwrap() as c_int
    }

    fn write_tls(&mut self) {
        self.tls_session.write_tls(&mut *self.socket).unwrap();
    }

    pub(crate) fn wants_read(&self) -> bool {
        self.tls_session.wants_read()
    }

    pub(crate) fn wants_write(&self) -> bool {
        self.tls_session.wants_write()
    }

    pub(crate) fn handle(&mut self) -> HandleResult {
        // 1. Initialize the TLS read
        let r = self.read_tls();
        if r == -1 {
            return HandleResult::EOF;
        }

        // 2. Read request.
        let mut plaintext = Vec::new();
        let r = self.read(&mut plaintext);
        if r == -1 {
            return HandleResult::EOF;
        }

        //println!("REQUEST({}):: {}", plaintext.len(),
        //         String::from_utf8(plaintext.clone()).expect("failed to decode"));

        if plaintext.len() <= 0 {
            // 3. Send empty response (no request data).
            self.write_tls();
        } else {
            // 3. Parse response.

            let response = b"HTTP/1.0 200 OK\r\nConnection: close\r\n\r\nHello world from rustls tlsserver\r\n";

            //println!("RESPONSE({}):: {}", response.len(),
            //         String::from_utf8(response.to_vec()).expect("failed to decode"));

            let r = self.write(response);
            if r > 0 {
                // Finalize the TLS write
                self.write_tls();
                self.tls_session.send_close_notify();

                return HandleResult::Close
            }
        }

        HandleResult::Continue
    }
}

fn load_certs(filename: &str) -> Vec<rustls::Certificate> {
    let certfile = fs::File::open(filename).expect("cannot open certificate file");
    let mut reader = BufReader::new(certfile);
    rustls::internal::pemfile::certs(&mut reader).unwrap()
}

fn load_private_key(filename: &str) -> rustls::PrivateKey {
    let rsa_keys = {
        let keyfile = fs::File::open(filename)
            .expect("cannot open private key file");
        let mut reader = BufReader::new(keyfile);
        rustls::internal::pemfile::rsa_private_keys(&mut reader)
            .expect("file contains invalid rsa private key")
    };

    let pkcs8_keys = {
        let keyfile = fs::File::open(filename)
            .expect("cannot open private key file");
        let mut reader = BufReader::new(keyfile);
        rustls::internal::pemfile::pkcs8_private_keys(&mut reader)
            .expect("file contains invalid pkcs8 private key (encrypted keys not supported)")
    };

    // prefer to load pkcs8 keys
    if !pkcs8_keys.is_empty() {
        pkcs8_keys[0].clone()
    } else {
        assert!(!rsa_keys.is_empty());
        rsa_keys[0].clone()
    }
}

fn make_config() -> Arc<rustls::ServerConfig> {
    let mut config = rustls::ServerConfig::new(NoClientAuth::new());

    // TODO: Load from secure file (fetched from Omnibus).
    let certs = load_certs("end.fullchain");
    let privkey = load_private_key("end.rsa");

    config.set_single_cert_with_ocsp_and_sct(certs, privkey, vec![], vec![]).unwrap();

    Arc::new(config)
}

pub(crate) struct SessionManager;

impl SessionManager {
    pub(crate) fn create_session(fd: c_int) -> Option<usize> {
        let p: *mut ApiSession = Box::into_raw(Box::new(ApiSession::new(fd, CONFIG.clone())));

        Self::new_session(p)
    }

    fn new_session(svr_ptr : *mut ApiSession) -> Option<usize> {
        match GLOBAL_CONTEXTS.write() {
            Ok(mut gctxts) => {
                let curr_id = GLOBAL_CONTEXT_COUNT.fetch_add(1, Ordering::SeqCst);
                gctxts.insert(curr_id, AtomicPtr::new(svr_ptr));
                Some(curr_id)
            },
            Err(x) => {
                warn!("SessionManager: Locking global context SgxRwLock failed! {:?}", x);
                None
            },
        }
    }

    pub(crate) fn get_session(sess_id: size_t) -> Option<*mut ApiSession> {
        match GLOBAL_CONTEXTS.read() {
            Ok(gctxts) => {
                match gctxts.get(&sess_id) {
                    Some(s) => {
                        Some(s.load(Ordering::SeqCst))
                    },
                    None => {
                        warn!("SessionManager: Global contexts cannot find session id = {}", sess_id);
                        None
                    }
                }
            },
            Err(x) => {
                warn!("SessionManager: Locking global context SgxRwLock failed on get_session! {:?}", x);
                None
            },
        }
    }

    pub(crate) fn remove_session(sess_id: size_t) {
        if let Ok(mut gctxts) = GLOBAL_CONTEXTS.write() {
            if let Some(session_ptr) = gctxts.get(&sess_id) {
                let session_ptr = session_ptr.load(Ordering::SeqCst);
                let session = unsafe { &mut *(session_ptr) };
                let _ = unsafe { Box::<ApiSession>::from_raw(session as *mut _) };
                let _ = gctxts.remove(&sess_id);
            }
        }
    }
}