use alloc::string::String;
use alloc::sync::Arc;

use lazy_static::lazy_static;

use api::handler::router::Router;
use api::middleware::recovery::middleware_recovery;

lazy_static! {
    pub(crate) static ref ROUTER: Arc<Router> = Arc::new(build_routes());
}

fn build_routes() -> Router {
    let mut r = Router::new();

    r.require(middleware_recovery);

    r.route("/test", |mut r| {
        r.require(|req, res, next| {
            info!("inside test");
            next(req, res)
        });

        r.get("/ping", |_req, res| {
            res.ok("PONG");
            Ok(())
        });

        r.get("/panic", |_req, _res| {
            panic!("YELP");
        });

        r.post("/post", |_req, res| {
            //println!("Received: {:?}", req.body());

            res.ok("Ok");
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