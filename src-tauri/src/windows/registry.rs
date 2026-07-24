use crate::backup::{RegistryHive, RegistryLocation, RegistryView};

use super::{WindowsError, WindowsErrorKind, WindowsResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRegistryValue {
    pub value_type: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRegistryValueState {
    pub key_existed: bool,
    pub value: Option<RawRegistryValue>,
}

#[cfg(windows)]
fn map_io(operation: &'static str, error: std::io::Error) -> WindowsError {
    WindowsError::io(operation, &error)
}

#[cfg(windows)]
fn root(hive: RegistryHive) -> winreg::RegKey {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    match hive {
        RegistryHive::CurrentUser => winreg::RegKey::predef(HKEY_CURRENT_USER),
        RegistryHive::LocalMachine => winreg::RegKey::predef(HKEY_LOCAL_MACHINE),
    }
}

#[cfg(windows)]
fn view_flag(view: RegistryView) -> u32 {
    use winreg::enums::{KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    match view {
        RegistryView::Registry32 => KEY_WOW64_32KEY,
        RegistryView::Registry64 => KEY_WOW64_64KEY,
    }
}

#[cfg(windows)]
pub fn read_value_state(
    location: &RegistryLocation,
    max_bytes: usize,
) -> WindowsResult<RawRegistryValueState> {
    use std::io::ErrorKind;
    use winreg::enums::KEY_READ;

    let key = match root(location.hive).open_subkey_with_flags(
        &location.canonical_subkey,
        KEY_READ | view_flag(location.view),
    ) {
        Ok(key) => key,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(RawRegistryValueState {
                key_existed: false,
                value: None,
            })
        }
        Err(error) => return Err(map_io("open registry key for read", error)),
    };

    let value = match key.get_raw_value(&location.value_name) {
        Ok(value) => {
            if value.bytes.len() > max_bytes {
                return Err(WindowsError::new(
                    WindowsErrorKind::ResourceLimit,
                    "read bounded registry value",
                    None,
                ));
            }
            Some(RawRegistryValue {
                value_type: value.vtype as u32,
                bytes: value.bytes,
            })
        }
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => return Err(map_io("read raw registry value", error)),
    };

    Ok(RawRegistryValueState {
        key_existed: true,
        value,
    })
}

#[cfg(not(windows))]
pub fn read_value_state(
    _location: &RegistryLocation,
    _max_bytes: usize,
) -> WindowsResult<RawRegistryValueState> {
    Err(WindowsError::unsupported("read registry value"))
}

#[cfg(windows)]
fn checked_reg_type(value_type: u32) -> WindowsResult<winreg::enums::RegType> {
    use winreg::enums::{
        REG_BINARY, REG_DWORD, REG_DWORD_BIG_ENDIAN, REG_EXPAND_SZ, REG_FULL_RESOURCE_DESCRIPTOR,
        REG_LINK, REG_MULTI_SZ, REG_NONE, REG_QWORD, REG_RESOURCE_LIST,
        REG_RESOURCE_REQUIREMENTS_LIST, REG_SZ,
    };
    let value = match value_type {
        x if x == REG_NONE as u32 => REG_NONE,
        x if x == REG_SZ as u32 => REG_SZ,
        x if x == REG_EXPAND_SZ as u32 => REG_EXPAND_SZ,
        x if x == REG_BINARY as u32 => REG_BINARY,
        x if x == REG_DWORD as u32 => REG_DWORD,
        x if x == REG_DWORD_BIG_ENDIAN as u32 => REG_DWORD_BIG_ENDIAN,
        x if x == REG_LINK as u32 => REG_LINK,
        x if x == REG_MULTI_SZ as u32 => REG_MULTI_SZ,
        x if x == REG_RESOURCE_LIST as u32 => REG_RESOURCE_LIST,
        x if x == REG_FULL_RESOURCE_DESCRIPTOR as u32 => REG_FULL_RESOURCE_DESCRIPTOR,
        x if x == REG_RESOURCE_REQUIREMENTS_LIST as u32 => REG_RESOURCE_REQUIREMENTS_LIST,
        x if x == REG_QWORD as u32 => REG_QWORD,
        _ => {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "validate registry value type",
                None,
            ))
        }
    };
    Ok(value)
}

