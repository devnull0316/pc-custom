use std::fmt;

use serde::{Deserialize, Serialize};

use crate::backup::Fingerprint;

use super::{WindowsError, WindowsErrorKind, WindowsResult};

const MAX_ENTRY_NAME_UTF16: usize = 256;
const MAX_ENUM_BUFFER_BYTES: usize = 4 * 1024 * 1024;

/// A user-assigned Windows RAS entry name.
///
/// It is serialized only because the UI must let the user select an existing
/// entry and rollback must remember the exact entry. Debug output is always
/// redacted so routine diagnostics cannot disclose the name.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VpnEntryName(String);

impl VpnEntryName {
    pub(crate) fn unselected() -> Self {
        Self(String::new())
    }

    pub(crate) fn journal_redacted() -> Self {
        Self("[REDACTED]".to_owned())
    }

    pub fn new(value: String) -> WindowsResult<Self> {
        let candidate = Self(value);
        if candidate.is_valid() {
            Ok(candidate)
        } else {
            Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "validate registered VPN entry name",
                None,
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_valid(&self) -> bool {
        !self.0.is_empty()
            && !self.0.contains('\0')
            && self.0.encode_utf16().count() <= MAX_ENTRY_NAME_UTF16
    }

    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::of_bytes(self.0.as_bytes())
    }
}

impl fmt::Debug for VpnEntryName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VpnEntryName([REDACTED])")
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VpnEntryState {
    pub name: VpnEntryName,
    pub connected: bool,
}

impl fmt::Debug for VpnEntryState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VpnEntryState")
            .field("name", &"[REDACTED]")
            .field("connected", &self.connected)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VpnInventory {
    pub entries: Vec<VpnEntryState>,
}

impl VpnInventory {
    pub fn contains(&self, name: &VpnEntryName) -> bool {
        self.entries.iter().any(|entry| &entry.name == name)
    }

    pub fn is_connected(&self, name: &VpnEntryName) -> Option<bool> {
        self.entries
            .iter()
            .find(|entry| &entry.name == name)
            .map(|entry| entry.connected)
    }

    pub fn connected_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.connected).count()
    }

    pub fn selected_fingerprint(&self, name: &VpnEntryName) -> Option<Fingerprint> {
        self.is_connected(name).map(|connected| {
            let state = [u8::from(connected)];
            Fingerprint::of_parts([name.as_str().as_bytes(), state.as_slice()])
        })
    }
}

impl fmt::Debug for VpnInventory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VpnInventory")
            .field("entry_count", &self.entries.len())
            .field("connected_count", &self.connected_count())
            .finish()
    }
}

/// An opaque RAS connection handle owned by this process. It is never
/// serialized; after a restart ownership cannot be proven and rollback must be
/// guided instead of disconnecting a connection by name.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VpnConnectionHandle(usize);

impl fmt::Debug for VpnConnectionHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VpnConnectionHandle([REDACTED])")
    }
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct ActiveConnection {
    name_hash: Fingerprint,
    handle: VpnConnectionHandle,
}

#[cfg(windows)]
fn api_error(operation: &'static str, code: u32) -> WindowsError {
    use windows::Win32::Foundation::ERROR_ACCESS_DENIED;

    WindowsError::new(
        if code == ERROR_ACCESS_DENIED.0 {
            WindowsErrorKind::AccessDenied
        } else {
            WindowsErrorKind::ApiFailure
        },
        operation,
        Some(i64::from(code)),
    )
}

#[cfg(windows)]
fn decode_fixed_wide(value: &[u16], operation: &'static str) -> WindowsResult<String> {
    let end = value
        .iter()
        .position(|unit| *unit == 0)
        .ok_or_else(|| WindowsError::new(WindowsErrorKind::InvalidData, operation, None))?;
    String::from_utf16(&value[..end])
        .map_err(|_| WindowsError::new(WindowsErrorKind::InvalidData, operation, None))
}

