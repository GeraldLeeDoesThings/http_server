use std::{collections::HashMap, fmt::Display, str::Split};

use crate::{
    connection::Connection,
    handler::{AnyHandler, AsyncHandler, Handler},
    header::Header,
    method_multi_map::MethodMultiMap,
    request::{Method, Request},
    response::{MaybeResponse, Response, ResponseCode},
};

pub struct PathParameterRouter {
    label: String,
    router: BaseRouter,
}

impl PathParameterRouter {
    fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            router: BaseRouter::new(),
        }
    }

    fn consume_path_param(&mut self, value: &str, request: &mut Request) -> &mut BaseRouter {
        request
            .get_path_parameters_mut()
            .insert(self.label.clone(), value.to_string());
        &mut self.router
    }

    const fn get_router_mut(&mut self) -> &mut BaseRouter {
        &mut self.router
    }
}

pub struct BaseRouter {
    sub_routers: HashMap<String, Box<Self>>,
    handler_map: MethodMultiMap<AnyHandler>,
    wildcard: Option<Box<PathParameterRouter>>,
}

enum ResolvedPath<'a> {
    Success(&'a mut AnyHandler),
    PathResolutionFailed,
    NoHandlerForMethod,
}

impl Default for BaseRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> BaseRouter {
    pub fn new() -> Self {
        Self {
            sub_routers: HashMap::new(),
            handler_map: MethodMultiMap::new(),
            wildcard: None,
        }
    }

    pub fn route(&mut self, connection: &mut Connection, request: &mut Request) -> MaybeResponse {
        self.route_from_path(
            connection,
            request,
            &mut request.get_target().clone().split('/'),
        )
    }

    fn resolve_route_mut(
        &'_ mut self,
        path: &mut Split<'a, char>,
        request: &mut Request,
    ) -> ResolvedPath<'_> {
        match path.next() {
            Some(next) => self
                .sub_routers
                .get_mut(next)
                .map(|router| router.resolve_route_mut(path, request))
                .unwrap_or_else(|| {
                    self.wildcard
                        .as_mut()
                        .map(|path_router| {
                            path_router
                                .consume_path_param(next, request)
                                .resolve_route_mut(path, request)
                        })
                        .unwrap_or(ResolvedPath::PathResolutionFailed)
                }),
            None => self
                .handler_map
                .map_mut(request.get_method())
                .map_or(ResolvedPath::NoHandlerForMethod, |handler| {
                    ResolvedPath::Success(handler)
                }),
        }
    }

    pub fn register_handler<T: Handler + Send + 'static>(
        &mut self,
        handler: T,
        methods: &[Method],
    ) {
        self.handler_map
            .insert(AnyHandler::from_handler(handler), methods);
    }

    pub fn register_async_handler<T: AsyncHandler + Send + 'static>(
        &mut self,
        handler: T,
        methods: &[Method],
    ) {
        self.handler_map
            .insert(AnyHandler::from_async_handler(handler), methods);
    }

    pub fn create_route(&mut self, path: &mut Split<'a, char>) -> &mut Self {
        if let Some(next) = path.next() {
            if next.starts_with('{') && next.ends_with('}') {
                assert!(next.len() >= 2);
                self.wildcard = Some(Box::new(PathParameterRouter::new(
                    next.get(1..next.len() - 1)
                        .expect("Path parameter index invalid despite length check."),
                )));
                println!("Adding wildcard: {}", next);
                return self
                    .wildcard
                    .as_mut()
                    .expect("Wildcard router missing despite just assigning one.")
                    .get_router_mut()
                    .create_route(path);
            } else if !self.sub_routers.contains_key(next) {
                assert!(
                    self.sub_routers
                        .insert(next.to_string(), Box::new(Self::new()))
                        .is_none(),
                    "Router added between checks."
                )
            }
            self.sub_routers
                .get_mut(next)
                .expect("Router should have just been inserted, or already present.")
                .create_route(path)
        } else {
            self
        }
    }

    pub fn register_handler_from_path<T: Handler + Send + 'static>(
        &mut self,
        handler: T,
        path: &str,
        methods: &[Method],
    ) {
        self.create_route(&mut path.split('/'))
            .register_handler(handler, methods);
    }

    pub fn register_async_handler_from_path<T: AsyncHandler + Send + 'static>(
        &mut self,
        handler: T,
        path: &str,
        methods: &[Method],
    ) {
        self.create_route(&mut path.split('/'))
            .register_async_handler(handler, methods)
    }

    fn route_from_path(
        &mut self,
        connection: &mut Connection,
        request: &'a mut Request,
        path: &mut Split<'a, char>,
    ) -> MaybeResponse {
        match self.resolve_route_mut(path, request) {
            ResolvedPath::Success(AnyHandler::Sync(handler)) => {
                MaybeResponse::Now(handler.handle(connection, request))
            }
            ResolvedPath::Success(AnyHandler::Async(async_handler)) => {
                MaybeResponse::Later(async_handler.handle(connection, request))
            }
            ResolvedPath::PathResolutionFailed => MaybeResponse::Now(Response::new(
                ResponseCode::NotFound,
                request.get_protocol(),
            )),
            ResolvedPath::NoHandlerForMethod => {
                let mut response =
                    Response::new(ResponseCode::MethodNotAllowed, request.get_protocol());
                response.get_headers_mut().insert(
                    Header::Allow,
                    self.handler_map
                        .iter_mapped_methods()
                        .map(|method| method.as_str())
                        .intersperse(", ")
                        .collect(),
                );
                MaybeResponse::Now(response)
            }
        }
    }
}

impl Display for BaseRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.handler_map.iter_mapped_methods().count() > 0 {
            writeln!(f, "Handled")?;
        } else {
            writeln!(f, "Default")?;
        }
        for (name, router) in &self.sub_routers {
            write!(f, "{} -> {}", name, router)?;
        }
        Ok(())
    }
}
