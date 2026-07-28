use crate::backup::KeyboardAccessibilitySettings;

use super::{WindowsError, WindowsErrorKind, WindowsResult};

#[cfg(windows)]
use windows::Win32::UI::{
    Accessibility::{
        FILTERKEYS, SKF_CONFIRMHOTKEY, SKF_HOTKEYACTIVE, SKF_LALTLATCHED, SKF_LALTLOCKED,
        SKF_LCTLLATCHED, SKF_LCTLLOCKED, SKF_LSHIFTLATCHED, SKF_LSHIFTLOCKED, SKF_RALTLATCHED,
        SKF_RALTLOCKED, SKF_RCTLLATCHED, SKF_RCTLLOCKED, SKF_RSHIFTLATCHED, SKF_RSHIFTLOCKED,
        SKF_STICKYKEYSON, STICKYKEYS, STICKYKEYS_FLAGS,
    },
    WindowsAndMessaging::{
        SystemParametersInfoW, FKF_CONFIRMHOTKEY, FKF_FILTERKEYSON, FKF_HOTKEYACTIVE,
        SPI_GETFILTERKEYS, SPI_GETSTICKYKEYS, SPI_SETFILTERKEYS, SPI_SETSTICKYKEYS,
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    },
};

#[cfg(windows)]
const STICKY_HOTKEY_ACTIVE: u32 = SKF_HOTKEYACTIVE.0;
#[cfg(not(windows))]
const STICKY_HOTKEY_ACTIVE: u32 = 0x0000_0004;
#[cfg(windows)]
const STICKY_CONFIRM_HOTKEY: u32 = SKF_CONFIRMHOTKEY.0;
#[cfg(not(windows))]
const STICKY_CONFIRM_HOTKEY: u32 = 0x0000_0008;
pub const STICKY_SHORTCUT_FLAGS: u32 = STICKY_HOTKEY_ACTIVE | STICKY_CONFIRM_HOTKEY;

#[cfg(windows)]
const FILTER_HOTKEY_ACTIVE: u32 = FKF_HOTKEYACTIVE;
#[cfg(not(windows))]
const FILTER_HOTKEY_ACTIVE: u32 = 0x0000_0004;
#[cfg(windows)]
const FILTER_CONFIRM_HOTKEY: u32 = FKF_CONFIRMHOTKEY;
#[cfg(not(windows))]
const FILTER_CONFIRM_HOTKEY: u32 = 0x0000_0008;
pub const FILTER_SHORTCUT_FLAGS: u32 = FILTER_HOTKEY_ACTIVE | FILTER_CONFIRM_HOTKEY;

#[cfg(windows)]
const STICKY_FEATURE_ENABLED: u32 = SKF_STICKYKEYSON.0;
#[cfg(not(windows))]
const STICKY_FEATURE_ENABLED: u32 = 0x0000_0001;

#[cfg(windows)]
const FILTER_FEATURE_ENABLED: u32 = FKF_FILTERKEYSON;
#[cfg(not(windows))]
const FILTER_FEATURE_ENABLED: u32 = 0x0000_0001;

#[cfg(windows)]
const STICKY_TRANSIENT_STATE_FLAGS: u32 = SKF_LALTLATCHED.0
    | SKF_LALTLOCKED.0
    | SKF_LCTLLATCHED.0
    | SKF_LCTLLOCKED.0
    | SKF_LSHIFTLATCHED.0
    | SKF_LSHIFTLOCKED.0
    | SKF_RALTLATCHED.0
    | SKF_RALTLOCKED.0
    | SKF_RCTLLATCHED.0
    | SKF_RCTLLOCKED.0
    | SKF_RSHIFTLATCHED.0
    | SKF_RSHIFTLOCKED.0;
#[cfg(not(windows))]
const STICKY_TRANSIENT_STATE_FLAGS: u32 = 0x3F3F_0000;

pub const fn sticky_feature_is_enabled(settings: KeyboardAccessibilitySettings) -> bool {
    settings.sticky_flags & STICKY_FEATURE_ENABLED != 0
}

pub const fn sticky_shortcut_is_enabled(settings: KeyboardAccessibilitySettings) -> bool {
    settings.sticky_flags & STICKY_HOTKEY_ACTIVE != 0
}

pub const fn sticky_confirmation_is_enabled(settings: KeyboardAccessibilitySettings) -> bool {
    settings.sticky_flags & STICKY_CONFIRM_HOTKEY != 0
}