#[cfg(windows)]
fn wide_name(name: &VpnEntryName) -> Vec<u16> {
    name.as_str()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn enumerate_entry_names() -> WindowsResult<Vec<VpnEntryName>> {
    use std::mem::size_of;
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::ERROR_SUCCESS,
            NetworkManagement::Rras::{RasEnumEntriesW, ERROR_BUFFER_TOO_SMALL, RASENTRYNAMEW},
        },
    };

    let mut bytes = 0u32;
    let mut count = 0u32;
    let first =
        unsafe { RasEnumEntriesW(PCWSTR::null(), PCWSTR::null(), None, &mut bytes, &mut count) };
    if first == ERROR_SUCCESS.0 && count == 0 {
        return Ok(Vec::new());
    }
    if first != ERROR_BUFFER_TOO_SMALL {
        return Err(api_error("enumerate registered VPN entries", first));
    }
    if bytes == 0 || bytes as usize > MAX_ENUM_BUFFER_BYTES {
        return Err(WindowsError::new(
            WindowsErrorKind::ResourceLimit,
            "bound registered VPN entry enumeration",
            None,
        ));
    }

    let slots = (bytes as usize).div_ceil(size_of::<RASENTRYNAMEW>());
    let mut records = vec![RASENTRYNAMEW::default(); slots];
    for record in &mut records {
        record.dwSize = size_of::<RASENTRYNAMEW>() as u32;
    }
    let result = unsafe {
        RasEnumEntriesW(
            PCWSTR::null(),
            PCWSTR::null(),
            Some(records.as_mut_ptr()),
            &mut bytes,
            &mut count,
        )
    };
    if result != ERROR_SUCCESS.0 {
        return Err(api_error("enumerate registered VPN entries", result));
    }
    if count as usize > records.len() {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate registered VPN entry count",
            None,
        ));
    }
    records.truncate(count as usize);
    records
        .into_iter()
        .map(|record| {
            decode_fixed_wide(&record.szEntryName, "decode registered VPN entry name")
                .and_then(VpnEntryName::new)
        })
        .collect()
}

#[cfg(windows)]
fn entry_is_vpn(name: &VpnEntryName) -> WindowsResult<bool> {
    use std::{mem::size_of, slice};
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::ERROR_SUCCESS,
            NetworkManagement::Rras::{
                RASET_Vpn, RasGetEntryPropertiesW, ERROR_BUFFER_TOO_SMALL, RASENTRYW,
            },
        },
    };

    let encoded = wide_name(name);
    let mut bytes = size_of::<RASENTRYW>() as u32;
    let mut header = RASENTRYW {
        dwSize: size_of::<RASENTRYW>() as u32,
        ..Default::default()
    };
    let first = unsafe {
        RasGetEntryPropertiesW(
            PCWSTR::null(),
            PCWSTR(encoded.as_ptr()),
            Some(&mut header),
            &mut bytes,
            None,
            None,
        )
    };
    if first == ERROR_SUCCESS.0 {
        return Ok(header.dwType == RASET_Vpn);
    }
    if first != ERROR_BUFFER_TOO_SMALL || bytes as usize > MAX_ENUM_BUFFER_BYTES {
        return Err(api_error("read registered VPN entry properties", first));
    }

    let words = (bytes as usize).div_ceil(size_of::<usize>());
    let mut aligned = vec![0usize; words];
    let buffer = unsafe {
        slice::from_raw_parts_mut(
            aligned.as_mut_ptr().cast::<u8>(),
            aligned.len() * size_of::<usize>(),
        )
    };
    let entry = unsafe { &mut *buffer.as_mut_ptr().cast::<RASENTRYW>() };
    entry.dwSize = size_of::<RASENTRYW>() as u32;
    let result = unsafe {
        RasGetEntryPropertiesW(
            PCWSTR::null(),
            PCWSTR(encoded.as_ptr()),
            Some(entry),
            &mut bytes,
            None,
            None,
        )
    };
    if result != ERROR_SUCCESS.0 {
        return Err(api_error("read registered VPN entry properties", result));
    }
    Ok(entry.dwType == RASET_Vpn)
}

