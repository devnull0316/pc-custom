use super::{WindowsError, WindowsErrorKind, WindowsResult};

#[cfg(windows)]
pub fn active_power_scheme_guid() -> WindowsResult<String> {
    use windows::{
        core::GUID,
        Win32::{
            Foundation::{LocalFree, HLOCAL},
            System::Power::PowerGetActiveScheme,
        },
    };

    let mut pointer: *mut GUID = std::ptr::null_mut();
    let status = unsafe { PowerGetActiveScheme(None, &mut pointer) };
    if status.0 != 0 || pointer.is_null() {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "PowerGetActiveScheme",
            Some(i64::from(status.0)),
        ));
    }
    let guid = unsafe { *pointer };
    let release_result = unsafe { LocalFree(HLOCAL(pointer.cast::<core::ffi::c_void>())) };
    if !release_result.is_invalid() {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "LocalFree active power scheme",
            None,
        ));
    }
    Ok(format!(
        "{{{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7],
    ))
}

#[cfg(not(windows))]
pub fn active_power_scheme_guid() -> WindowsResult<String> {
    Err(WindowsError::unsupported("PowerGetActiveScheme"))
}
