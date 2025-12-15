use crate::{connection::Connection, request::Request, response::Response};

pub trait Handler {
    fn handle(&mut self, connection: &mut Connection, request: &Request) -> Response;
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