#[cfg(windows)]
fn enumerate_active_connections() -> WindowsResult<Vec<ActiveConnection>> {
    use std::mem::size_of;
    use windows::Win32::{
        Foundation::ERROR_SUCCESS,
        NetworkManagement::Rras::{RasEnumConnectionsW, ERROR_BUFFER_TOO_SMALL, RASCONNW},
    };

    let mut bytes = 0u32;
    let mut count = 0u32;
    let first = unsafe { RasEnumConnectionsW(None, &mut bytes, &mut count) };
    if first == ERROR_SUCCESS.0 && count == 0 {
        return Ok(Vec::new());
    }
    if first != ERROR_BUFFER_TOO_SMALL {
        return Err(api_error("enumerate active VPN connections", first));
    }
    if bytes == 0 || bytes as usize > MAX_ENUM_BUFFER_BYTES {
        return Err(WindowsError::new(
            WindowsErrorKind::ResourceLimit,
            "bound active VPN connection enumeration",
            None,
        ));
    }

    let slots = (bytes as usize).div_ceil(size_of::<RASCONNW>());
    let mut records = vec![RASCONNW::default(); slots];
    for record in &mut records {
        record.dwSize = size_of::<RASCONNW>() as u32;
    }
    let result = unsafe { RasEnumConnectionsW(Some(records.as_mut_ptr()), &mut bytes, &mut count) };
    if result != ERROR_SUCCESS.0 {
        return Err(api_error("enumerate active VPN connections", result));
    }
    if count as usize > records.len() {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate active VPN connection count",
            None,
        ));
    }
    records.truncate(count as usize);

    let mut active = Vec::with_capacity(records.len());
    for record in records {
        let name = VpnEntryName::new(decode_fixed_wide(
            &record.szEntryName,
            "decode active VPN entry name",
        )?)?;
        active.push(ActiveConnection {
            name_hash: name.fingerprint(),
            handle: VpnConnectionHandle(record.hrasconn.0 as usize),
        });
    }
    Ok(active)
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandleState {
    Connected,
    Disconnected,
    Transitional,
    Gone,
}

#[cfg(windows)]
fn handle_state(handle: VpnConnectionHandle) -> WindowsResult<HandleState> {
    use std::mem::size_of;
    use windows::Win32::{
        Foundation::{ERROR_INVALID_HANDLE, ERROR_SUCCESS},
        NetworkManagement::Rras::{
            RASCS_Connected, RASCS_Disconnected, RasGetConnectStatusW, HRASCONN, RASCONNSTATUSW,
        },
    };

    let mut status = RASCONNSTATUSW {
        dwSize: size_of::<RASCONNSTATUSW>() as u32,
        ..Default::default()
    };
    let code =
        unsafe { RasGetConnectStatusW(HRASCONN(handle.0 as *mut core::ffi::c_void), &mut status) };
    if code == ERROR_INVALID_HANDLE.0 {
        return Ok(HandleState::Gone);
    }
    if code != ERROR_SUCCESS.0 {
        return Err(api_error("read owned VPN connection status", code));
    }
    if status.rasconnstate == RASCS_Connected {
        Ok(HandleState::Connected)
    } else if status.rasconnstate == RASCS_Disconnected {
        Ok(HandleState::Disconnected)
    } else {
        Ok(HandleState::Transitional)
    }
}

