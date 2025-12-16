use std::{collections::HashMap, fmt::Display, str::Split};

use crate::{
    connection::Connection,
    handler::Handler,
    request::Request,
    response::{Response, ResponseCode},
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
    handler: Option<Box<dyn Handler>>,
    wildcard: Option<Box<PathParameterRouter>>,
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
            handler: None,
            wildcard: None,
        }
    }

    pub fn route(&mut self, connection: &mut Connection, request: &mut Request) -> Response {
        self.route_from_path(
            connection,
            request,
            &mut request.get_target().clone().split('/'),
        )
    }

    fn resolve_route_mut(
        &mut self,
        path: &mut Split<'a, char>,
        request: &mut Request,
    ) -> Option<&mut Self> {
        match path.next() {
            Some(next) => self
                .sub_routers
                .get_mut(next)
                .and_then(|router| router.resolve_route_mut(path, request))
                .or_else(|| {
                    self.wildcard.as_mut().and_then(|path_router| {
                        path_router
                            .consume_path_param(next, request)
                            .resolve_route_mut(path, request)
                    })
                }),
            None => Some(self),
        }
    }

    pub fn register_handler<T: Handler + 'static>(
        &mut self,
        handler: T,
    ) -> Option<Box<dyn Handler>> {
        self.handler.replace(Box::new(handler))
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

    pub fn register_handler_from_path<T: Handler + 'static>(&mut self, handler: T, path: &str) {
        self.create_route(&mut path.split('/'))
            .register_handler(handler);
    }

    fn route_from_path(
        &mut self,
        connection: &mut Connection,
        request: &'a mut Request,
        path: &mut Split<'a, char>,
    ) -> Response {
        self.resolve_route_mut(path, request)
            .and_then(|router| {
                router
                    .handler
                    .as_mut()
                    .map(|handler| handler.handle(connection, request))
            })
            .unwrap_or_else(|| Response::new(ResponseCode::NotFound, request.get_protocol()))
    }
}

impl Display for BaseRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.handler {
            Some(_) => writeln!(f, "Handled")?,
            None => writeln!(f, "Default")?,
        }
        for (name, router) in &self.sub_routers {
            write!(f, "{} -> {}", name, router)?;
        }
        Ok(())
    }
}
