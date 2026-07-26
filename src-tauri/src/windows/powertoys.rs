//! PowerToys installation observation through documented Windows registration only.
//! This module never opens PowerToys settings files.

use crate::action::PowerToysInstallationObservation;

use super::{resolve_powertoys_app_path, WindowsResult};

const POWERTOYS_DISPLAY_NAMES: &[&str] = &[
    "Microsoft PowerToys",
    "Microsoft PowerToys (Preview)",
    "PowerToys (Preview)",
];

fn is_known_powertoys_uninstall_record(display_name: &str, publisher: Option<&str>) -> bool {
    POWERTOYS_DISPLAY_NAMES.contains(&display_name) && publisher == Some("Microsoft Corporation")
}

#[cfg(windows)]
fn registry_string(key: &winreg::RegKey, value_name: &str) -> Option<String> {
    use winreg::enums::RegType;

    let raw = key.get_raw_value(value_name).ok()?;
    if raw.vtype != RegType::REG_SZ || raw.bytes.len() % 2 != 0 || raw.bytes.len() > 65_536 {
        return None;
    }
    let units = raw
        .bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|value| *value != 0)
        .collect::<Vec<_>>();
    String::from_utf16(&units).ok()
}

/// Optional version metadata from Windows' documented uninstall registration.
/// The App Paths result remains the sole installed/launchable decision.
#[cfg(windows)]
fn registered_display_version() -> Option<String> {
    use winreg::{
        enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        },
        RegKey,
    };

    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    const MAX_SUBKEYS_PER_VIEW: usize = 4_096;
    let roots = [
        RegKey::predef(HKEY_CURRENT_USER),
        RegKey::predef(HKEY_LOCAL_MACHINE),
    ];
    let views = [KEY_READ | KEY_WOW64_64KEY, KEY_READ | KEY_WOW64_32KEY];

    for root in roots {
        for view in views {
            let Ok(uninstall) = root.open_subkey_with_flags(UNINSTALL, view) else {
                continue;
            };
            for subkey_name in uninstall.enum_keys().take(MAX_SUBKEYS_PER_VIEW).flatten() {
                let Ok(entry) = uninstall.open_subkey_with_flags(&subkey_name, KEY_READ) else {
                    continue;
                };
                let Some(display_name) = registry_string(&entry, "DisplayName") else {
                    continue;
                };
                let publisher = registry_string(&entry, "Publisher");
                if !is_known_powertoys_uninstall_record(&display_name, publisher.as_deref()) {
                    continue;
                }
                let version = registry_string(&entry, "DisplayVersion")?;
                let bounded = version.trim().chars().take(64).collect::<String>();
                return (!bounded.is_empty()).then_some(bounded);
            }
        }
    }
    None
}

#[cfg(windows)]
pub fn read_powertoys_installation() -> WindowsResult<PowerToysInstallationObservation> {
    let installed = resolve_powertoys_app_path()?.is_some();
    Ok(PowerToysInstallationObservation {
        installed,
        version: installed.then(registered_display_version).flatten(),
        launch_available: installed,
    })
}

#[cfg(not(windows))]
pub fn read_powertoys_installation() -> WindowsResult<PowerToysInstallationObservation> {
    Err(super::WindowsError::unsupported(
        "read PowerToys documented registration",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uninstall_identity_requires_known_name_and_microsoft_publisher() {
        assert!(is_known_powertoys_uninstall_record(
            "Microsoft PowerToys",
            Some("Microsoft Corporation")
        ));
        assert!(is_known_powertoys_uninstall_record(
            "PowerToys (Preview)",
            Some("Microsoft Corporation")
        ));
        assert!(!is_known_powertoys_uninstall_record(
            "PowerToys (Preview)",
            Some("Unknown Publisher")
        ));
        assert!(!is_known_powertoys_uninstall_record(
            "PowerToys helper",
            Some("Microsoft Corporation")
        ));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "CC実機確認専用: App Pathsとアンインストール登録を読み取るだけ"]
    fn actual_powertoys_registration_is_reported_without_settings_access() {
        let observed =
            read_powertoys_installation().expect("read documented PowerToys registration");
        println!(
            "PowerToys installed={}, version={}, launch_available={}",
            observed.installed,
            observed.version.as_deref().unwrap_or("unknown"),
            observed.launch_available
        );
        assert_eq!(observed.installed, observed.launch_available);
    }
}
