use libc::{EBADF, EFAULT, EINVAL, EIO, EISDIR};
use syscalls::{Errno, Sysno, syscall};

use crate::error_utils::MaybeFatal;

const BUFFER_SIZE: usize = 256;

pub struct Connection {
    descriptor: usize,
    buffer: [u8; BUFFER_SIZE],
}

#[derive(Clone, Copy, Debug)]
pub enum ConnectionReadError {
    ReadError(Errno),
}

impl MaybeFatal for ConnectionReadError {
    fn is_fatal(&self) -> bool {
        match self {
            Self::ReadError(errno) => {
                matches!(errno.into_raw(), EBADF | EFAULT | EINVAL | EIO | EISDIR)
            }
        }
    }
}

impl Connection {
    pub(crate) const fn new(descriptor: usize) -> Self {
        Self {
            descriptor,
            buffer: [0; BUFFER_SIZE],
        }
    }

    fn read_once(&mut self) -> Result<&[u8], ConnectionReadError> {
        unsafe {
            syscall!(
                Sysno::read,
                self.descriptor,
                &mut self.buffer as *mut _ as usize,
                BUFFER_SIZE
            )
        }
        .map_err(ConnectionReadError::ReadError)
        .map(|count| &self.buffer[0..count])
    }

    pub fn read(&mut self) -> Result<Vec<u8>, ConnectionReadError> {
        let mut collected: Vec<u8> = Vec::new();
        let mut read_result = self.read_once();
        while let Ok(full_buffer) = read_result
            && !full_buffer.is_empty()
        {
            collected.extend(full_buffer);
            read_result = self.read_once();
        }
        if read_result.is_err_and(|err| err.is_fatal()) {
            return read_result.map(|_| collected);
        }
        Ok(collected)
    }
}