pub const fn sticky_transient_state_is_active(settings: KeyboardAccessibilitySettings) -> bool {
    settings.sticky_flags & STICKY_TRANSIENT_STATE_FLAGS != 0
}

pub const fn filter_feature_is_enabled(settings: KeyboardAccessibilitySettings) -> bool {
    settings.filter_flags & FILTER_FEATURE_ENABLED != 0
}

pub const fn filter_shortcut_is_enabled(settings: KeyboardAccessibilitySettings) -> bool {
    settings.filter_flags & FILTER_HOTKEY_ACTIVE != 0
}

pub const fn filter_confirmation_is_enabled(settings: KeyboardAccessibilitySettings) -> bool {
    settings.filter_flags & FILTER_CONFIRM_HOTKEY != 0
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

/// Replaces both documented structures after re-reading the expected state.
/// If the second write fails, compensation touches only values still matching
/// the expected or desired transaction states.
#[cfg(windows)]
pub fn replace_keyboard_accessibility_settings(
    expected: KeyboardAccessibilitySettings,
    desired: KeyboardAccessibilitySettings,
) -> WindowsResult<()> {
    validate_structure_sizes(expected)?;
    validate_structure_sizes(desired)?;
    let before = read_keyboard_accessibility_settings()?;
    if before != expected {
        return Err(WindowsError::new(
            WindowsErrorKind::ExternalConflict,
            "recheck keyboard accessibility settings before replacement",
            None,
        ));
    }
    let sticky_write_dispatched = !sticky_component_matches(expected, desired);
    if sticky_write_dispatched {
        if let Err(sticky_error) = set_sticky_settings(desired) {
            compensate_owned_sticky_component(expected, desired)?;
            return Err(sticky_error);
        }
    }
    let after_sticky = match read_keyboard_accessibility_settings() {
        Ok(settings) => settings,
        Err(read_error) => {
            if sticky_write_dispatched {
                compensate_owned_sticky_component(expected, desired)?;
            }
            return Err(read_error);
        }
    };
    if !sticky_component_matches(after_sticky, desired)
        || !filter_component_matches(after_sticky, expected)
    {
        if sticky_write_dispatched {
            compensate_owned_sticky_component(expected, desired)?;
        }
        return Err(WindowsError::new(
            WindowsErrorKind::ExternalConflict,
            "recheck keyboard accessibility settings between replacements",
            None,
        ));
    }
    if filter_component_matches(expected, desired) {
        return Ok(());
    }
    if let Err(filter_error) = set_filter_settings(desired) {
        compensate_partial_replacement(expected, desired)?;
        return Err(filter_error);
    }
    let applied = match read_keyboard_accessibility_settings() {
        Ok(settings) => settings,
        Err(read_error) => {
            compensate_partial_replacement(expected, desired)?;
            return Err(read_error);
        }
    };
    if applied != desired {
        compensate_partial_replacement(expected, desired)?;
        return Err(WindowsError::new(
            WindowsErrorKind::ExternalConflict,
            "verify keyboard accessibility settings after replacement",
            None,
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn replace_keyboard_accessibility_settings(
    _expected: KeyboardAccessibilitySettings,
    _desired: KeyboardAccessibilitySettings,
) -> WindowsResult<()> {
    Err(WindowsError::unsupported(
        "replace keyboard accessibility shortcut settings",
    ))
}

#[cfg(windows)]
fn compensate_owned_sticky_component(
    expected: KeyboardAccessibilitySettings,
    desired: KeyboardAccessibilitySettings,
) -> WindowsResult<()> {
    let current = read_keyboard_accessibility_settings().map_err(|error| {
        recovery_required(
            "read partial keyboard accessibility sticky update before compensation",
            error.os_code,
        )
    })?;
    if sticky_component_matches(current, expected) {
        return Ok(());
    }
    if !sticky_component_matches(current, desired) {
        return Err(recovery_required(
            "refuse to compensate an externally changed sticky-key state",
            None,
        ));
    }
    set_sticky_settings(expected).map_err(|error| {
        recovery_required(
            "compensate partial keyboard accessibility sticky update",
            error.os_code,
        )
    })?;
    let restored = read_keyboard_accessibility_settings().map_err(|error| {
        recovery_required(
            "verify partial keyboard accessibility sticky compensation",
            error.os_code,
        )
    })?;
    if !sticky_component_matches(restored, expected) {
        return Err(recovery_required(
            "verify partial keyboard accessibility sticky compensation",
            None,
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn compensate_owned_filter_component(
    expected: KeyboardAccessibilitySettings,
    desired: KeyboardAccessibilitySettings,
) -> WindowsResult<()> {
    let current = read_keyboard_accessibility_settings().map_err(|error| {
        recovery_required(
            "read partial keyboard accessibility filter update before compensation",
            error.os_code,
        )
    })?;
    if filter_component_matches(current, expected) {
        return Ok(());
    }
    if !filter_component_matches(current, desired) {
        return Err(recovery_required(
            "refuse to compensate an externally changed filter-key state",
            None,
        ));
    }
    set_filter_settings(expected).map_err(|error| {
        recovery_required(
            "compensate partial keyboard accessibility filter update",
            error.os_code,
        )
    })?;
    let restored = read_keyboard_accessibility_settings().map_err(|error| {
        recovery_required(
            "verify partial keyboard accessibility filter compensation",
            error.os_code,
        )
    })?;
    if !filter_component_matches(restored, expected) {
        return Err(recovery_required(
            "verify partial keyboard accessibility filter compensation",
            None,
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn compensate_partial_replacement(
    expected: KeyboardAccessibilitySettings,
    desired: KeyboardAccessibilitySettings,
) -> WindowsResult<()> {
    let filter_error = compensate_owned_filter_component(expected, desired).err();
    let sticky_error = compensate_owned_sticky_component(expected, desired).err();
    if let Some(error) = filter_error.or(sticky_error) {
        return Err(error);
    }

    let restored = read_keyboard_accessibility_settings().map_err(|error| {
        recovery_required("verify keyboard accessibility compensation", error.os_code)
    })?;
    if restored != expected {
        return Err(recovery_required(
            "verify keyboard accessibility compensation",
            None,
        ));
    }
    Ok(())
}

#[cfg(test)]
fn components_are_transaction_owned(
    current: KeyboardAccessibilitySettings,
    expected: KeyboardAccessibilitySettings,
    desired: KeyboardAccessibilitySettings,
) -> bool {
    (sticky_component_matches(current, expected) || sticky_component_matches(current, desired))
        && (filter_component_matches(current, expected)
            || filter_component_matches(current, desired))
}

fn sticky_component_matches(
    current: KeyboardAccessibilitySettings,
    expected: KeyboardAccessibilitySettings,
) -> bool {
    current.sticky_size == expected.sticky_size && current.sticky_flags == expected.sticky_flags
}

fn filter_component_matches(
    current: KeyboardAccessibilitySettings,
    expected: KeyboardAccessibilitySettings,
) -> bool {
    current.filter_size == expected.filter_size
        && current.filter_flags == expected.filter_flags
        && current.filter_wait_ms == expected.filter_wait_ms
        && current.filter_delay_ms == expected.filter_delay_ms
        && current.filter_repeat_ms == expected.filter_repeat_ms
        && current.filter_bounce_ms == expected.filter_bounce_ms
}

const fn recovery_required(operation: &'static str, os_code: Option<i64>) -> WindowsError {
    WindowsError::new(WindowsErrorKind::RecoveryRequired, operation, os_code)
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
            sticky_flags: 0x0000_01FE,
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

    #[test]
    fn ignored_latch_and_lock_bits_are_detected_before_any_write() {
        let mut settings = sample();
        settings.sticky_flags |= STICKY_TRANSIENT_STATE_FLAGS;
        assert!(sticky_transient_state_is_active(settings));
        assert!(!sticky_transient_state_is_active(sample()));
    }

    #[test]
    fn shortcut_and_confirmation_bits_are_observed_separately() {
        let mut settings = sample();
        settings.sticky_flags = STICKY_CONFIRM_HOTKEY;
        settings.filter_flags = FILTER_HOTKEY_ACTIVE;

        assert!(!sticky_shortcut_is_enabled(settings));
        assert!(sticky_confirmation_is_enabled(settings));
        assert!(filter_shortcut_is_enabled(settings));
        assert!(!filter_confirmation_is_enabled(settings));
    }

    #[test]
    fn transaction_components_accept_only_expected_or_desired_values() {
        let expected = sample();
        let desired = without_shift_shortcuts(expected);
        let mut mixed = expected;
        mixed.sticky_flags = desired.sticky_flags;
        assert!(components_are_transaction_owned(mixed, expected, desired));

        mixed.filter_wait_ms += 1;
        assert!(!components_are_transaction_owned(mixed, expected, desired));
    }
}
