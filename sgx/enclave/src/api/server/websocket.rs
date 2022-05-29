use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;

use futures::future::BoxFuture;
use mio::net::TcpStream;
use std::sync::SgxMutex;
use tungstenite::Message;
use tungstenite::protocol::{Role, WebSocketConfig, WebSocketContext};

use crate::api::handler::context::Context;
use crate::api::results::{Error, ErrorKind};
use crate::api::server::connection::Deferral;

pub(crate) type SubscriptionHandler = Arc<dyn Send + Sync + Fn(Arc<SgxMutex<Context>>, Arc<Message>) -> BoxFuture<'static, ()>>;
pub(crate) type SubscriptionHandlerFn = fn(Arc<SgxMutex<Context>>, Arc<Message>) -> BoxFuture<'static, ()>;

pub(crate) struct WebSocket {
    deferral: Arc<SgxMutex<Deferral>>,
    subscriptions: Vec<SubscriptionHandler>,
    context: Option<Arc<SgxMutex<Context>>>,
    ws_context: WebSocketContext,
    pending: Option<Vec<Message>>,
    ready: bool,
}

impl WebSocket {
    #[inline]
    pub(crate) fn new(deferral: Arc<SgxMutex<Deferral>>) -> Self {
        Self {
            deferral,
            subscriptions: Vec::new(),
            context: None,
            ws_context: WebSocketContext::new(
                Role::Server, Some(WebSocketConfig::default()),
            ),
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
    pub fn send(&mut self, msg: Message) -> Result<(), Error> {
        if !self.ready {
            if self.pending.is_none() {
                self.pending = Some(Vec::new());
            }

            self.pending.as_mut().unwrap().push(msg);

            return Ok(());
        }

        return match self.deferral.as_ref().lock() {
            Ok(mut deferral) => {
                deferral.defer(Box::new(move |conn| {
                    conn.ws_send(msg)
                }))
            }
            Err(err) => {
                Err(Error::new_with_kind(
                    ErrorKind::WSFault,
                    format!("failed to acquire lock on 'deferral' \
                    during Websocket->send: {:?}", err).to_string(),
                ))
            }
        };
    }

    #[inline]
    pub fn send_with_tls_stream(
        &mut self,
        msg: Message,
        tls_stream: &mut rustls::Stream<rustls::ServerConnection, TcpStream>,
    ) -> Result<(), Error> {
        _send_msg_to_ws(&mut self.ws_context, msg, tls_stream)
    }

    #[inline]
    pub fn activate(
        &mut self,
        tls_stream: &mut rustls::Stream<rustls::ServerConnection, TcpStream>,
        context: Context,
    ) -> Result<(), Error> {
        self.context = Some(Arc::new(SgxMutex::new(context)));
        self.ready = true;

        if self.pending.is_some() {
            for msg in self.pending.take().unwrap() {
                _send_msg_to_ws(&mut self.ws_context, msg, tls_stream)?;
            }
        }

        Ok(())
    }

    #[inline]
    pub fn handle(
        &mut self,
        tls_stream: &mut rustls::Stream<rustls::ServerConnection, TcpStream>,
    ) -> Result<(), Error> {
        return match self.ws_context.read_message(tls_stream) {
            Ok(msg) => {
                self._broadcast_msg_to_subscribers(
                    self.context.as_ref().unwrap().clone(),
                    Arc::new(msg)
                )
            }
            Err(err) => {
                Err(Error::new_with_kind(
                    ErrorKind::WSFault,
                    format!("failed to read ws message: {:?}", err),
                ))
            }
        };
    }

    #[inline]
    fn _broadcast_msg_to_subscribers(
        &self,
        ctx: Arc<SgxMutex<Context>>,
        msg: Arc<Message>
    ) -> Result<(), Error> {
        return match self.deferral.lock() {
            Ok(mut deferral) => {
                for sub in self.subscriptions.iter() {
                    let sub = Arc::clone(sub);
                    let ctx = Arc::clone(&ctx);
                    let msg = Arc::clone(&msg);

                    deferral.spawn(async move {
                        sub(ctx, msg).await
                    })?;
                }

                Ok(())
            }
            Err(err) => {
                Err(Error::new_with_kind(
                    ErrorKind::WSFault,
                    format!("failed to acquire lock on 'deferral' \
                            during Websocket->handle: {:?}", err).to_string(),
                ))
            }
        }
    }
}

#[inline]
fn _send_msg_to_ws(
    ws_context: &mut WebSocketContext,
    msg: Message,
    tls_stream: &mut rustls::Stream<rustls::ServerConnection, TcpStream>,
) -> Result<(), Error> {
    return match ws_context.write_message(tls_stream, msg) {
        Ok(_) => Ok(()),
        Err(err) => {
            Err(Error::new_with_kind(
                ErrorKind::WSFault,
                format!("failed to write ws messages: {:?}", err).to_string(),
            ))
        }
    }
}