use std::net::Ipv4Addr;

use http_server::{
    handler::Handler,
    response::{Response, ResponseCode},
    router::BaseRouter,
    server::HTTPServer,
    socket::Socket,
};

struct EchoHandler {}

impl Handler for EchoHandler {
    fn handle(
        &mut self,
        _connection: &mut http_server::connection::Connection,
        request: &http_server::request::Request,
    ) -> Response {
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

#[test]
fn run_server() {
    let mut router = BaseRouter::new();
    router.register_handler_from_path(EchoHandler {}, "/hello/{foo}/bar/baz/{biz}");
    println!("{}", router);
    let mut server = HTTPServer::new(
        Socket::new(5000, Ipv4Addr::new(127, 0, 0, 1)).unwrap(),
        router,
    );
    server.run();
}
