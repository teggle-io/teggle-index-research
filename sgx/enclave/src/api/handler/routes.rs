use alloc::string::String;
use alloc::sync::Arc;

use lazy_static::lazy_static;

use api::handler::router::Router;
use api::middleware::recovery::middleware_recovery;

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
        r.require(|req, res, ctx, next| {
            info!("inside test");
            ctx.insert("test", "value");

            next(req, res, ctx)
        });

        r.get("/ping", |_req, res, _ctx| {
            res.ok("PONG")
        });

        r.get("/panic", |_req, _res, _ctx| {
            panic!("YELP");
        });

        r.post("/post", |req, res, ctx| {
            let content_type: Option<String> = req.header(http::header::CONTENT_TYPE);
            let test_val: Option<String> = ctx.get("test");
            let payload: TestPayload = req.json()?;

            error!("Content-Type: {:?}", content_type);
            error!("test value: {:?}", test_val);
            error!("Payload: {:?}", payload);

            res.ok("Ok")
        });
    });

    r.get("/ping", |_req, res, _ctx| {
        res.ok("PONG")
    });

    r.get("/hello/:name", |req, res, _ctx| {
        let name: Option<String> = req.var("name");

        res.ok(format!("Hello {}", name.unwrap()).as_str())
    });

    r.get("/calc/:a/:b", |req, res, _ctx| {
        let a: Option<u32> = req.var("a");
        let b: Option<u32> = req.var("b");

        res.ok(format!("Sum {}", a.unwrap() + b.unwrap()).as_str())
    });

    r
}