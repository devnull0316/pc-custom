use super::{WindowsError, WindowsErrorKind, WindowsResult};
use crate::backup::PowerSchemeGuid;

#[cfg(windows)]
pub fn active_power_scheme() -> WindowsResult<PowerSchemeGuid> {
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
    Ok(PowerSchemeGuid {
        data1: guid.data1,
        data2: guid.data2,
        data3: guid.data3,
        data4: guid.data4,
    })
}

#[cfg(not(windows))]
pub fn active_power_scheme() -> WindowsResult<PowerSchemeGuid> {
    Err(WindowsError::unsupported("PowerGetActiveScheme"))
}

pub fn active_power_scheme_guid() -> WindowsResult<String> {
    active_power_scheme().map(PowerSchemeGuid::canonical_string)
}

#[cfg(windows)]
pub fn set_active_power_scheme(scheme: &PowerSchemeGuid) -> WindowsResult<()> {
    use windows::{core::GUID, Win32::System::Power::PowerSetActiveScheme};

    let guid = GUID::from_values(scheme.data1, scheme.data2, scheme.data3, scheme.data4);
    let status = unsafe { PowerSetActiveScheme(None, Some(&guid)) };
    if status.0 != 0 {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "PowerSetActiveScheme",
            Some(i64::from(status.0)),
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_active_power_scheme(_scheme: &PowerSchemeGuid) -> WindowsResult<()> {
    Err(WindowsError::unsupported("PowerSetActiveScheme"))
}
