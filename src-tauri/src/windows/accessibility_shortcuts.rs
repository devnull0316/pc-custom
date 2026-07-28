use crate::backup::KeyboardAccessibilitySettings;

use super::{WindowsError, WindowsErrorKind, WindowsResult};

#[cfg(windows)]
use windows::Win32::UI::{
    Accessibility::{
        FILTERKEYS, SKF_CONFIRMHOTKEY, SKF_HOTKEYACTIVE, SKF_STICKYKEYSON, STICKYKEYS,
        STICKYKEYS_FLAGS,
    },
    WindowsAndMessaging::{
        SystemParametersInfoW, FKF_CONFIRMHOTKEY, FKF_FILTERKEYSON, FKF_HOTKEYACTIVE,
        SPI_GETFILTERKEYS, SPI_GETSTICKYKEYS, SPI_SETFILTERKEYS, SPI_SETSTICKYKEYS,
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    },
};

#[cfg(windows)]
pub const STICKY_SHORTCUT_FLAGS: u32 = SKF_HOTKEYACTIVE.0 | SKF_CONFIRMHOTKEY.0;
#[cfg(not(windows))]
pub const STICKY_SHORTCUT_FLAGS: u32 = 0x0000_0004 | 0x0000_0008;

#[cfg(windows)]
pub const FILTER_SHORTCUT_FLAGS: u32 = FKF_HOTKEYACTIVE | FKF_CONFIRMHOTKEY;
#[cfg(not(windows))]
pub const FILTER_SHORTCUT_FLAGS: u32 = 0x0000_0004 | 0x0000_0008;

#[cfg(windows)]
const STICKY_FEATURE_ENABLED: u32 = SKF_STICKYKEYSON.0;
#[cfg(not(windows))]
const STICKY_FEATURE_ENABLED: u32 = 0x0000_0001;

#[cfg(windows)]
const FILTER_FEATURE_ENABLED: u32 = FKF_FILTERKEYSON;
#[cfg(not(windows))]
const FILTER_FEATURE_ENABLED: u32 = 0x0000_0001;

pub const fn sticky_feature_is_enabled(settings: KeyboardAccessibilitySettings) -> bool {
    settings.sticky_flags & STICKY_FEATURE_ENABLED != 0
}

pub const fn filter_feature_is_enabled(settings: KeyboardAccessibilitySettings) -> bool {
    settings.filter_flags & FILTER_FEATURE_ENABLED != 0
}

pub const fn without_shift_shortcuts(
    mut settings: KeyboardAccessibilitySettings,
) -> KeyboardAccessibilitySettings {
    settings.sticky_flags &= !STICKY_SHORTCUT_FLAGS;
    settings.filter_flags &= !FILTER_SHORTCUT_FLAGS;
    settings
}

