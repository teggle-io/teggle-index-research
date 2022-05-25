use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;

use lazy_static::lazy_static;

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
        r.require(|req, res, ctx, next| Box::pin(async move {
            //info!("inside test");
            ctx.insert("test", "value");

            next(req, res, ctx).await
        }));

        r.get("/ping", |_req, res, _ctx| Box::pin(async move {
            res.ok("PONG")
        }));

        r.get("/panic", |_req, _res, _ctx| Box::pin(async move {
            panic!("YELP");
        }));

        r.post("/post", |req, res, ctx| Box::pin(async move {
            let content_type: Option<String> = req.header(http::header::CONTENT_TYPE);
            let test_val: Option<String> = ctx.get("test");
            let payload: TestPayload = req.json()?;

            error!("Content-Type: {:?}", content_type);
            error!("test value: {:?}", test_val);
            error!("Payload: {:?}", payload);

            res.ok("Ok")
        }));

        r.get("/fetch", |
            _req,
            res: &mut Response,
            ctx: &mut Context
        | Box::pin(async move {
            let resp = ctx.https()
                .host("catfact.ninja")
                .path("fact")
                .get().await?;

            if let Some((_, body)) = resp {
                res.header(http::header::CONTENT_TYPE, "application/json");
                res.body(body);

                Ok(())
            } else {
                res.ok("No results")
            }
        }));
    });

    r.get("/ping", |_req, res, _ctx| Box::pin(async move {
        res.ok("PONG")
    }));

    r.get("/hello/:name", |req, res, _ctx| Box::pin(async move {
        let name: Option<String> = req.var("name");

        res.ok(format!("Hello {}", name.unwrap()).as_str())
    }));

    r.get("/calc/:a/:b", |req, res, _ctx| Box::pin(async move {
        let a: Option<u32> = req.var("a");
        let b: Option<u32> = req.var("b");

        res.ok(format!("Sum {}", a.unwrap() + b.unwrap()).as_str())
    }));

    r
}