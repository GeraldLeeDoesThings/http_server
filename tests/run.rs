use std::net::Ipv4Addr;

use http_server::{
    connection::Connection,
    handler::{AsyncHandler, Handler, SchemaHandler},
    impl_handler_for_schema_handler,
    request::{Method, Request},
    response::{Response, ResponseCode},
    router::BaseRouter,
    server::HTTPServer,
    socket::Socket,
};
use serde::{Deserialize, Serialize};
use tokio::{
    spawn,
    time::{Duration, sleep},
};

struct EchoHandler {}

impl Handler for EchoHandler {
    fn handle(&mut self, _connection: &mut Connection, request: &Request) -> Response {
        let mut response =
            Response::new(ResponseCode::Ok, http_server::protocol::Protocol::Http1_0);
        let content = format!(
            "foo: {}\nbiz: {}\n",
            request
                .get_path_parameters()
                .get("foo")
                .unwrap_or(&"Missing".to_string()),
            request
                .get_path_parameters()
                .get("biz")
                .unwrap_or(&"Missing".to_string())
        );
        response.set_content(Some(content));
        response.get_headers_mut().insert(
            http_server::header::Header::ContentType,
            "text/plain".to_string(),
        );
        response
    }
}

struct SlowEchoHandler {
    echo: EchoHandler,
}

impl AsyncHandler for SlowEchoHandler {
    fn handle(
        &mut self,
        connection: &mut Connection,
        request: &Request,
    ) -> tokio::task::JoinHandle<Response> {
        let response = self.echo.handle(connection, request);
        spawn(async move {
            sleep(Duration::from_secs(1)).await;
            response
        })
    }
}

#[derive(Deserialize, Serialize)]
struct SimpleSchema {
    foo: String,
    bar: usize,
}

struct DoubleHandler {}

impl SchemaHandler<SimpleSchema, Response> for DoubleHandler {
    fn handle_schema(
        &mut self,
        _connection: &mut Connection,
        request: &Request,
        data: SimpleSchema,
    ) -> Response {
        let response_json: SimpleSchema = SimpleSchema {
            foo: data.foo.repeat(2),
            bar: data.bar * 2,
        };
        Response::from_json(ResponseCode::Ok, request.get_protocol(), &response_json).unwrap()
    }
}

impl_handler_for_schema_handler!(DoubleHandler, SimpleSchema);

#[tokio::test]
async fn run_server() {
    let mut router = BaseRouter::new();
    router.register_handler_from_path(EchoHandler {}, "/hello/{foo}/bar/baz/{biz}", &[Method::Get]);
    router.register_handler_from_path(DoubleHandler {}, "/double", &[Method::Get]);
    router.register_async_handler_from_path(
        SlowEchoHandler {
            echo: EchoHandler {},
        },
        "/slow/{foo}",
        &[Method::Get],
    );
    println!("{}", router);
    let mut server = HTTPServer::new(
        Socket::new(5000, Ipv4Addr::new(127, 0, 0, 1)).unwrap(),
        router,
    );
    server.run().await;
}