#[cfg(windows)]
pub fn read_keyboard_accessibility_settings() -> WindowsResult<KeyboardAccessibilitySettings> {
    let sticky_size = std::mem::size_of::<STICKYKEYS>() as u32;
    let filter_size = std::mem::size_of::<FILTERKEYS>() as u32;
    let mut sticky = STICKYKEYS {
        cbSize: sticky_size,
        ..Default::default()
    };
    let mut filter = FILTERKEYS {
        cbSize: filter_size,
        ..Default::default()
    };

    unsafe {
        SystemParametersInfoW(
            SPI_GETSTICKYKEYS,
            sticky_size,
            Some((&mut sticky as *mut STICKYKEYS).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .map_err(|error| api_error("read Shift five-press shortcut settings", error))?;
    unsafe {
        SystemParametersInfoW(
            SPI_GETFILTERKEYS,
            filter_size,
            Some((&mut filter as *mut FILTERKEYS).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .map_err(|error| api_error("read right-Shift hold shortcut settings", error))?;

    let settings = KeyboardAccessibilitySettings {
        sticky_size: sticky.cbSize,
        sticky_flags: sticky.dwFlags.0,
        filter_size: filter.cbSize,
        filter_flags: filter.dwFlags,
        filter_wait_ms: filter.iWaitMSec,
        filter_delay_ms: filter.iDelayMSec,
        filter_repeat_ms: filter.iRepeatMSec,
        filter_bounce_ms: filter.iBounceMSec,
    };
    validate_structure_sizes(settings)?;
    Ok(settings)
}

#[cfg(not(windows))]
pub fn read_keyboard_accessibility_settings() -> WindowsResult<KeyboardAccessibilitySettings> {
    Err(WindowsError::unsupported(
        "read keyboard accessibility shortcut settings",
    ))
}

/// Replaces both documented structures and compensates the first write if the
/// second write fails. The caller is responsible for comparing an expected
/// precondition immediately before invoking this primitive.
#[cfg(windows)]
pub fn replace_keyboard_accessibility_settings(
    desired: KeyboardAccessibilitySettings,
) -> WindowsResult<()> {
    validate_structure_sizes(desired)?;
    let before = read_keyboard_accessibility_settings()?;
    set_sticky_settings(desired)?;
    if let Err(filter_error) = set_filter_settings(desired) {
        if let Err(compensation_error) = set_sticky_settings(before) {
            return Err(WindowsError::new(
                WindowsErrorKind::RecoveryRequired,
                "compensate partial keyboard accessibility update",
                compensation_error.os_code,
            ));
        }
        return Err(filter_error);
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn replace_keyboard_accessibility_settings(
    _desired: KeyboardAccessibilitySettings,
) -> WindowsResult<()> {
    Err(WindowsError::unsupported(
        "replace keyboard accessibility shortcut settings",
    ))
}

#[cfg(windows)]
fn set_sticky_settings(settings: KeyboardAccessibilitySettings) -> WindowsResult<()> {
    let mut sticky = STICKYKEYS {
        cbSize: settings.sticky_size,
        dwFlags: STICKYKEYS_FLAGS(settings.sticky_flags),
    };
    unsafe {
        SystemParametersInfoW(
            SPI_SETSTICKYKEYS,
            settings.sticky_size,
            Some((&mut sticky as *mut STICKYKEYS).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .map_err(|error| api_error("set Shift five-press shortcut settings", error))
}

#[cfg(windows)]
fn set_filter_settings(settings: KeyboardAccessibilitySettings) -> WindowsResult<()> {
    let mut filter = FILTERKEYS {
        cbSize: settings.filter_size,
        dwFlags: settings.filter_flags,
        iWaitMSec: settings.filter_wait_ms,
        iDelayMSec: settings.filter_delay_ms,
        iRepeatMSec: settings.filter_repeat_ms,
        iBounceMSec: settings.filter_bounce_ms,
    };
    unsafe {
        SystemParametersInfoW(
            SPI_SETFILTERKEYS,
            settings.filter_size,
            Some((&mut filter as *mut FILTERKEYS).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .map_err(|error| api_error("set right-Shift hold shortcut settings", error))
}

#[cfg(windows)]
fn validate_structure_sizes(settings: KeyboardAccessibilitySettings) -> WindowsResult<()> {
    if settings.sticky_size != std::mem::size_of::<STICKYKEYS>() as u32
        || settings.filter_size != std::mem::size_of::<FILTERKEYS>() as u32
    {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate keyboard accessibility structure sizes",
            None,
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn api_error(operation: &'static str, error: windows::core::Error) -> WindowsError {
    let code = i64::from(error.code().0);
    WindowsError::new(
        if error.code() == windows::Win32::Foundation::E_ACCESSDENIED {
            WindowsErrorKind::AccessDenied
        } else {
            WindowsErrorKind::ApiFailure
        },
        operation,
        Some(code),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> KeyboardAccessibilitySettings {
        KeyboardAccessibilitySettings {
            sticky_size: 8,
            sticky_flags: 0xABCD_01FE,
            filter_size: 24,
            filter_flags: 0x0000_007E,
            filter_wait_ms: 100,
            filter_delay_ms: 200,
            filter_repeat_ms: 300,
            filter_bounce_ms: 0,
        }
    }

    #[test]
    fn guard_clears_only_the_two_shortcut_bits_in_each_structure() {
        let before = sample();
        let after = without_shift_shortcuts(before);

        assert_eq!(
            before.sticky_flags ^ after.sticky_flags,
            before.sticky_flags & STICKY_SHORTCUT_FLAGS
        );
        assert_eq!(
            before.filter_flags ^ after.filter_flags,
            before.filter_flags & FILTER_SHORTCUT_FLAGS
        );
        assert_eq!(after.sticky_flags & STICKY_SHORTCUT_FLAGS, 0);
        assert_eq!(after.filter_flags & FILTER_SHORTCUT_FLAGS, 0);
        assert_eq!(after.sticky_size, before.sticky_size);
        assert_eq!(after.filter_size, before.filter_size);
        assert_eq!(after.filter_wait_ms, before.filter_wait_ms);
        assert_eq!(after.filter_delay_ms, before.filter_delay_ms);
        assert_eq!(after.filter_repeat_ms, before.filter_repeat_ms);
        assert_eq!(after.filter_bounce_ms, before.filter_bounce_ms);
    }

    #[test]
    fn feature_enabled_bits_are_detected_but_never_cleared() {
        let mut before = sample();
        before.sticky_flags |= STICKY_FEATURE_ENABLED;
        before.filter_flags |= FILTER_FEATURE_ENABLED;

        let after = without_shift_shortcuts(before);
        assert!(sticky_feature_is_enabled(before));
        assert!(filter_feature_is_enabled(before));
        assert!(sticky_feature_is_enabled(after));
        assert!(filter_feature_is_enabled(after));
    }
}
