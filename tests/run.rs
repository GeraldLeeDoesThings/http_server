use std::net::Ipv4Addr;

use http_server::{server::HTTPServer, socket::Socket};


#[test]
fn run_server() {
    let mut server = HTTPServer::new(
        Socket::new(5000, Ipv4Addr::new(127, 0, 0, 1)).unwrap()
    );
    server.run();
}
