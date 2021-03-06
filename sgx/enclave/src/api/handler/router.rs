use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use futures::future::BoxFuture;

use http::{Method, StatusCode};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::SgxRwLock;
use crate::api::handler::context::Context;

use crate::api::handler::response::Response;
use crate::api::handler::routes::ROUTER;
use crate::api::results::Error;

const CAPTURE_PLACEHOLDER: &'static str = "*CAPTURE*";

pub(crate) type Handler = Arc<dyn Send + Sync + for<'a> Fn(&'a mut Context, &'a mut Response) -> BoxFuture<'a, Result<(), Error>>>;
pub(crate) type HandlerFn = for<'a> fn(&'a mut Context, &'a mut Response) -> BoxFuture<'a, Result<(), Error>>;
pub(crate) type Middleware = Arc<dyn Send + Sync + for<'a> Fn(&'a mut Context, &'a mut Response, Handler) -> BoxFuture<'a, Result<(), Error>>>;
pub(crate) type MiddlewareFn = for<'a> fn(&'a mut Context, &'a mut Response, Handler) -> BoxFuture<'a, Result<(), Error>>;

#[inline]
pub(crate) async fn route_request(ctx: &mut Context, res: &mut Response) -> Result<(), Error> {
    let (method, path) = {
        let req = ctx.request();
        (req.method(), req.uri().path())
    };

    match ROUTER.clone().find(method, path) {
        Some((handler, captures)) => {
            ctx.request_mut().vars(captures);

            handler.route(ctx, res).await
        }
        None => {
            res.error(StatusCode::NOT_FOUND, "Not Found")
        }
    }
}

pub(crate) struct Router {
    top: Option<Arc<SgxRwLock<Router>>>,
    routes: Option<HashMap<String, RouteHandler>>,
    path: Option<PathBuf>,
    middleware: Vec<Middleware>,
}

impl Router {
    #[inline]
    pub(crate) fn new() -> Self {
        let top = Self {
            top: None,
            routes: Some(HashMap::new()),
            path: None,
            middleware: Vec::new(),
        };

        Self {
            top: Some(Arc::new(SgxRwLock::new(top))),
            routes: None,
            path: None,
            middleware: Vec::new(),
        }
    }

    #[allow(dead_code)]
    #[inline]
    pub fn route(&self, path: &str, func: fn(Router)) {
        let r = Router {
            top: self.top.clone(),
            routes: None,
            path: self.push_path(path),
            middleware: self.middleware.clone(),
        };

        func(r);
    }

    #[allow(dead_code)]
    #[inline]
    pub fn require(&mut self, middleware: MiddlewareFn) -> &mut Self {
        self.require_raw(Arc::new(middleware))
    }

    #[allow(dead_code)]
    #[inline]
    pub fn require_raw(&mut self, middleware: Middleware) -> &mut Self {
        self.middleware.push(middleware);
        self
    }

    #[allow(dead_code)]
    #[inline]
    pub fn get(&mut self, path: &str, handler: HandlerFn) -> &mut Self {
        self.handle(Method::GET, path, Arc::new(handler))
    }

    #[allow(dead_code)]
    #[inline]
    pub fn put(&mut self, path: &str, handler: HandlerFn) -> &mut Self {
        self.handle(Method::PUT, path, Arc::new(handler))
    }

    #[allow(dead_code)]
    #[inline]
    pub fn post(&mut self, path: &str, handler: HandlerFn) -> &mut Self {
        self.handle(Method::POST, path, Arc::new(handler))
    }

    #[allow(dead_code)]
    #[inline]
    pub fn delete(&mut self, path: &str, handler: HandlerFn) -> &mut Self {
        self.handle(Method::DELETE, path, Arc::new(handler))
    }

    #[allow(dead_code)]
    #[inline]
    pub fn patch(&mut self, path: &str, handler: HandlerFn) -> &mut Self {
        self.handle(Method::PATCH, path, Arc::new(handler))
    }

    #[allow(dead_code)]
    #[inline]
    pub fn head(&mut self, path: &str, handler: HandlerFn) -> &mut Self {
        self.handle(Method::HEAD, path, Arc::new(handler))
    }

    #[allow(dead_code)]
    #[inline]
    pub fn handle(&mut self, method: Method, path: &str, handler: Handler) -> &mut Self {
        self.add_route(method, self.push_path(path).unwrap(), handler)
    }

    pub fn find<P>(&self, method: &Method, path: P) -> Option<(RouteHandler, HashMap<String, String>)>
        where
            String: From<P>
    {
        match self.routes.as_ref() {
            Some(routes) => {
                let method = method.as_str();
                let path = path_into_trimmed_string(path);
                let path_parts: Vec<&str> = path.split("/")
                    .filter(|p| { !p.is_empty() })
                    .collect();

                let mut handler: Option<&RouteHandler> = None;
                let mut captures: HashMap<String, String> = HashMap::new();

                for (_unique, cur) in routes.into_iter() {
                    if cur.method.ne(method) {
                        continue;
                    }
                    if cur.tokens.len() != path_parts.len() {
                        continue;
                    }

                    let mut skipped = false;
                    let mut cur_i: usize = 0;
                    for token in cur.tokens.iter() {
                        match path_parts.get(cur_i) {
                            Some(p) => {
                                match token {
                                    RouteHandlerToken::Path { value } => {
                                        if value.ne(p) {
                                            skipped = true;
                                            break;
                                        }
                                    }
                                    RouteHandlerToken::Capture { name } => {
                                        captures.insert(name.clone(), p.to_string());
                                    }
                                }
                            }
                            None => {
                                skipped = true;
                                break;
                            }
                        }
                        cur_i += 1;
                    }
                    if skipped {
                        captures.clear();
                        continue;
                    }

                    handler = Some(cur);
                    break;
                }

                if let Some(handler) = handler {
                    return Some((handler.clone(), captures));
                }

                return None;
            }
            _ => {
                match self.top.as_ref() {
                    Some(top) => {
                        match top.write() {
                            Ok(top) => {
                                top.find(method, path)
                            }
                            Err(e) => {
                                unreachable!("Route failed to get top write lock!: {}", e);
                            }
                        }
                    }
                    _ => {
                        unreachable!("Invalid state: Route with no routes or top!");
                    }
                }
            }
        }
    }

