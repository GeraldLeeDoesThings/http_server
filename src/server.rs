use crate::{
    connection::Connection,
    error_utils::MaybeFatal,
    socket::{Socket, SocketAcceptError, SocketListeningError},
};

pub struct HTTPServer {
    socket: Socket,
    connections: Vec<Connection>,
}

#[derive(Debug)]
pub enum HTTPServerRunError {
    SocketListeningError(SocketListeningError),
    SocketAcceptError(SocketAcceptError),
}

impl MaybeFatal for HTTPServerRunError {
    fn is_fatal(&self) -> bool {
        match self {
            Self::SocketListeningError(socket_listening_error) => matches!(
                socket_listening_error,
                SocketListeningError::ListeningFailed(_)
            ),
            Self::SocketAcceptError(socket_accept_error) => socket_accept_error.is_fatal(),
        }
    }
}

impl HTTPServer {
    pub const fn new(socket: Socket) -> Self {
        Self {
            socket,
            connections: Vec::new(),
        }
    }

    pub fn run(&mut self) -> HTTPServerRunError {
        if !self.socket.is_listening()
            && let Err(err) = self.socket.start_listening()
        {
            return HTTPServerRunError::SocketListeningError(err);
        }
        loop {
            if let Err(err) = self.accept_connections()
                && err.is_fatal()
            {
                return err;
            }
            for connection in &mut self.connections {
                if let Ok(input_vec) = connection.read()
                    && !input_vec.is_empty()
                {
                    println!("Received: {:#?}", input_vec);
                }
            }
        }
    }

    pub fn accept_connections(&mut self) -> Result<(), HTTPServerRunError> {
        match self.socket.accept_connection() {
            Ok(descriptor) => {
                self.connections.push(Connection::new(descriptor));
                println!("Established new connection.");
                Ok(())
            }
            Err(err) => Err(HTTPServerRunError::SocketAcceptError(err)),
        }
    }
}
