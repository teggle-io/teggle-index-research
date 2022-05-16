use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::borrow::Borrow;

use http::{Method, StatusCode};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::SgxRwLock;

use api::handler::request::Request;
use api::handler::response::Response;
use api::handler::types::ApiError;

type Handler = fn(&Request, &mut Response, Vec<(String, String)>) -> Result<(), ApiError>;

const CAPTURE_PLACEHOLDER: &'static str = "*CAPTURE*";

pub(crate) fn route_request(req: &Request, res: &mut Response) -> Result<(), ApiError> {
    let route = format!("{}{}", req.method(), req.uri().path());
    let mut routes: path_router::Tree<Handler> = path_router::Tree::new();
    routes.add("GET/ping", |_req, res, _captures| {
        res.ok("PONG");
        Ok(())
    });

    let r = Router::new();
    r.route("/test", |mut r| {
        r.get("/ping", |_req, res, _captures| {
            res.ok("PONG");
            Ok(())
        });
    });

    match routes.find(&route) {
        Some((handler, captures)) => {
            handler(&req, res, captures)
        }
        None => {
            res.error(StatusCode::NOT_FOUND, "Not Found");

            Ok(())
        }
    }
}

pub(crate) struct Router {
    top: Option<Arc<SgxRwLock<Router>>>,
    routes: Option<HashMap<String, RouteHandler>>,
    path: Option<PathBuf>,
}

impl Router {
    pub(crate) fn new() -> Self {
        let top = Self {
            top: None,
            routes: Some(HashMap::new()),
            path: None,
        };

        Self {
            top: Some(Arc::new(SgxRwLock::new(top))),
            routes: None,
            path: None,
        }
    }

    pub fn route(&self, path: &str, func: fn(Router)) {
        let r = Router {
            top: self.top.clone(),
            routes: None,
            path: self.push_path(path),
        };

        func(r);
    }

    pub fn get(&mut self, path: &str, handler: Handler) {
        self.add_route(Method::GET, self.push_path(path).unwrap(), handler)
    }

    pub fn find<P>(&mut self, method: Method, path: P) -> Option<(Handler, HashMap<String, String>)>
        where
            String: From<P>
    {
        match self.routes.as_mut() {
            Some(routes) => {
                let method = method.as_str();
                let path: String = path.into();
                let mut path_parts: Vec<&str> = path.split("/").collect();
                let path_parts_len = path_parts.len();

                let mut handler: Option<Handler> = None;
                let mut captures: HashMap<String, String> = HashMap::new();

                for (_unique, cur) in routes.into_iter() {
                    if cur.method.ne(method) {
                        continue;
                    }

                    let cur_count = cur.tokens.len();
                    if cur_count != path_parts_len {
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

                    handler = Some(cur.handler);
                    break;
                }

                match handler {
                    Some(handler) => {
                        Some((handler, captures))
                    }
                    None => None
                }
            }
            None => {
                match self.top.as_ref() {
                    Some(top) => {
                        match top.write() {
                            Ok(mut top) => {
                                top.find(method, path)
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
            }
        }
    }

    // private

    fn push_path(&self, path: &str) -> Option<PathBuf> {
        match self.path.as_ref() {
            Some(p) => Some(p.clone()),
            None => Some(PathBuf::from(path))
        }
    }

    fn add_route(&mut self, method: Method, path: PathBuf, handler: Handler) {
        match self.routes.as_mut() {
            Some(routes) => {
                let path = path.to_str().unwrap();
                let route_handler =
                    RouteHandler::new(method.clone(), path, handler);

                match routes.get(&route_handler.unique) {
                    None => {
                        routes.insert(route_handler.unique.clone(), route_handler);

                        debug!("🔄 added route: {} {}", method, path);
                    }
                    Some(_) => {
                        panic!("duplicate route detected: {}", route_handler.unique);
                    }
                }
            }
            None => {
                match self.top.as_ref() {
                    Some(top) => {
                        match top.write() {
                            Ok(mut top) => {
                                top.add_route(method, path, handler);
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
            }
        }
    }
}

struct RouteHandler {
    unique: String,
    method: Method,
    tokens: Vec<RouteHandlerToken>,
    handler: Handler,
}

impl RouteHandler {
    fn new<P>(method: Method, path: P, handler: Handler) -> Self
        where
            String: From<P>
    {
        let (unique, tokens) =
            extract_route_handler_tokens(method.clone(), path);

        Self {
            unique, method, tokens, handler
        }
    }
}

enum RouteHandlerToken {
    Path { value: String },
    Capture { name: String },
}

fn extract_route_handler_tokens<P>(method: Method, path: P) -> (String, Vec<RouteHandlerToken>)
    where
        String: From<P>
{
    let path: String = path.into();
    let mut key_parts: Vec<String> = Vec::new();
    let mut tokens: Vec<RouteHandlerToken> = Vec::new();

    key_parts.push(method.to_string());

    for part in path.split("/").into_iter() {
        let part = part.to_string();
        if part.starts_with(":") {
            key_parts.push(CAPTURE_PLACEHOLDER.to_string());
            tokens.push(RouteHandlerToken::Capture {
                name: path.strip_prefix(":").unwrap().to_string()
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