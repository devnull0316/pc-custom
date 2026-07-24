use super::{WindowsError, WindowsErrorKind, WindowsResult};

/// Process-crossing writer lock for the Task 2 safety core.
///
/// A global writer lock is deliberately stricter than independent resource locks:
/// every resource key is therefore acquired in the same effective order. This
/// prevents two Totonoe instances from interleaving backup/apply/verify while the
/// owner-aware per-resource lease model is completed in the next profile slice.
#[cfg(windows)]
pub struct CoreMutationGuard {
    handle: windows::Win32::Foundation::HANDLE,
    acquired: bool,
}

#[cfg(windows)]
impl Drop for CoreMutationGuard {
    fn drop(&mut self) {
        use windows::Win32::{Foundation::CloseHandle, System::Threading::ReleaseMutex};

        if self.acquired {
            // Drop is the crash-safe last resort for early-return paths. The
            // transaction journal remains authoritative if Windows rejects a
            // release during process teardown.
            let _release_result = unsafe { ReleaseMutex(self.handle) };
            self.acquired = false;
        }
        let _close_result = unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(windows)]
pub struct AppInstanceGuard {
    _exclusive_file: std::fs::File,
}

#[cfg(windows)]
pub fn acquire_core_mutation_lock() -> WindowsResult<CoreMutationGuard> {
    use windows::core::w;

    acquire_named_mutex(
        w!("Local\\Totonoe.CoreMutation.v1"),
        30_000,
        "CreateMutexW core mutation lock",
        "wait for core mutation lock",
    )
}

/// Held for the whole application lifetime. A second process stays fail-closed
/// and cannot reconcile a live first process's SetThreadExecutionState lease.
#[cfg(windows)]
pub fn acquire_app_instance_lock(path: &std::path::Path) -> WindowsResult<AppInstanceGuard> {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .share_mode(0)
        .open(path)
        .map_err(|error| {
            let kind = match error.raw_os_error() {
                Some(32 | 33) => WindowsErrorKind::ResourceLimit,
                Some(5) => WindowsErrorKind::AccessDenied,
                _ => WindowsErrorKind::ApiFailure,
            };
            WindowsError::new(
                kind,
                "open exclusive per-user app instance lock",
                error.raw_os_error().map(i64::from),
            )
        })?;
    Ok(AppInstanceGuard {
        _exclusive_file: file,
    })
}

#[cfg(windows)]
fn acquire_named_mutex(
    name: windows::core::PCWSTR,
    wait_ms: u32,
    create_operation: &'static str,
    wait_operation: &'static str,
) -> WindowsResult<CoreMutationGuard> {
    use windows::Win32::{
        Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{CreateMutexW, WaitForSingleObject},
    };

    let handle = unsafe { CreateMutexW(None, false, name) }.map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            create_operation,
            Some(i64::from(error.code().0)),
        )
    })?;

    let wait = unsafe { WaitForSingleObject(handle, wait_ms) };
    if wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED {
        return Ok(CoreMutationGuard {
            handle,
            acquired: true,
        });
    }

    let close_result = unsafe { CloseHandle(handle) };
    if let Err(error) = close_result {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "CloseHandle failed named lock acquisition",
            Some(i64::from(error.code().0)),
        ));
    }
    if wait == WAIT_TIMEOUT {
        Err(WindowsError::new(
            WindowsErrorKind::ResourceLimit,
            wait_operation,
            None,
        ))
    } else {
        let error = std::io::Error::last_os_error();
        Err(WindowsError::io(wait_operation, &error))
    }
}

#[cfg(not(windows))]
pub struct CoreMutationGuard;

#[cfg(not(windows))]
pub struct AppInstanceGuard;

#[cfg(not(windows))]
pub fn acquire_core_mutation_lock() -> WindowsResult<CoreMutationGuard> {
    Err(WindowsError::unsupported("acquire core mutation lock"))
}

#[cfg(not(windows))]
pub fn acquire_app_instance_lock(_path: &std::path::Path) -> WindowsResult<AppInstanceGuard> {
    Err(WindowsError::unsupported("acquire app instance lock"))
}
