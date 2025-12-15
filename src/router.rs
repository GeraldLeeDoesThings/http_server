use std::{collections::HashMap, fmt::Display, str::Split};

use crate::{
    connection::Connection,
    handler::Handler,
    request::Request,
    response::{Response, ResponseCode},
};

pub struct Router {
    sub_routers: HashMap<String, Box<Self>>,
    handler: Option<Box<dyn Handler>>,
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Router {
    pub fn new() -> Self {
        Self {
            sub_routers: HashMap::new(),
            handler: None,
        }
    }

    pub fn route(&mut self, connection: &mut Connection, request: &Request) -> Response {
        self.route_from_path(connection, request, &mut request.get_target().split('/'))
    }

    fn resolve_route_mut(&mut self, path: &mut Split<'a, char>) -> Option<&mut Self> {
        match path.next() {
            Some(next) => {
                self.sub_routers.get_mut(next).and_then(|router| router.resolve_route_mut(path))
            },
            None => {
                Some(self)
            },
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
            if !self.sub_routers.contains_key(next) {
                assert!(self.sub_routers.insert(next.to_string(), Box::new(Self::new())).is_none(), "Router added between checks.")
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
        request: &'a Request,
        path: &mut Split<'a, char>,
    ) -> Response {
        self.resolve_route_mut(path)
            .and_then(|router| {
                router
                    .handler
                    .as_mut()
                    .map(|handler| handler.handle(connection, request))
            })
            .unwrap_or_else(|| Response::new(ResponseCode::NotFound, request.get_protocol()))
    }
}

impl Display for Router {
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
