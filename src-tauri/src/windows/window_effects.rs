use serde::{Deserialize, Serialize};

use super::{WindowsError, WindowsErrorKind, WindowsResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccentColor {
    pub alpha: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub opaque_blend: bool,
}

#[cfg(windows)]
pub fn apply_mica_backdrop(
    hwnd: windows::Win32::Foundation::HWND,
    dark: bool,
) -> WindowsResult<()> {
    use windows::Win32::{
        Foundation::BOOL,
        Graphics::Dwm::{
            DwmSetWindowAttribute, DWMSBT_MAINWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
        },
    };

    let dark_value = BOOL::from(dark);
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&dark_value as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        )
    }
    .map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "DwmSetWindowAttribute dark mode",
            Some(i64::from(error.code().0)),
        )
    })?;

    let backdrop = DWMSBT_MAINWINDOW;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&backdrop as *const _).cast(),
            std::mem::size_of_val(&backdrop) as u32,
        )
    }
    .map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "DwmSetWindowAttribute Mica",
            Some(i64::from(error.code().0)),
        )
    })
}

#[cfg(not(windows))]
pub fn apply_mica_backdrop(_hwnd: isize, _dark: bool) -> WindowsResult<()> {
    Err(WindowsError::unsupported("apply Mica backdrop"))
}

#[cfg(windows)]
pub fn system_accent_color() -> WindowsResult<AccentColor> {
    use windows::Win32::{Foundation::BOOL, Graphics::Dwm::DwmGetColorizationColor};

    let mut color = 0u32;
    let mut opaque = BOOL::default();
    unsafe { DwmGetColorizationColor(&mut color, &mut opaque) }.map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "DwmGetColorizationColor",
            Some(i64::from(error.code().0)),
        )
    })?;
    Ok(AccentColor {
        alpha: (color >> 24) as u8,
        red: (color >> 16) as u8,
        green: (color >> 8) as u8,
        blue: color as u8,
        opaque_blend: opaque.as_bool(),
    })
}

#[cfg(not(windows))]
pub fn system_accent_color() -> WindowsResult<AccentColor> {
    Err(WindowsError::unsupported("read system accent color"))
}
