use libc::{EBADF, EDESTADDRREQ, EDQUOT, EFAULT, EFBIG, EINVAL, EIO, EISDIR, ENOSPC, EPERM, EPIPE};
use syscalls::{Errno, Sysno, syscall};

use crate::{
    error_utils::MaybeFatal,
    request::{Request, RequestFactory, RequestParseError},
    response::Response,
};

const BUFFER_SIZE: usize = 256;

pub struct Connection {
    descriptor: usize,
    buffer: [u8; BUFFER_SIZE],
    state: ConnectionStatus,
    collector: String,
    write_index: usize,
    request_factory: Option<RequestFactory>,
}

#[derive(Clone, Debug)]
pub enum ConnectionReadError {
    ReadError(Errno),
    NotReadyToRead(ConnectionStatus),
    MalformedRequest(RequestParseError),
    RequestIncomplete,
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
    ReadingHeaders,
    ReadingContent(usize),
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
            Self::NotReadyToRead(_) | Self::RequestIncomplete => true,
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
            Self::NotReadyToWrite(state) => matches!(state, ConnectionStatus::ReadingHeaders),
        }
    }
}

impl Connection {
    pub(crate) const fn new(descriptor: usize) -> Self {
        Self {
            descriptor,
            buffer: [0; BUFFER_SIZE],
            state: ConnectionStatus::ReadingHeaders,
            collector: String::new(),
            write_index: 0,
            request_factory: Some(RequestFactory::new()),
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

    pub fn read(&mut self) -> Result<Request, ConnectionReadError> {
        if !self.is_reading() {
            return Err(ConnectionReadError::NotReadyToRead(self.state));
        }

        let mut read_result = self.read_once();
        while let Ok(read_size) = read_result {
            if read_size == 0 {
                break;
            }
            read_result = self.read_once();
        }
        if let Err(error) = read_result.as_ref() && error.is_fatal() {
            self.kill();
            return Err(error.clone());
        }
        if let Err(error) = self
            .request_factory
            .as_mut()
            .expect("Tried to process read result without request factory.")
            .process_str(&self.collector)
        {
            self.kill();
            return Err(ConnectionReadError::MalformedRequest(error));
        }
        self.collector.clear();
        if self.is_alive()
            && self
                .request_factory
                .as_ref()
                .is_some_and(|req_fact| req_fact.is_completed())
        {
            self.state = ConnectionStatus::AwaitingResponse;
            return self
                .request_factory
                .take()
                .expect("Request factory missing inside guard.")
                .try_into()
                .map_err(ConnectionReadError::MalformedRequest);
        }
        match read_result {
            Ok(_) => Err(ConnectionReadError::RequestIncomplete),
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

    pub fn begin_response(&mut self, response: &Response) -> Result<(), ConnectionResponseError> {
        if !self.is_awaiting_response() {
            return Err(ConnectionResponseError::NotReadyToRespond(self.state));
        }
        self.collector = format!("{}", response);
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
        matches!(
            self.state,
            ConnectionStatus::ReadingHeaders | ConnectionStatus::ReadingContent(_)
        )
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
