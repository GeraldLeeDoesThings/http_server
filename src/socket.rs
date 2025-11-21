use std::net::Ipv4Addr;

use libc::{
    AF_INET, EBADF, EFAULT, EINVAL, ENOTSOCK, EOPNOTSUPP, SOCK_NONBLOCK, SOCK_STREAM, in_port_t,
    sa_family_t, sockaddr_in,
};
use syscalls::{Errno, Sysno, syscall};

use crate::error_utils::MaybeFatal;

pub struct Socket {
    file_descriptor: usize,
    address_descriptor: sockaddr_in,
    listening: bool,
}

#[derive(Debug)]
pub enum SocketCreateError {
    DescriptorCreationFailed(Errno),
    BindingFailed(Errno),
}

#[derive(Debug)]
pub enum SocketListeningError {
    AlreadyListening,
    ListeningFailed(Errno),
}

#[derive(Debug)]
pub enum SocketAcceptError {
    NotListening,
    AcceptFailed(Errno),
}

impl MaybeFatal for SocketAcceptError {
    fn is_fatal(&self) -> bool {
        match self {
            Self::NotListening => false,
            Self::AcceptFailed(errno) => matches!(
                errno.into_raw(),
                EBADF | EFAULT | EINVAL | ENOTSOCK | EOPNOTSUPP
            ),
        }
    }
}

impl Socket {
    pub fn new(port: in_port_t, address: Ipv4Addr) -> Result<Self, SocketCreateError> {
        let address_descriptor: sockaddr_in = sockaddr_in {
            sin_family: AF_INET as sa_family_t,
            sin_port: port.to_be(),
            sin_addr: libc::in_addr {
                s_addr: address.to_bits().to_be(),
            },
            sin_zero: [0; 8],
        };

        let file_descriptor: usize =
            unsafe { syscall!(Sysno::socket, AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0) }
                .map_err(SocketCreateError::DescriptorCreationFailed)?;

        unsafe {
            syscall!(
                Sysno::bind,
                file_descriptor,
                &address_descriptor as *const _ as usize,
                size_of::<sockaddr_in>()
            )
        }
        .map_err(SocketCreateError::BindingFailed)?;

        Ok(Self {
            file_descriptor,
            address_descriptor,
            listening: false,
        })
    }

    pub fn start_listening(&mut self) -> Result<(), SocketListeningError> {
        if self.listening {
            return Err(SocketListeningError::AlreadyListening);
        }

        let result = unsafe { syscall!(Sysno::listen, self.file_descriptor, 64) }
            .map_err(SocketListeningError::ListeningFailed)
            .map(|_| ());
        if result.is_ok() {
            self.listening = true;
        }
        result
    }

    pub fn accept_connection(&mut self) -> Result<usize, SocketAcceptError> {
        if !self.listening {
            return Err(SocketAcceptError::NotListening);
        }

        let result = unsafe { syscall!(Sysno::accept4, self.file_descriptor, 0, 0, SOCK_NONBLOCK) }
            .map_err(SocketAcceptError::AcceptFailed);

        if let Err(ref err) = result
            && err.is_fatal()
        {
            self.listening = false;
        }

        result
    }

    pub const fn get_address_descriptor(&self) -> &sockaddr_in {
        &self.address_descriptor
    }

    pub const fn is_listening(&self) -> bool {
        self.listening
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        unsafe {
            let _ = syscall!(Sysno::close, self.file_descriptor);
        }
    }
}