    // private

    #[inline]
    fn push_path(&self, path: &str) -> Option<PathBuf> {
        match self.path.as_ref() {
            Some(p) => {
                // Sub paths need to be relative.
                let path = path.strip_prefix("/").unwrap();
                let mut new_path = p.clone();
                new_path.push(path);
                Some(new_path)
            }
            None => Some(PathBuf::from(path))
        }
    }

    fn add_route(&mut self, method: Method, path: PathBuf, handler: Handler) -> &mut Self {
        match self.top.as_ref() {
            Some(top) => {
                match top.write() {
                    Ok(mut top) => {
                        top.add_route_from_top(method, path, handler,
                                               self.middleware.clone());
                    }
                    Err(e) => {
                        unreachable!("Route failed to get top write lock!: {}", e);
                    }
                }
            }
            None => {
                unreachable!("Invalid state: Route with no routes or top!");
            }
        }

        self
    }

    fn add_route_from_top(&mut self, method: Method, path: PathBuf,
                          handler: Handler, middleware: Vec<Middleware>,
    ) -> &mut Self {
        if self.top.is_some() {
            unreachable!("Cannot call add_route_from_top unless top.")
        }

        match self.routes.as_mut() {
            Some(routes) => {
                let path = path.to_str().unwrap();
                let route_handler =
                    RouteHandler::new(method.clone(), path, handler,
                                      middleware);

                match routes.get(&route_handler.unique) {
                    None => {
                        routes.insert(route_handler.unique.clone(), route_handler);

                        debug!("???? added route: {} {}", method, path);
                    }
                    Some(_) => {
                        panic!("duplicate route detected: {}", route_handler.unique);
                    }
                };
            }
            _ => {
                unreachable!("Invalid state: top Route with no routes vec!");
            }
        }

        self
    }
}

#[derive(Clone)]
pub(crate) struct RouteHandler {
    unique: String,
    method: Method,
    tokens: Vec<RouteHandlerToken>,
    handler: Handler,
    middleware: Arc<Vec<Middleware>>,
}

impl RouteHandler {
    #[inline]
    fn new<P>(method: Method, path: P, handler: Handler, middleware: Vec<Middleware>) -> Self
        where
            String: From<P>
    {
        let (unique, tokens) =
            extract_route_handler_tokens(method.clone(), path);

        Self {
            unique,
            method,
            tokens,
            handler,
            middleware: Arc::new(middleware),
        }
    }

    #[inline]
    async fn route(&self, ctx: &mut Context, res: &mut Response) -> Result<(), Error> {
        if self.middleware.len() > 0 {
            _invoke_middleware(ctx, res, self.middleware.clone(), 0,
                               self.handler.clone()).await
        } else {
            self.handler.clone()(ctx, res).await
        }
    }
}

#[inline]
fn _invoke_middleware<'a>(
    ctx: &'a mut Context,
    res: &'a mut Response,
    middleware: Arc<Vec<Middleware>>,
    level: usize,
    handler: Handler,
) -> BoxFuture<'a, Result<(), Error>> {
    Box::pin(async move {
        let cur = middleware.get(level).unwrap();
        let last = level + 1 >= middleware.len();
        let middleware = middleware.clone();
        let handler = handler.clone();

        cur(
            ctx,
            res,
            Arc::new(move |ctx, res| {
                let middleware = middleware.clone();
                let handler = handler.clone();

                Box::pin(async move {
                    if last {
                        handler(ctx, res).await
                    } else {
                        _invoke_middleware(ctx, res, middleware, level + 1,
                                           handler.clone()).await
                    }
                })
            }),
        ).await
    })
}

#[derive(Clone)]
enum RouteHandlerToken {
    Path { value: String },
    Capture { name: String },
}

fn extract_route_handler_tokens<P>(method: Method, path: P) -> (String, Vec<RouteHandlerToken>)
    where
        String: From<P>
{
    let path = path_into_trimmed_string(path);
    let mut tokens: Vec<RouteHandlerToken> = Vec::new();
    let mut key_parts: Vec<String> = Vec::new();
    key_parts.push(method.to_string());

    for part in path.split("/").into_iter() {
        if part.is_empty() { continue; }
        let part = part.to_string();
        if part.starts_with(":") {
            key_parts.push(CAPTURE_PLACEHOLDER.to_string());
            tokens.push(RouteHandlerToken::Capture {
                name: part.strip_prefix(":").unwrap().to_string()
            });
        } else {
            key_parts.push(part.clone());
            tokens.push(RouteHandlerToken::Path {
                value: part
            });
        }
    }

    (key_parts.join("/"), tokens)
}

#[inline]
pub fn path_into_trimmed_string<P>(path: P) -> String
    where
        String: From<P>
{
    let mut path: String = path.into();
    if path.starts_with("/") {
        path = path.strip_prefix("/")
            .unwrap()
            .to_string()
    }
    path
}