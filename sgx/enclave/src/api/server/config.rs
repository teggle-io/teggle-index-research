use alloc::vec::Vec;
use core::time::Duration;
use rustls::server::NoClientAuth;

use std::io::BufReader;
use std::sync::Arc;
use std::untrusted::fs;

fn load_certs(filename: &str) -> Vec<rustls::Certificate> {
    let certfile = fs::File::open(filename).expect("cannot open certificate file");
    let mut reader = BufReader::new(certfile);
    rustls_pemfile::certs(&mut reader)
        .unwrap()
        .iter()
        .map(|v| rustls::Certificate(v.clone()))
        .collect()
}

fn load_private_key(filename: &str) -> rustls::PrivateKey {
    let keyfile = fs::File::open(filename).expect("cannot open private key file");
    let mut reader = BufReader::new(keyfile);

    loop {
        match rustls_pemfile::read_one(&mut reader).expect("cannot parse private key .pem file") {
            Some(rustls_pemfile::Item::RSAKey(key)) => return rustls::PrivateKey(key),
            Some(rustls_pemfile::Item::PKCS8Key(key)) => return rustls::PrivateKey(key),
            Some(rustls_pemfile::Item::ECKey(key)) => return rustls::PrivateKey(key),
            None => break,
            _ => {}
        }
    }

    panic!(
        "no keys found in {:?} (encrypted keys not supported)",
        filename
    );
}

pub struct Config {
    tls_config: Arc<rustls::ServerConfig>,
    max_bytes_received: usize,
    request_timeout: Duration,
    exec_timeout: Duration,
    max_defers_queue: Option<usize>,
    max_futures_queue: Option<usize>,
}

impl Config {
    pub fn new(
        max_bytes_received: usize,
        request_timeout: Duration,
        exec_timeout: Duration,
        max_defers_queue: Option<usize>,
        max_futures_queue: Option<usize>,
    ) -> Self {
        Self {
            tls_config: make_config(),
            max_bytes_received,
            request_timeout,
            exec_timeout,
            max_defers_queue,
            max_futures_queue
        }
    }

    pub fn tls_config(&self) -> &Arc<rustls::ServerConfig> {
        &self.tls_config
    }

    pub fn max_bytes_received(&self) -> usize {
        self.max_bytes_received
    }

    pub fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    pub fn exec_timeout(&self) -> Duration {
        self.exec_timeout
    }

    pub fn max_defers_queue(&self) -> Option<usize> {
        self.max_defers_queue
    }

    pub fn max_futures_queue(&self) -> Option<usize> {
        self.max_futures_queue
    }
}

pub fn make_config() -> Arc<rustls::ServerConfig> {
    // TODO: Load from secure file (fetched from Omnibus).
    let certs = load_certs("end.fullchain");
    let privkey = load_private_key("end.rsa");

    let config = rustls::ServerConfig::builder()
        .with_cipher_suites(&rustls::ALL_CIPHER_SUITES.to_vec())
        .with_safe_default_kx_groups()
        .with_protocol_versions(&rustls::ALL_VERSIONS.to_vec())
        .expect("inconsistent cipher-suites/versions specified")
        .with_client_cert_verifier(NoClientAuth::new())
        .with_single_cert_with_ocsp_and_sct(certs, privkey, vec![], vec![])
        .expect("bad certificates/private key");

    Arc::new(config)
}