#[cfg(windows)]
fn abandon_dial_attempt(name: &VpnEntryName, handle: VpnConnectionHandle) -> WindowsResult<()> {
    use std::{thread, time::Duration};
    use windows::Win32::{
        Foundation::{ERROR_INVALID_HANDLE, ERROR_SUCCESS},
        NetworkManagement::Rras::{RasHangUpW, HRASCONN},
    };

    if handle.0 == 0 {
        return Ok(());
    }
    let code = unsafe { RasHangUpW(HRASCONN(handle.0 as *mut core::ffi::c_void)) };
    if code != ERROR_SUCCESS.0 && code != ERROR_INVALID_HANDLE.0 {
        return Err(WindowsError::new(
            WindowsErrorKind::RecoveryRequired,
            "cancel incomplete VPN connection",
            Some(i64::from(code)),
        ));
    }
    let hash = name.fingerprint();
    for _ in 0..200 {
        let exact_handle_listed = enumerate_active_connections()?
            .iter()
            .any(|connection| connection.name_hash == hash && connection.handle == handle);
        if !exact_handle_listed && handle_state(handle)? == HandleState::Gone {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(WindowsError::new(
        WindowsErrorKind::RecoveryRequired,
        "verify incomplete VPN connection cleanup",
        None,
    ))
}

#[cfg(windows)]
fn validate_connected_attempt(
    verification: WindowsResult<bool>,
    cleanup: impl FnOnce() -> WindowsResult<()>,
) -> WindowsResult<()> {
    match verification {
        Ok(true) => Ok(()),
        Ok(false) => {
            cleanup()?;
            Err(WindowsError::new(
                WindowsErrorKind::RecoveryRequired,
                "verify registered VPN connection after apply",
                None,
            ))
        }
        Err(error) => {
            if cleanup().is_ok() {
                Err(error)
            } else {
                Err(WindowsError::new(
                    WindowsErrorKind::RecoveryRequired,
                    "cancel VPN after post-connect verification failure",
                    error.os_code,
                ))
            }
        }
    }
}

#[cfg(windows)]
pub fn read_vpn_inventory() -> WindowsResult<VpnInventory> {
    let active = enumerate_active_connections()?;
    let mut entries = Vec::new();
    for name in enumerate_entry_names()? {
        if entry_is_vpn(&name)? {
            let hash = name.fingerprint();
            entries.push(VpnEntryState {
                name,
                connected: active.iter().any(|connection| connection.name_hash == hash),
            });
        }
    }
    entries.sort_by_key(|entry| (!entry.connected, entry.name.clone()));
    entries.dedup_by(|left, right| left.name == right.name);
    Ok(VpnInventory { entries })
}

#[cfg(not(windows))]
pub fn read_vpn_inventory() -> WindowsResult<VpnInventory> {
    Err(WindowsError::unsupported("read registered VPN inventory"))
}

#[cfg(windows)]
pub fn connect_registered_vpn(name: &VpnEntryName) -> WindowsResult<VpnConnectionHandle> {
    use std::mem::size_of;
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::ERROR_SUCCESS,
            NetworkManagement::Rras::{RasDialW, HRASCONN, RASDIALPARAMSW},
        },
    };

    if !name.is_valid() {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate selected VPN entry",
            None,
        ));
    }
    let before = read_vpn_inventory()?;
    match before.is_connected(name) {
        None => {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "find selected registered VPN entry",
                None,
            ))
        }
        Some(true) => {
            return Err(WindowsError::new(
                WindowsErrorKind::ExternalConflict,
                "selected VPN connected before owned apply",
                None,
            ))
        }
        Some(false) => {}
    }

    // Only the entry name is supplied. Username, password, domain, callback,
    // phone number and encrypted-password fields remain zeroed. This process
    // never reads, receives or stores credential material.
    let mut parameters = RASDIALPARAMSW {
        dwSize: size_of::<RASDIALPARAMSW>() as u32,
        ..Default::default()
    };
    let encoded = wide_name(name);
    parameters.szEntryName[..encoded.len()].copy_from_slice(&encoded);
    let mut raw_handle = HRASCONN::default();
    let code = unsafe { RasDialW(None, PCWSTR::null(), &parameters, 0, None, &mut raw_handle) };
    let handle = VpnConnectionHandle(raw_handle.0 as usize);
    if code != ERROR_SUCCESS.0 {
        abandon_dial_attempt(name, handle)?;
        return Err(api_error(
            "connect registered VPN without receiving credentials",
            code,
        ));
    }
    let verification = (|| {
        let hash = name.fingerprint();
        let active = enumerate_active_connections()?;
        Ok(handle.0 != 0
            && handle_state(handle)? == HandleState::Connected
            && active
                .iter()
                .any(|connection| connection.handle == handle && connection.name_hash == hash))
    })();
    validate_connected_attempt(verification, || abandon_dial_attempt(name, handle))?;
    Ok(handle)
}

#[cfg(not(windows))]
pub fn connect_registered_vpn(_name: &VpnEntryName) -> WindowsResult<VpnConnectionHandle> {
    Err(WindowsError::unsupported("connect registered VPN"))
}