#[cfg(windows)]
fn ensure_user_scope(location: &RegistryLocation, operation: &'static str) -> WindowsResult<()> {
    if location.hive != RegistryHive::CurrentUser {
        return Err(WindowsError::new(
            WindowsErrorKind::AccessDenied,
            operation,
            None,
        ));
    }
    Ok(())
}

#[cfg(windows)]
pub fn write_raw_value(
    location: &RegistryLocation,
    value_type: u32,
    bytes: &[u8],
) -> WindowsResult<()> {
    use winreg::{
        enums::{KEY_CREATE_SUB_KEY, KEY_QUERY_VALUE, KEY_SET_VALUE},
        RegValue,
    };

    ensure_user_scope(location, "write machine-scope registry value")?;
    if bytes.len() > crate::backup::MAX_REGISTRY_VALUE_BYTES {
        return Err(WindowsError::new(
            WindowsErrorKind::ResourceLimit,
            "write bounded registry value",
            None,
        ));
    }
    let (key, _) = root(location.hive)
        .create_subkey_with_flags(
            &location.canonical_subkey,
            KEY_CREATE_SUB_KEY | KEY_QUERY_VALUE | KEY_SET_VALUE | view_flag(location.view),
        )
        .map_err(|error| map_io("create registry key", error))?;
    key.set_raw_value(
        &location.value_name,
        &RegValue {
            bytes: bytes.to_vec(),
            vtype: checked_reg_type(value_type)?,
        },
    )
    .map_err(|error| map_io("write raw registry value", error))
}

#[cfg(not(windows))]
pub fn write_raw_value(
    _location: &RegistryLocation,
    _value_type: u32,
    _bytes: &[u8],
) -> WindowsResult<()> {
    Err(WindowsError::unsupported("write registry value"))
}

#[cfg(windows)]
pub fn delete_value(location: &RegistryLocation) -> WindowsResult<()> {
    use std::io::ErrorKind;
    use winreg::enums::KEY_SET_VALUE;

    ensure_user_scope(location, "delete machine-scope registry value")?;
    let key = match root(location.hive).open_subkey_with_flags(
        &location.canonical_subkey,
        KEY_SET_VALUE | view_flag(location.view),
    ) {
        Ok(key) => key,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(map_io("open registry key for delete", error)),
    };
    match key.delete_value(&location.value_name) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(map_io("delete registry value", error)),
    }
}

#[cfg(not(windows))]
pub fn delete_value(_location: &RegistryLocation) -> WindowsResult<()> {
    Err(WindowsError::unsupported("delete registry value"))
}

#[cfg(all(test, windows))]
pub fn delete_key_if_empty(location: &RegistryLocation) -> WindowsResult<bool> {
    use std::io::ErrorKind;
    use winreg::enums::{KEY_READ, KEY_WRITE};

    ensure_user_scope(location, "delete machine-scope registry key")?;
    let key = match root(location.hive).open_subkey_with_flags(
        &location.canonical_subkey,
        KEY_READ | view_flag(location.view),
    ) {
        Ok(key) => key,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(map_io("open registry key for empty check", error)),
    };
    match key.enum_values().next() {
        Some(Ok(_)) => return Ok(false),
        Some(Err(error)) => return Err(map_io("enumerate registry values before delete", error)),
        None => {}
    }
    match key.enum_keys().next() {
        Some(Ok(_)) => return Ok(false),
        Some(Err(error)) => return Err(map_io("enumerate registry subkeys before delete", error)),
        None => {}
    }
    drop(key);

    let (parent_path, leaf) = location.canonical_subkey.rsplit_once('\\').ok_or_else(|| {
        WindowsError::new(WindowsErrorKind::InvalidData, "split registry key", None)
    })?;
    let parent = root(location.hive)
        .open_subkey_with_flags(parent_path, KEY_WRITE | view_flag(location.view))
        .map_err(|error| map_io("open registry parent key", error))?;
    match parent.delete_subkey(leaf) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(true),
        Err(error) => Err(map_io("delete empty registry key", error)),
    }
}

#[cfg(all(test, not(windows)))]
pub fn delete_key_if_empty(_location: &RegistryLocation) -> WindowsResult<bool> {
    Err(WindowsError::unsupported("delete registry key"))
}
