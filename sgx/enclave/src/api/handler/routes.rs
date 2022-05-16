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

    r
}