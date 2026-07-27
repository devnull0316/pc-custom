use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    X64,
    Arm64,
    X86,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OsIdentitySource {
    WmiAndRegistry64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OsIdentity {
    pub major: u32,
    pub minor: u32,
    pub base_build: u32,
    pub revision: Option<u32>,
    pub operating_system_sku: u32,
    pub product_type: u32,
    pub architecture: Architecture,
    pub observed_at_unix_ms: u64,
    pub source: OsIdentitySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsIdentityErrorKind {
    UnsupportedPlatform,
    WmiUnavailable,
    RegistryUnavailable,
    InvalidObservation,
    ConflictingObservation,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct OsIdentityError {
    pub kind: OsIdentityErrorKind,
    message: &'static str,
}

impl OsIdentityError {
    const fn new(kind: OsIdentityErrorKind, message: &'static str) -> Self {
        Self { kind, message }
    }
}

impl OsIdentity {
    /// This is the sole production entry point for Windows build discovery.
    #[cfg(windows)]
    pub fn load() -> Result<Self, OsIdentityError> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let wmi_result = std::thread::Builder::new()
            .name("totonoe-os-identity-wmi".to_owned())
            .spawn(query_wmi_identity)
            .map_err(|_| {
                OsIdentityError::new(
                    OsIdentityErrorKind::WmiUnavailable,
                    "unable to start the OS identity observer",
                )
            })?
            .join()
            .map_err(|_| {
                OsIdentityError::new(
                    OsIdentityErrorKind::WmiUnavailable,
                    "the OS identity observer terminated unexpectedly",
                )
            })?;
        let wmi = wmi_result?;

        let registry = query_registry_observation()?;
        if registry.base_build != wmi.base_build {
            return Err(OsIdentityError::new(
                OsIdentityErrorKind::ConflictingObservation,
                "Windows build observations disagree",
            ));
        }

        let observed_at_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| {
                OsIdentityError::new(
                    OsIdentityErrorKind::InvalidObservation,
                    "the system clock is before the Unix epoch",
                )
            })?
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;

        Ok(Self {
            major: wmi.major,
            minor: wmi.minor,
            base_build: wmi.base_build,
            revision: registry.revision,
            operating_system_sku: wmi.operating_system_sku,
            product_type: wmi.product_type,
            architecture: registry.architecture,
            observed_at_unix_ms,
            source: OsIdentitySource::WmiAndRegistry64,
        })
    }

    #[cfg(not(windows))]
    pub fn load() -> Result<Self, OsIdentityError> {
        Err(OsIdentityError::new(
            OsIdentityErrorKind::UnsupportedPlatform,
            "PCカスタム Windows Actions are unavailable on this platform",
        ))
    }

    #[cfg(test)]
    #[doc(hidden)]
    pub fn from_test_build(base_build: u32) -> Self {
        Self {
            major: 10,
            minor: 0,
            base_build,
            revision: Some(1),
            operating_system_sku: 48,
            product_type: 1,
            architecture: Architecture::X64,
            observed_at_unix_ms: 0,
            source: OsIdentitySource::WmiAndRegistry64,
        }
    }
}

#[cfg(windows)]
#[derive(Debug)]
struct WmiIdentity {
    major: u32,
    minor: u32,
    base_build: u32,
    operating_system_sku: u32,
    product_type: u32,
}

#[cfg(windows)]
#[allow(non_snake_case)]
#[derive(Debug, Deserialize)]
struct WmiOperatingSystem {
    Version: String,
    BuildNumber: String,
    OperatingSystemSKU: u32,
    ProductType: u32,
}

#[cfg(windows)]
fn query_wmi_identity() -> Result<WmiIdentity, OsIdentityError> {
    use wmi::{COMLibrary, WMIConnection};

    let com = COMLibrary::new().map_err(|_| {
        OsIdentityError::new(
            OsIdentityErrorKind::WmiUnavailable,
            "WMI initialization failed while identifying Windows",
        )
    })?;
    let connection = WMIConnection::new(com).map_err(|_| {
        OsIdentityError::new(
            OsIdentityErrorKind::WmiUnavailable,
            "WMI connection failed while identifying Windows",
        )
    })?;
    let mut rows: Vec<WmiOperatingSystem> = connection
        .raw_query(
            "SELECT Version, BuildNumber, OperatingSystemSKU, ProductType FROM Win32_OperatingSystem",
        )
        .map_err(|_| {
            OsIdentityError::new(
                OsIdentityErrorKind::WmiUnavailable,
                "WMI did not return a Windows identity",
            )
        })?;
    if rows.len() != 1 {
        return Err(OsIdentityError::new(
            OsIdentityErrorKind::InvalidObservation,
            "WMI returned an ambiguous Windows identity",
        ));
    }
    let row = rows.pop().expect("length checked");
    let mut version = row.Version.split('.');
    let major = version.next().and_then(|v| v.parse().ok()).ok_or_else(|| {
        OsIdentityError::new(
            OsIdentityErrorKind::InvalidObservation,
            "Windows major version is invalid",
        )
    })?;
    let minor = version.next().and_then(|v| v.parse().ok()).ok_or_else(|| {
        OsIdentityError::new(
            OsIdentityErrorKind::InvalidObservation,
            "Windows minor version is invalid",
        )
    })?;
    let base_build = row.BuildNumber.parse().map_err(|_| {
        OsIdentityError::new(
            OsIdentityErrorKind::InvalidObservation,
            "Windows build number is invalid",
        )
    })?;
    Ok(WmiIdentity {
        major,
        minor,
        base_build,
        operating_system_sku: row.OperatingSystemSKU,
        product_type: row.ProductType,
    })
}

#[cfg(windows)]
struct RegistryObservation {
    base_build: u32,
    revision: Option<u32>,
    architecture: Architecture,
}

#[cfg(windows)]
fn query_registry_observation() -> Result<RegistryObservation, OsIdentityError> {
    use winreg::{
        enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY},
        RegKey,
    };

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let current_version = hklm
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            KEY_READ | KEY_WOW64_64KEY,
        )
        .map_err(|_| {
            OsIdentityError::new(
                OsIdentityErrorKind::RegistryUnavailable,
                "the 64-bit Windows version registry view is unavailable",
            )
        })?;
    let build_text: String = current_version
        .get_value("CurrentBuildNumber")
        .map_err(|_| {
            OsIdentityError::new(
                OsIdentityErrorKind::RegistryUnavailable,
                "the Windows build registry value is unavailable",
            )
        })?;
    let base_build = build_text.parse().map_err(|_| {
        OsIdentityError::new(
            OsIdentityErrorKind::InvalidObservation,
            "the Windows build registry value is invalid",
        )
    })?;
    let revision = match current_version.get_value::<u32, _>("UBR") {
        Ok(value) => Some(value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Err(OsIdentityError::new(
                OsIdentityErrorKind::RegistryUnavailable,
                "the Windows revision registry value could not be read",
            ));
        }
    };

    let machine_environment = hklm
        .open_subkey_with_flags(
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
            KEY_READ | KEY_WOW64_64KEY,
        )
        .map_err(|_| {
            OsIdentityError::new(
                OsIdentityErrorKind::RegistryUnavailable,
                "the native architecture observation is unavailable",
            )
        })?;
    let architecture_text: String = machine_environment
        .get_value("PROCESSOR_ARCHITECTURE")
        .map_err(|_| {
            OsIdentityError::new(
                OsIdentityErrorKind::RegistryUnavailable,
                "the native architecture value is unavailable",
            )
        })?;
    let architecture = match architecture_text.as_str() {
        "AMD64" => Architecture::X64,
        "ARM64" => Architecture::Arm64,
        "x86" | "X86" => Architecture::X86,
        _ => Architecture::Unknown,
    };

    Ok(RegistryObservation {
        base_build,
        revision,
        architecture,
    })
}
