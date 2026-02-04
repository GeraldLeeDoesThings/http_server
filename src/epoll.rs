use std::mem::MaybeUninit;

use libc::{
    EBADF, EEXIST, EFAULT, EINTR, EINVAL, ELOOP, ENOENT, ENOMEM, ENOSPC, EPERM, EPOLL_CTL_ADD,
    EPOLL_CTL_DEL, EPOLL_CTL_MOD, EPOLLIN, EPOLLOUT, epoll_event,
};
use syscalls::{
    Errno,
    Sysno::{close, epoll_create1, epoll_ctl, epoll_wait},
    syscall,
};

pub struct EPoll {
    file_descriptor: usize,
}

#[derive(Debug)]
#[allow(unused)]
pub enum EPollCtlError {
    InvalidFileDescriptor,
    AlreadyRegistered,
    InvalidInput,
    CircularOrTooDeepNesting,
    NotRegistered,
    OutOfMemory,
    MaxUserWatchesHit,
    EPollNotSupported,
    Other(i32),
}

impl From<i32> for EPollCtlError {
    fn from(value: i32) -> Self {
        match value {
            EBADF => Self::InvalidFileDescriptor,
            EEXIST => Self::AlreadyRegistered,
            EINVAL => Self::InvalidInput,
            ELOOP => Self::CircularOrTooDeepNesting,
            ENOENT => Self::NotRegistered,
            ENOMEM => Self::OutOfMemory,
            ENOSPC => Self::MaxUserWatchesHit,
            EPERM => Self::EPollNotSupported,
            other => Self::Other(other),
        }
    }
}

#[repr(transparent)]
pub struct EPollEvent {
    event_bits: epoll_event,
}

impl EPollEvent {
    pub const fn readable(&self) -> bool {
        self.event_bits.events & EPOLLIN as u32 > 0
    }

    pub const fn writable(&self) -> bool {
        self.event_bits.events & EPOLLOUT as u32 > 0
    }

    pub const fn file_descriptor(&self) -> usize {
        self.event_bits.u64 as usize
    }
}

impl EPoll {
    pub fn new() -> Result<Self, Errno> {
        Ok(Self {
            file_descriptor: unsafe { syscall!(epoll_create1, 0)? },
        })
    }

    pub fn wait(&self, buffer: &mut Vec<EPollEvent>) {
        unsafe {
            match syscall!(
                epoll_wait,
                self.file_descriptor,
                buffer.as_mut_ptr(),
                buffer.capacity(),
                usize::MAX
            ) {
                Ok(num_events) => {
                    buffer.set_len(num_events);
                }
                Err(errno) => match errno.into_raw() {
                    EBADF | EINVAL => {
                        panic!("EPoll file descriptor has become invalidated during use!")
                    }
                    EFAULT => panic!("Buffer memory is not writable!"),
                    EINTR => self.wait(buffer),
                    _ => unreachable!("Enexpected error code!"),
                },
            }
        }
    }

    pub fn add(
        &self,
        file_descriptor: usize,
        read: bool,
        write: bool,
    ) -> Result<(), EPollCtlError> {
        self.ctl_add_modify(true, file_descriptor, read, write)
    }

    pub fn modify(
        &self,
        file_descriptor: usize,
        read: bool,
        write: bool,
    ) -> Result<(), EPollCtlError> {
        self.ctl_add_modify(false, file_descriptor, read, write)
    }

    pub fn delete(&self, file_descriptor: usize) -> Result<(), EPollCtlError> {
        unsafe {
            syscall!(
                epoll_ctl,
                self.file_descriptor,
                EPOLL_CTL_DEL,
                file_descriptor
            )
        }
        .map_err(|errno| EPollCtlError::from(errno.into_raw()))
        .map(|return_value| assert_eq!(return_value, 0))
    }

    fn ctl_add_modify(
        &self,
        is_new_file_descriptor: bool,
        file_descriptor: usize,
        read: bool,
        write: bool,
    ) -> Result<(), EPollCtlError> {
        let mut event = unsafe { MaybeUninit::<epoll_event>::zeroed().assume_init() };
        if read {
            event.events |= u32::try_from(EPOLLIN).expect("EPOLLIN is positive i32.");
        }

        if write {
            event.events |= u32::try_from(EPOLLOUT).expect("EPOLLOUT is a positive i32.");
        }

        event.u64 = file_descriptor as u64;
        unsafe {
            syscall!(
                epoll_ctl,
                self.file_descriptor,
                if is_new_file_descriptor {
                    EPOLL_CTL_ADD
                } else {
                    EPOLL_CTL_MOD
                },
                file_descriptor,
                &mut event as *mut _
            )
        }
        .map_err(|errno| EPollCtlError::from(errno.into_raw()))
        .map(|return_value| assert_eq!(return_value, 0))
    }
}

impl Drop for EPoll {
    fn drop(&mut self) {
        let _ = unsafe { syscall!(close, self.file_descriptor) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create() {
        EPoll::new().expect("Failed to create epoll.");
    }
}
