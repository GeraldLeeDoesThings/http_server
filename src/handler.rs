use serde::de::DeserializeOwned;
use tokio::task::JoinHandle;

use crate::{
    connection::Connection,
    request::Request,
    response::{Response, ResponseCode},
};

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
            Self::Async(async_handler) => {
                Some(async_handler.handle(connection, request).await.unwrap())
            }
        }
    }
}

pub trait Handler: Send {
    fn handle(&mut self, connection: &mut Connection, request: &Request) -> Response;
}

pub trait AsyncHandler: Send {
    fn handle(&mut self, connection: &mut Connection, request: &Request) -> JoinHandle<Response>;
}

#[macro_export]
macro_rules! impl_handler_for_schema_handler {
    ($type:ty,$data:ty) => {
        impl Handler for $type
        where
            $type: SchemaHandler<$data, Response>,
        {
            fn handle(&mut self, connection: &mut Connection, request: &Request) -> Response {
                let data: $data = match serde_json::from_str(request.get_content().as_str()) {
                    Ok(data) => data,
                    Err(error) => {
                        let mut response =
                            Response::new(ResponseCode::BadRequest, request.get_protocol());
                        response.set_content(format!("{}", error).into());
                        return response;
                    }
                };
                self.handle_schema(connection, request, data)
            }
        }
    };
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

pub trait SchemaHandler<D: DeserializeOwned, R>: Send {
    fn handle_schema(&mut self, connection: &mut Connection, request: &Request, data: D) -> R;
}

// impl_handler_for_schema_handler!(ConstantHandler);

/*
impl<'a, Data: DeserializeOwned, T: SchemaHandler<Data, Response>> Handler for T {
    fn handle(&mut self, connection: &mut Connection, request: &Request) -> Response {
        let data: Data = match serde_json::from_str(request.get_content().as_str()) {
            Ok(data) => data,
            Err(error) => {
                let mut response = Response::new(ResponseCode::BadRequest, request.get_protocol());
                response.set_content(format!("{}", error).into());
                return response;
            }
        };
        self.handle_schema(connection, request, data)
    }
}
*/

impl<'a, Data: DeserializeOwned> AsyncHandler
    for dyn SchemaHandler<Data, JoinHandle<Response>> + 'a
{
    fn handle(&mut self, connection: &mut Connection, request: &Request) -> JoinHandle<Response> {
        let data: Data = match serde_json::from_str(request.get_content().as_str()) {
            Ok(data) => data,
            Err(error) => {
                let mut response = Response::new(ResponseCode::BadRequest, request.get_protocol());
                response.set_content(format!("{}", error).into());
                return tokio::spawn(async { response });
            }
        };
        self.handle_schema(connection, request, data)
    }
}
