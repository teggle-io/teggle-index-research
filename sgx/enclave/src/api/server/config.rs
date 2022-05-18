use alloc::vec::Vec;
use core::time::Duration;

use rustls::NoClientAuth;
use std::io::BufReader;
use std::sync::Arc;
use std::untrusted::fs;

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

pub(crate) struct Config {
    tls_config: Arc<rustls::ServerConfig>,
    max_bytes_received: usize,
    keep_alive_time: Duration,
}

impl Config {
    pub(crate) fn new(
        max_bytes_received: usize,
        keep_alive_time: Duration,
    ) -> Self {
        Self {
            tls_config: make_config(),
            max_bytes_received,
            keep_alive_time
        }
    }

    pub(crate) fn tls_config(&self) -> &Arc<rustls::ServerConfig> {
        &self.tls_config
    }

    pub(crate) fn max_bytes_received(&self) -> usize {
        self.max_bytes_received
    }

    pub(crate) fn keep_alive_time(&self) -> Duration {
        self.keep_alive_time
    }
}

pub fn make_config() -> Arc<rustls::ServerConfig> {
    let mut config = rustls::ServerConfig::new(NoClientAuth::new());

    // TODO: Load from secure file (fetched from Omnibus).
    let certs = load_certs("end.fullchain");
    let privkey = load_private_key("end.rsa");

    config.set_single_cert_with_ocsp_and_sct(certs, privkey, vec![], vec![]).unwrap();
    Arc::new(config)
}