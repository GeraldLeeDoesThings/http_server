use libc::{EBADF, EDESTADDRREQ, EDQUOT, EFAULT, EFBIG, EINVAL, EIO, EISDIR, ENOSPC, EPERM, EPIPE};
use syscalls::{Errno, Sysno, syscall};

use crate::{
    error_utils::MaybeFatal,
    request::{Request, RequestParseError},
};

const BUFFER_SIZE: usize = 256;

pub struct Connection {
    descriptor: usize,
    buffer: [u8; BUFFER_SIZE],
    state: ConnectionStatus,
    collector: String,
    write_index: usize,
}

#[derive(Clone, Debug)]
pub enum ConnectionReadError {
    ReadError(Errno),
    NotReadyToRead(ConnectionStatus),
    MalformedRequest(RequestParseError),
}

pub enum ConnectionResponseError {
    NotReadyToRespond(ConnectionStatus),
}

#[derive(Clone, Copy, Debug)]
pub enum ConnectionWriteError {
    WriteError(Errno),
    NotReadyToWrite(ConnectionStatus),
}

#[derive(Clone, Copy, Debug)]
pub enum ConnectionStatus {
    Reading,
    AwaitingResponse,
    Writing,
    Dead,
}

impl MaybeFatal for ConnectionReadError {
    fn is_fatal(&self) -> bool {
        match self {
            Self::ReadError(errno) => {
                matches!(errno.into_raw(), EBADF | EFAULT | EINVAL | EIO | EISDIR)
            }
            Self::NotReadyToRead(_) => true,
            Self::MalformedRequest(_) => false,
        }
    }
}

impl MaybeFatal for ConnectionWriteError {
    fn is_fatal(&self) -> bool {
        match self {
            Self::WriteError(errno) => {
                matches!(
                    errno.into_raw(),
                    EBADF
                        | EDESTADDRREQ
                        | EDQUOT
                        | EFAULT
                        | EFBIG
                        | EINVAL
                        | EIO
                        | ENOSPC
                        | EPERM
                        | EPIPE
                )
            }
            Self::NotReadyToWrite(state) => matches!(state, ConnectionStatus::Reading),
        }
    }
}

impl Connection {
    pub(crate) const fn new(descriptor: usize) -> Self {
        Self {
            descriptor,
            buffer: [0; BUFFER_SIZE],
            state: ConnectionStatus::Reading,
            collector: String::new(),
            write_index: 0,
        }
    }

    fn read_once(&mut self) -> Result<usize, ConnectionReadError> {
        unsafe {
            syscall!(
                Sysno::read,
                self.descriptor,
                &mut self.buffer as *mut _ as usize,
                BUFFER_SIZE
            )
        }
        .map_err(ConnectionReadError::ReadError)
        .inspect(|&count| {
            self.collector
                .push_str(&String::from_utf8_lossy(&self.buffer[0..count]));
        })
    }

    fn parse_request(&self) -> Result<Request, ConnectionReadError> {
        self.collector
            .as_str()
            .try_into()
            .map_err(ConnectionReadError::MalformedRequest)
    }

    pub fn read(&mut self) -> Result<Request, ConnectionReadError> {
        if !self.is_reading() {
            return Err(ConnectionReadError::NotReadyToRead(self.state));
        }

        let mut read_result = self.read_once();
        while let Ok(read_size) = read_result {
            if read_size == 0 {
                return self.parse_request();
            }
            read_result = self.read_once();
        }
        if read_result.as_ref().is_err_and(|err| err.is_fatal()) {
            self.kill();
        }
        if self.is_alive() && self.collector.ends_with("\r\n\r\n") {
            self.state = ConnectionStatus::AwaitingResponse;
            return self.parse_request();
        }
        match read_result {
            Ok(_) => {
                self.state = ConnectionStatus::AwaitingResponse;
                self.parse_request()
            }
            Err(err) => Err(err),
        }
    }

    fn write_once(&self) -> Result<usize, ConnectionWriteError> {
        unsafe {
            syscall!(
                Sysno::write,
                self.descriptor,
                self.collector[self.write_index..].as_ptr() as usize,
                self.collector.len() - self.write_index
            )
        }
        .map_err(ConnectionWriteError::WriteError)
    }

    pub fn begin_response(&mut self, data: &str) -> Result<(), ConnectionResponseError> {
        if !self.is_awaiting_response() {
            return Err(ConnectionResponseError::NotReadyToRespond(self.state));
        }
        self.collector.clear();
        self.collector.push_str(data);
        self.write_index = 0;
        self.state = ConnectionStatus::Writing;
        Ok(())
    }

    pub fn write(&mut self) -> Result<(), ConnectionWriteError> {
        if !self.is_writing() {
            return Err(ConnectionWriteError::NotReadyToWrite(self.state));
        }

        let mut write_result = self.write_once();
        while let Ok(count) = write_result
            && count > 0
        {
            self.write_index += count;
            write_result = self.write_once();
        }
        if write_result.is_err_and(|err| err.is_fatal()) || self.write_index >= self.collector.len()
        {
            self.kill();
        }
        write_result.map(|_| ())
    }

    pub const fn get_file_descriptor(&self) -> usize {
        self.descriptor
    }

    pub const fn is_alive(&self) -> bool {
        !matches!(self.state, ConnectionStatus::Dead)
    }

    pub const fn is_reading(&self) -> bool {
        matches!(self.state, ConnectionStatus::Reading)
    }

    pub const fn is_writing(&self) -> bool {
        matches!(self.state, ConnectionStatus::Writing)
    }

    pub const fn is_awaiting_response(&self) -> bool {
        matches!(self.state, ConnectionStatus::AwaitingResponse)
    }

    pub const fn kill(&mut self) {
        self.state = ConnectionStatus::Dead;
    }

    pub fn reset(&mut self) {
        self.collector.clear();
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            let _ = syscall!(Sysno::close, self.descriptor);
        }
    }
}
