use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;

use lazy_static::lazy_static;
use tungstenite::Message;

use crate::api::handler::context::Context;
use crate::api::handler::response::Response;
use crate::api::handler::router::Router;
use crate::api::middleware::recovery::middleware_recovery;

lazy_static! {
    pub(crate) static ref ROUTER: Arc<Router> = Arc::new(build_routes());
}

#[derive(Debug, Serialize, Deserialize)]
struct TestPayload {
    pub name: String,
    pub email: String,
}

fn build_routes() -> Router {
    let mut r = Router::new();

    r.require(middleware_recovery);

    r.route("/test", |mut r| {
        r.require(|ctx, res, next| Box::pin(async move {
            //info!("inside test");
            ctx.insert("test", Box::new("value"));

            next(ctx, res).await
        }));

        r.get("/ping", |_ctx, res| Box::pin(async move {
            res.ok("PONG")
        }));

        r.get("/panic", |_ctx, _res| Box::pin(async move {
            panic!("YELP");
        }));

        r.post("/post", |ctx: &mut Context, res: &mut Response|
            Box::pin(async move {
                let req = ctx.request();
                let content_type: Option<String> = req.header(http::header::CONTENT_TYPE);
                let test_val: Option<&String> = ctx.get("test");
                let payload: TestPayload = req.json()?;

                error!("Content-Type: {:?}", content_type);
                error!("test value: {:?}", test_val);
                error!("Payload: {:?}", payload);

                res.ok("Ok")
            }));

        r.get("/fetch", |ctx: &mut Context, res: &mut Response|
            Box::pin(async move {
                let resp = ctx.https()
                    .host("catfact.ninja")
                    .path("fact")
                    .get().await?;

                if let Some((_, body)) = resp {
                    res.header(http::header::CONTENT_TYPE, "application/json");
                    res.body(body);

                    Ok(())
                } else {
                    res.error(http::StatusCode::NO_CONTENT,"No results")
                }
            }));
    });

    r.get("/ping", |_ctx, res| Box::pin(async move {
        res.ok("PONG")
    }));

    r.get("/ping/empty", |_ctx, _res| Box::pin(async move {
        Ok(())
    }));

    r.get("/hello/:name", |ctx, res| Box::pin(async move {
        let req = ctx.request();
        let name: Option<String> = req.var("name");

        res.ok(format!("Hello {}", name.unwrap()).as_str())
    }));

    r.get("/calc/:a/:b", |ctx, res| Box::pin(async move {
        let req = ctx.request();
        let a: Option<u32> = req.var("a");
        let b: Option<u32> = req.var("b");

        res.ok(format!("Sum {}", a.unwrap() + b.unwrap()).as_str())
    }));

    r.get("/ws", |ctx: &mut Context, _res| Box::pin(async move {
        ctx.subscribe(|ctx, msg| Box::pin(async move {
            match ctx.lock() {
                Ok(ctx) => {
                    match *msg {
                        Message::Text(_)
                        | Message::Binary(_) => {
                            info!("WS MSG: {:?}", msg);

                            if let Err(err) = ctx.send_bin(b"Hello, World".to_vec()) {
                                warn!("failed to send msg: {:?}", err);
                            }

                            if let Err(err) = ctx.send_ping(b"PING".to_vec()) {
                                warn!("failed to send ping: {:?}", err);
                            }
                        }
                        Message::Pong(_) => {
                            // PONG to the above ping.
                            info!("WS PONG");
                        }
                        _ => {}
                    }
                }
                Err(err) => {
                    warn!("failed to acquire ctx lock while processing message: {:?}", err);
                }
            }
        }))?;

        ctx.send_bin(b"Welcome".to_vec())?;

        Ok(())
    }));

    r
}