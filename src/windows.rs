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

#[cfg(all(test, windows))]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::WindowsSingleInstance;

    #[test]
    fn rejects_duplicate_instance_until_first_handle_is_dropped() {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let name = format!(
            r"Local\Background_Codex_Quota_test_{}_{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );

        let first = WindowsSingleInstance::acquire(&name)
            .unwrap()
            .expect("first instance should acquire the mutex");
        assert!(WindowsSingleInstance::acquire(&name).unwrap().is_none());

        drop(first);
        assert!(WindowsSingleInstance::acquire(&name).unwrap().is_some());
    }
}
