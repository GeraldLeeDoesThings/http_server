use std::net::Ipv4Addr;

use http_server::{
    handler::ConstantHandler,
    response::{Response, ResponseCode},
    router::Router,
    server::HTTPServer,
    socket::Socket,
};

#[test]
fn run_server() {
    let mut router = Router::new();
    router.register_handler_from_path(
        ConstantHandler::new(Response::new(
            ResponseCode::NoContent,
            http_server::protocol::Protocol::Http1_0,
        )),
        "/hello",
    );
    println!("{}", router);
    let mut server = HTTPServer::new(
        Socket::new(5000, Ipv4Addr::new(127, 0, 0, 1)).unwrap(),
        router,
    );
    server.run();
}
