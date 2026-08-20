#[cfg(windows)]
mod platform {
    use std::{io, ptr};

    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
        System::Threading::CreateMutexW,
    };

    pub struct WindowsSingleInstance {
        handle: HANDLE,
    }

    impl WindowsSingleInstance {
        pub fn acquire(name: &str) -> io::Result<Option<Self>> {
            let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let handle = unsafe { CreateMutexW(ptr::null(), 0, name.as_ptr()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                unsafe { CloseHandle(handle) };
                return Ok(None);
            }
            Ok(Some(Self { handle }))
        }
    }

    impl Drop for WindowsSingleInstance {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.handle) };
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::io;

    pub struct WindowsSingleInstance;

    impl WindowsSingleInstance {
        pub fn acquire(_name: &str) -> io::Result<Option<Self>> {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the backend runtime is supported on Windows only",
            ))
        }
    }
}

pub use platform::WindowsSingleInstance;
