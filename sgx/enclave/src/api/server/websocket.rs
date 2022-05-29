use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;

use futures::future::BoxFuture;
use std::sync::SgxMutex;

use crate::api::handler::context::Context;
use crate::api::results::{Error, ErrorKind};
use crate::api::server::connection::{Deferral};

pub(crate) type SubscriptionHandler = Arc<dyn Send + Sync + for<'a> Fn(&'a mut Context) -> BoxFuture<'a, Result<(), Error>>>;
pub(crate) type SubscriptionHandlerFn = for<'a> fn(&'a mut Context) -> BoxFuture<'a, Result<(), Error>>;

pub(crate) struct WebSocket {
    deferral: Arc<SgxMutex<Deferral>>,
    subscriptions: Vec<SubscriptionHandler>,
    pending: Option<Vec<Vec<u8>>>,
    ready: bool,
}

impl WebSocket {
    #[inline]
    pub(crate) fn new(deferral: Arc<SgxMutex<Deferral>>) -> Self {
        Self {
            deferral,
            subscriptions: Vec::new(),
            pending: None,
            ready: false,
        }
    }

    #[inline]
    pub(crate) fn subscribe(&mut self, handler: SubscriptionHandler) -> Result<(), Error> {
        self.subscriptions.push(handler);

        Ok(())
    }

    #[inline]
    pub fn send(&mut self, data: Vec<u8>) -> Result<(), Error> {
        if !self.ready {
            if self.pending.is_none() {
                self.pending = Some(Vec::new());
            }

            self.pending.as_mut().unwrap().push(data);
            return Ok(());
        }

        let data = Arc::new(data);

        return match self.deferral.as_ref().lock() {
            Ok(mut deferral) => {
                deferral.defer(Arc::new(move |conn| {
                    send_websocket_frame(data.clone(), conn.mut_tls_con())
                }));

                Ok(())
            }
            Err(err) => {
                Err(Error::new_with_kind(
                    ErrorKind::WSFault,
                    format!("failed to acquire lock on 'deferral' \
                    during Websocket->send: {:?}", err).to_string()
                ))
            }
        }
    }

    #[inline]
    pub fn activate(&mut self, tls_conn: &mut rustls::ServerConnection) -> Result<(), Error>  {
        self.ready = true;

        if self.pending.is_some() {
            for p in self.pending.take().unwrap() {
                send_websocket_frame(Arc::new(p), tls_conn)?;
            }
        }

        Ok(())
    }

    #[inline]
    pub fn handle(&mut self, tls_conn: &mut rustls::ServerConnection) -> Result<(), Error>  {
        Ok(())
    }
}

#[inline]
fn send_websocket_frame(data: Arc<Vec<u8>>, tls_conn: &mut rustls::ServerConnection) -> Result<(), Error> {
    // TODO:
    Ok(())
}