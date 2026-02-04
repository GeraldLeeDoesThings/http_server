use libc::{EINVAL, EMFILE, ENFILE, ENODEV, ENOMEM};
use syscalls::syscall;

#[derive(Debug)]
pub struct EventFD {
    file_descriptor: usize,
}

#[derive(Debug)]
pub enum EventFDCreateError {
    UnsupportedFlag,
    ProcessFileDescriptorLimitHit,
    SystemFileDescriptorLimitHit,
    MountingFailed,
    OutOfMemory,
}

impl TryFrom<i32> for EventFDCreateError {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(match value {
            EINVAL => Self::UnsupportedFlag,
            EMFILE => Self::ProcessFileDescriptorLimitHit,
            ENFILE => Self::SystemFileDescriptorLimitHit,
            ENODEV => Self::MountingFailed,
            ENOMEM => Self::OutOfMemory,
            _ => return Err(()),
        })
    }
}

impl EventFD {
    pub fn new() -> Result<Self, EventFDCreateError> {
        unsafe { syscall!(syscalls::Sysno::eventfd, 0, 0) }
            .map(|file_descriptor| Self { file_descriptor })
            .map_err(|errno| {
                EventFDCreateError::try_from(errno.into_raw()).expect("Impossible return value.")
            })
    }

    pub const fn get_file_descriptor(&self) -> usize {
        self.file_descriptor
    }

    pub fn set(&self) {
        unsafe {
            syscall!(
                syscalls::Sysno::write,
                self.file_descriptor,
                1_u64.to_ne_bytes().as_ptr(),
                8
            )
        }
        .expect("Failed to set event!");
    }

    pub fn unset(&self) {
        unimplemented!()
    }
}

impl Drop for EventFD {
    fn drop(&mut self) {
        let _ = unsafe { syscall!(syscalls::Sysno::close, self.file_descriptor) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create() {
        EventFD::new().expect("Failed to create eventfd.");
    }

    #[test]
    fn set() {
        let event_fd = EventFD::new().expect("Failed to create eventfd.");
        event_fd.set();
    }
}
