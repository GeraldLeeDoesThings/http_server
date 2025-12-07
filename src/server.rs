use std::mem::MaybeUninit;

use libc::{FD_SET, FD_ZERO, fd_set};
use syscalls::syscall;

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
            self.wait_for_event();
            if let Err(err) = self.accept_connections()
                && err.is_fatal()
            {
                return err;
            }
            for connection in &mut self.connections {
                if connection.is_reading() {
                    if let Ok(request) = connection.read() {
                        println!("Received request:\n{}", request);
                    }
                } else if connection.is_awaiting_response() {
                    let _ = connection.begin_response("HTTP/1.1 204 OK\r\n\r\n");
                } else if connection.is_writing() {
                    let _ = connection.write();
                }
            }
            self.connections = self
                .connections
                .drain(..)
                .filter(|con| con.is_alive())
                .collect()
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

    fn wait_for_event(&self) {
        let mut write_file_descriptors = MaybeUninit::<fd_set>::uninit();
        let mut write_file_descriptors = unsafe {
            FD_ZERO(write_file_descriptors.as_mut_ptr());
            self.connections
                .iter()
                .filter(|con| con.is_awaiting_response() || con.is_writing())
                .for_each(|con| {
                    FD_SET(
                        con.get_file_descriptor()
                            .try_into()
                            .expect("File descriptor does not fit in an i32."),
                        write_file_descriptors.as_mut_ptr(),
                    )
                });
            write_file_descriptors.assume_init()
        };

        let mut read_file_descriptors = MaybeUninit::<fd_set>::uninit();
        let mut read_file_descriptors = unsafe {
            FD_ZERO(read_file_descriptors.as_mut_ptr());
            self.connections
                .iter()
                .filter(|con| con.is_reading())
                .for_each(|con| {
                    FD_SET(
                        con.get_file_descriptor()
                            .try_into()
                            .expect("File descriptor does not fit in an i32."),
                        read_file_descriptors.as_mut_ptr(),
                    );
                });
            FD_SET(
                self.socket
                    .get_file_descriptor()
                    .try_into()
                    .expect("File descriptor does not fit in an i32."),
                read_file_descriptors.as_mut_ptr(),
            );
            read_file_descriptors.assume_init()
        };

        let max_file_descriptor: i32 = (self
            .connections
            .iter()
            .map(|con| con.get_file_descriptor())
            .max()
            .unwrap_or(0)
            .max(self.socket.get_file_descriptor())
            + 1)
        .try_into()
        .expect("Max file descriptor does not fit in an i32.");

        unsafe {
            let _ = syscall!(
                syscalls::Sysno::select,
                max_file_descriptor,
                &mut read_file_descriptors as *mut _ as usize,
                &mut write_file_descriptors as *mut _ as usize,
                0,
                0
            );
        }
    }
}