#[cfg(windows)]
pub fn disconnect_owned_vpn(name: &VpnEntryName, handle: VpnConnectionHandle) -> WindowsResult<()> {
    use std::{thread, time::Duration};
    use windows::Win32::{
        Foundation::ERROR_SUCCESS,
        NetworkManagement::Rras::{RasHangUpW, HRASCONN},
    };

    let hash = name.fingerprint();
    let active = enumerate_active_connections()?;
    let exact = active
        .iter()
        .filter(|connection| connection.name_hash == hash)
        .collect::<Vec<_>>();
    if exact.len() != 1
        || exact[0].handle != handle
        || handle_state(handle)? != HandleState::Connected
    {
        return Err(WindowsError::new(
            WindowsErrorKind::ExternalConflict,
            "owned VPN state changed before disconnect",
            None,
        ));
    }

    let code = unsafe { RasHangUpW(HRASCONN(handle.0 as *mut core::ffi::c_void)) };
    if code != ERROR_SUCCESS.0 {
        return Err(api_error("disconnect owned VPN connection", code));
    }
    for _ in 0..200 {
        let still_listed = enumerate_active_connections()?
            .iter()
            .any(|connection| connection.name_hash == hash);
        if !still_listed && handle_state(handle)? == HandleState::Gone {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(WindowsError::new(
        WindowsErrorKind::RecoveryRequired,
        "wait for owned VPN disconnection",
        None,
    ))
}

#[cfg(not(windows))]
pub fn disconnect_owned_vpn(
    _name: &VpnEntryName,
    _handle: VpnConnectionHandle,
) -> WindowsResult<()> {
    Err(WindowsError::unsupported("disconnect owned VPN"))
}

#[cfg(all(test, windows))]
pub(crate) struct TestVpnEntryGuard {
    name: VpnEntryName,
    deleted: bool,
}

#[cfg(all(test, windows))]
impl TestVpnEntryGuard {
    pub(crate) fn create_unreachable(suffix: &str) -> WindowsResult<Self> {
        use std::mem::size_of;
        use windows::{
            core::PCWSTR,
            Win32::{
                Foundation::ERROR_SUCCESS,
                NetworkManagement::Rras::{RASET_Vpn, RasSetEntryPropertiesW, RASENTRYW},
            },
        };

        let name_str = format!("pc-custom-test-{suffix}");
        let name = VpnEntryName::new(name_str)?;
        let encoded_name = wide_name(&name);

        let mut entry = RASENTRYW {
            dwSize: size_of::<RASENTRYW>() as u32,
            dwType: RASET_Vpn,
            ..Default::default()
        };

        let address = wide_string("198.51.100.1");
        let len = address.len().min(entry.szLocalPhoneNumber.len());
        entry.szLocalPhoneNumber[..len].copy_from_slice(&address[..len]);

        let dev_type = wide_string("VPN");
        let dev_len = dev_type.len().min(entry.szDeviceType.len());
        entry.szDeviceType[..dev_len].copy_from_slice(&dev_type[..dev_len]);

        let code = unsafe {
            RasSetEntryPropertiesW(
                PCWSTR::null(),
                PCWSTR(encoded_name.as_ptr()),
                &entry as *const RASENTRYW,
                size_of::<RASENTRYW>() as u32,
                None,
                0,
            )
        };
        if code != ERROR_SUCCESS.0 {
            return Err(api_error("create test VPN entry", code));
        }

        Ok(Self {
            name,
            deleted: false,
        })
    }

    pub(crate) fn name(&self) -> &VpnEntryName {
        &self.name
    }

    pub(crate) fn cleanup(&mut self) -> WindowsResult<()> {
        if self.deleted {
            return Ok(());
        }
        use windows::{
            core::PCWSTR,
            Win32::{
                Foundation::ERROR_SUCCESS,
                NetworkManagement::Rras::{RasDeleteEntryW, ERROR_CANNOT_FIND_PHONEBOOK_ENTRY},
            },
        };
        let encoded_name = wide_name(&self.name);
        let code = unsafe { RasDeleteEntryW(PCWSTR::null(), PCWSTR(encoded_name.as_ptr())) };
        self.deleted = true;
        if code == ERROR_SUCCESS.0 || code == ERROR_CANNOT_FIND_PHONEBOOK_ENTRY {
            Ok(())
        } else {
            Err(api_error("delete test VPN entry", code))
        }
    }
}

#[cfg(all(test, windows))]
impl Drop for TestVpnEntryGuard {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

#[cfg(all(test, windows))]
fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(all(test, windows))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VpnProbe {
    pub registered_count: usize,
    pub connected_hashes: Vec<Fingerprint>,
}

#[cfg(all(test, windows))]
pub(crate) fn read_vpn_probe_in_child() -> WindowsResult<VpnProbe> {
    use std::process::Command;

    const HELPER: &str = "windows::vpn::tests::vpn_probe_helper";
    const PREFIX: &str = "VPN_PROBE_SAFE=";

    let output = Command::new(
        std::env::current_exe()
            .map_err(|error| WindowsError::io("locate VPN probe helper", &error))?,
    )
    .args(["--ignored", "--exact", HELPER, "--nocapture"])
    .output()
    .map_err(|error| WindowsError::io("run VPN probe helper", &error))?;
    if !output.status.success() {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "VPN probe helper failed",
            output.status.code().map(i64::from),
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "decode VPN probe helper output",
            None,
        )
    })?;
    let encoded = stdout
        .lines()
        .find_map(|line| line.strip_prefix(PREFIX))
        .ok_or_else(|| {
            WindowsError::new(
                WindowsErrorKind::InvalidData,
                "find VPN probe helper result",
                None,
            )
        })?;
    let (count, hashes) = encoded.split_once(';').ok_or_else(|| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "split VPN probe helper result",
            None,
        )
    })?;
    let registered_count = count.parse::<usize>().map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "decode VPN probe registered count",
            None,
        )
    })?;
    let connected_hashes = if hashes.is_empty() {
        Vec::new()
    } else {
        hashes
            .split(',')
            .map(|encoded_hash| {
                let bytes = hex::decode(encoded_hash).map_err(|_| {
                    WindowsError::new(
                        WindowsErrorKind::InvalidData,
                        "decode VPN probe connection hash",
                        None,
                    )
                })?;
                let hash: [u8; 32] = bytes.try_into().map_err(|_| {
                    WindowsError::new(
                        WindowsErrorKind::InvalidData,
                        "validate VPN probe connection hash",
                        None,
                    )
                })?;
                Ok(Fingerprint(hash))
            })
            .collect::<WindowsResult<Vec<_>>>()?
    };
    Ok(VpnProbe {
        registered_count,
        connected_hashes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vpn_names_and_handles_are_redacted_from_debug_output() {
        let private = "Confidential organization connection";
        let name = VpnEntryName::new(private.to_owned()).expect("valid test name");
        let entry = VpnEntryState {
            name: name.clone(),
            connected: true,
        };
        let inventory = VpnInventory {
            entries: vec![entry.clone()],
        };
        assert!(!format!("{name:?}").contains(private));
        assert!(!format!("{entry:?}").contains(private));
        assert!(!format!("{inventory:?}").contains(private));
        assert!(!format!("{:?}", VpnConnectionHandle(123)).contains("123"));
    }

    #[test]
    fn selected_fingerprint_changes_with_connection_state() {
        let name = VpnEntryName::new("test entry".to_owned()).expect("valid test name");
        let inventory = |connected| VpnInventory {
            entries: vec![VpnEntryState {
                name: name.clone(),
                connected,
            }],
        };
        assert_ne!(
            inventory(false).selected_fingerprint(&name),
            inventory(true).selected_fingerprint(&name)
        );
    }

    #[cfg(windows)]
    #[test]
    fn verification_error_after_vpn_dial_still_abandons_the_owned_handle() {
        let cleanup_called = std::cell::Cell::new(false);
        let verification_error = WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "injected post-connect verification failure",
            Some(123),
        );

        let error = validate_connected_attempt(Err(verification_error), || {
            cleanup_called.set(true);
            Ok(())
        })
        .expect_err("verification failure must still be returned");
        assert!(cleanup_called.get());
        assert_eq!(
            error.operation,
            "injected post-connect verification failure"
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "helper process for independent RAS connection readback"]
    fn vpn_probe_helper() {
        match read_vpn_inventory() {
            Ok(inventory) => {
                let hashes = inventory
                    .entries
                    .iter()
                    .filter(|entry| entry.connected)
                    .map(|entry| hex::encode(entry.name.fingerprint().0))
                    .collect::<Vec<_>>()
                    .join(",");
                println!("VPN_PROBE_SAFE={};{hashes}", inventory.entries.len());
            }
            Err(error) => println!("VPN_PROBE_UNAVAILABLE reason={:?}", error.os_code),
        }
    }
}
