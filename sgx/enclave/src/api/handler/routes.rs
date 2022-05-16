use alloc::string::String;
use alloc::sync::Arc;
use lazy_static::lazy_static;

use api::handler::router::Router;

lazy_static! {
    pub(crate) static ref ROUTER: Arc<Router> = Arc::new(build_routes());
}

fn build_routes() -> Router {
    let mut r = Router::new();
    r.route("/test", |mut r| {
        r.get("/ping", |_req, res| {
            res.ok("PONG");
            Ok(())
        });
    });

    r.get("/ping", |_req, res| {
        res.ok("PONG");
        Ok(())
    });

    r.get("/hello/:name", |req, res| {
        let name: Option<String> = req.path_var("name");
        res.ok(format!("Hello {}", name.unwrap()).as_str());

        Ok(())
    });

    r.get("/calc/:a/:b", |req, res| {
        let a: Option<u32> = req.path_var("a");
        let b: Option<u32> = req.path_var("b");
        res.ok(format!("Sum {}", a.unwrap() + b.unwrap()).as_str());

        Ok(())
    });

    r
}