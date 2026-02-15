use crate::{connection::Connection, request::Request, response::Response};

pub enum AnyHandler {
    Sync(Box<dyn Handler>),
    Async(Box<dyn AsyncHandler>),
}

impl AnyHandler {
    pub fn from_handler<T: Handler + Send + 'static>(handler: T) -> Self {
        Self::Sync(Box::new(handler))
    }

    pub fn from_async_handler<T: AsyncHandler + Send + 'static>(handler: T) -> Self {
        Self::Async(Box::new(handler))
    }

    pub fn handle_sync(
        &mut self,
        connection: &mut Connection,
        request: &Request,
    ) -> Option<Response> {
        match self {
            Self::Sync(handler) => Some(handler.handle(connection, request)),
            Self::Async(_) => None,
        }
    }

    pub async fn handle_async(
        &mut self,
        connection: &mut Connection,
        request: &Request,
    ) -> Option<Response> {
        match self {
            Self::Sync(_) => None,
            Self::Async(async_handler) => Some(async_handler.handle(connection, request).await),
        }
    }
}

pub trait Handler: Send {
    fn handle(&mut self, connection: &mut Connection, request: &Request) -> Response;
}

pub trait AsyncHandler: Send {
    fn handle(
        &mut self,
        connection: &mut Connection,
        request: &Request,
    ) -> Box<dyn Future<Output = Response> + Send + Unpin>;
}

pub struct ConstantHandler {
    response: Response,
}

impl Handler for ConstantHandler {
    fn handle(&mut self, _connection: &mut Connection, _request: &Request) -> Response {
        self.response.clone()
    }
}

impl ConstantHandler {
    pub const fn new(response: Response) -> Self {
        Self { response }
    }
}
