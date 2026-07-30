use std::fmt;

use serde::{Deserialize, Serialize};

use crate::backup::Fingerprint;

use super::{WindowsError, WindowsErrorKind, WindowsResult};

const MAX_PRINTER_NAME_UTF16: usize = 1_024;
const MAX_ENUM_BUFFER_BYTES: usize = 4 * 1024 * 1024;

/// A user-assigned printer name.
///
/// The value is serialized for the UI and durable rollback, but its `Debug`
/// representation is always redacted so routine diagnostics cannot disclose it.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrinterName(String);

impl PrinterName {
    pub(crate) fn unselected() -> Self {
        Self(String::new())
    }

    pub fn new(value: String) -> WindowsResult<Self> {
        let candidate = Self(value);
        if candidate.is_valid() {
            Ok(candidate)
        } else {
            Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "validate printer name",
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
            && self.0.encode_utf16().count() <= MAX_PRINTER_NAME_UTF16
    }

    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::of_bytes(self.0.as_bytes())
    }
}

impl fmt::Debug for PrinterName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PrinterName([REDACTED])")
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultPrinterInventory {
    pub default: PrinterName,
    pub printers: Vec<PrinterName>,
    pub windows_managed: bool,
}

impl DefaultPrinterInventory {
    pub fn fingerprint(&self) -> Fingerprint {
        let managed = [u8::from(self.windows_managed)];
        Fingerprint::of_parts([managed.as_slice(), self.default.as_str().as_bytes()])
    }

    pub fn contains(&self, name: &PrinterName) -> bool {
        self.printers.iter().any(|candidate| candidate == name)
    }
}

impl fmt::Debug for DefaultPrinterInventory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DefaultPrinterInventory")
            .field("default", &"[REDACTED]")
            .field("printer_count", &self.printers.len())
            .field("windows_managed", &self.windows_managed)
            .finish()
    }
}

#[cfg(windows)]
fn api_error(operation: &'static str) -> WindowsError {
    use windows::Win32::Foundation::{GetLastError, ERROR_ACCESS_DENIED};

    let code = unsafe { GetLastError() };
    WindowsError::new(
        if code == ERROR_ACCESS_DENIED {
            WindowsErrorKind::AccessDenied
        } else {
            WindowsErrorKind::ApiFailure
        },
        operation,
        Some(i64::from(code.0)),
    )
}

#[cfg(windows)]
fn read_default_printer() -> WindowsResult<PrinterName> {
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER},
            Graphics::Printing::GetDefaultPrinterW,
        },
    };

    let mut required = 0u32;
    let first = unsafe { GetDefaultPrinterW(PWSTR::null(), &mut required) };
    let first_error = unsafe { GetLastError() };
    if first.as_bool()
        || first_error != ERROR_INSUFFICIENT_BUFFER
        || required == 0
        || required as usize > MAX_PRINTER_NAME_UTF16 + 1
    {
        return Err(api_error("measure default printer name"));
    }

    let mut buffer = vec![0u16; required as usize];
    if !unsafe { GetDefaultPrinterW(PWSTR(buffer.as_mut_ptr()), &mut required) }.as_bool() {
        return Err(api_error("read default printer"));
    }
    let end = buffer.iter().position(|unit| *unit == 0).ok_or_else(|| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate default printer name",
            None,
        )
    })?;
    let name = String::from_utf16(&buffer[..end]).map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "decode default printer name",
            None,
        )
    })?;
    PrinterName::new(name)
}

#[cfg(windows)]
fn windows_manages_default_printer() -> WindowsResult<bool> {
    use std::io::ErrorKind;
    use winreg::{
        enums::{HKEY_CURRENT_USER, KEY_READ},
        RegKey,
    };

    let root = RegKey::predef(HKEY_CURRENT_USER);
    let key = match root.open_subkey_with_flags(
        r"Software\Microsoft\Windows NT\CurrentVersion\Windows",
        KEY_READ,
    ) {
        Ok(key) => key,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(WindowsError::io("read default printer policy key", &error)),
    };
    match key.get_value::<u32, _>("LegacyDefaultPrinterMode") {
        Ok(1) => Ok(false),
        Ok(0) => Ok(true),
        Ok(_) => Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate default printer policy",
            None,
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(true),
        Err(error) => Err(WindowsError::io(
            "read default printer policy value",
            &error,
        )),
    }
}

#[cfg(windows)]
pub fn enumerate_installed_printers() -> WindowsResult<Vec<PrinterName>> {
    use std::{mem::size_of, slice};
    use windows::{
        core::{PCWSTR, PWSTR},
        Win32::Graphics::Printing::{
            EnumPrintersW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_4W,
        },
    };

    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let mut needed = 0u32;
    let mut returned = 0u32;
    let _ = unsafe { EnumPrintersW(flags, PCWSTR::null(), 4, None, &mut needed, &mut returned) };
    if needed == 0 {
        return Ok(Vec::new());
    }
    if needed as usize > MAX_ENUM_BUFFER_BYTES {
        return Err(WindowsError::new(
            WindowsErrorKind::ResourceLimit,
            "bound installed printer enumeration",
            None,
        ));
    }

    let word_count = (needed as usize).div_ceil(size_of::<usize>());
    let mut aligned = vec![0usize; word_count];
    let bytes = unsafe {
        slice::from_raw_parts_mut(
            aligned.as_mut_ptr().cast::<u8>(),
            aligned.len() * size_of::<usize>(),
        )
    };
    unsafe {
        EnumPrintersW(
            flags,
            PCWSTR::null(),
            4,
            Some(bytes),
            &mut needed,
            &mut returned,
        )
    }
    .map_err(|_| api_error("enumerate installed printers"))?;

    let available_records = bytes.len() / size_of::<PRINTER_INFO_4W>();
    if returned as usize > available_records {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate installed printer enumeration",
            None,
        ));
    }
    let records = unsafe {
        slice::from_raw_parts(
            aligned.as_ptr().cast::<PRINTER_INFO_4W>(),
            returned as usize,
        )
    };
    let mut printers = Vec::with_capacity(records.len());
    for record in records {
        let PWSTR(pointer) = record.pPrinterName;
        if pointer.is_null() {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "validate installed printer name",
                None,
            ));
        }
        let value = unsafe { PCWSTR(pointer).to_string() }.map_err(|_| {
            WindowsError::new(
                WindowsErrorKind::InvalidData,
                "decode installed printer name",
                None,
            )
        })?;
        printers.push(PrinterName::new(value)?);
    }
    printers.sort();
    printers.dedup();
    Ok(printers)
}

#[cfg(not(windows))]
pub fn enumerate_installed_printers() -> WindowsResult<Vec<PrinterName>> {
    Err(WindowsError::unsupported("enumerate installed printers"))
}

#[cfg(windows)]
pub fn read_default_printer_inventory() -> WindowsResult<DefaultPrinterInventory> {
    let default = read_default_printer()?;
    let mut printers = enumerate_installed_printers()?;
    if !printers.iter().any(|candidate| candidate == &default) {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "match default printer to installed printers",
            None,
        ));
    }
    printers.sort_by_key(|candidate| candidate != &default);
    Ok(DefaultPrinterInventory {
        default,
        printers,
        windows_managed: windows_manages_default_printer()?,
    })
}

#[cfg(not(windows))]
pub fn read_default_printer_inventory() -> WindowsResult<DefaultPrinterInventory> {
    Err(WindowsError::unsupported("read default printer inventory"))
}

#[cfg(windows)]
pub fn replace_default_printer(
    expected: &PrinterName,
    intended: &PrinterName,
) -> WindowsResult<DefaultPrinterInventory> {
    use windows::{core::PCWSTR, Win32::Graphics::Printing::SetDefaultPrinterW};

    if !expected.is_valid() || !intended.is_valid() {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate default printer replacement",
            None,
        ));
    }
    let current = read_default_printer_inventory()?;
    if current.windows_managed || current.default != *expected || !current.contains(intended) {
        return Err(WindowsError::new(
            WindowsErrorKind::ExternalConflict,
            "default printer changed before write",
            None,
        ));
    }

    let mut wide: Vec<u16> = intended.as_str().encode_utf16().collect();
    wide.push(0);
    if !unsafe { SetDefaultPrinterW(PCWSTR(wide.as_ptr())) }.as_bool() {
        return Err(api_error("set default printer"));
    }

    let after = read_default_printer_inventory().map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::RecoveryRequired,
            "read default printer after write",
            error.os_code,
        )
    })?;
    if after.windows_managed || after.default != *intended {
        return Err(WindowsError::new(
            WindowsErrorKind::RecoveryRequired,
            "verify default printer after write",
            None,
        ));
    }
    Ok(after)
}

#[cfg(not(windows))]
pub fn replace_default_printer(
    _expected: &PrinterName,
    _intended: &PrinterName,
) -> WindowsResult<DefaultPrinterInventory> {
    Err(WindowsError::unsupported("replace default printer"))
}

#[cfg(all(test, windows))]
fn print_dialog_default_printer() -> WindowsResult<PrinterName> {
    use std::{mem::size_of, slice};
    use windows::Win32::{
        Foundation::{GlobalFree, HGLOBAL},
        System::Memory::{GlobalLock, GlobalSize, GlobalUnlock},
        UI::Controls::Dialogs::{
            PrintDlgExW, DEVNAMES, PD_NOWARNING, PD_RETURNDEFAULT, PRINTDLGEXW,
        },
    };

    fn read_device_name(handle: HGLOBAL) -> WindowsResult<PrinterName> {
        if handle.is_invalid() {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "validate print dialog device names",
                None,
            ));
        }
        let size = unsafe { GlobalSize(handle) };
        if size < size_of::<DEVNAMES>() || size > MAX_ENUM_BUFFER_BYTES {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "bound print dialog device names",
                None,
            ));
        }
        let pointer = unsafe { GlobalLock(handle) };
        if pointer.is_null() {
            return Err(api_error("lock print dialog device names"));
        }
        let result = (|| {
            let header = unsafe { pointer.cast::<DEVNAMES>().read_unaligned() };
            let units = size / size_of::<u16>();
            let offset = usize::from(header.wDeviceOffset);
            if offset >= units {
                return Err(WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "validate print dialog device offset",
                    None,
                ));
            }
            let all = unsafe { slice::from_raw_parts(pointer.cast::<u16>(), units) };
            let tail = &all[offset..];
            let end = tail.iter().position(|unit| *unit == 0).ok_or_else(|| {
                WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "validate print dialog device name",
                    None,
                )
            })?;
            let value = String::from_utf16(&tail[..end]).map_err(|_| {
                WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "decode print dialog device name",
                    None,
                )
            })?;
            PrinterName::new(value)
        })();
        let _ = unsafe { GlobalUnlock(handle) };
        result
    }

    let mut dialog = PRINTDLGEXW {
        lStructSize: size_of::<PRINTDLGEXW>() as u32,
        Flags: PD_RETURNDEFAULT | PD_NOWARNING,
        ..Default::default()
    };
    unsafe { PrintDlgExW(&mut dialog) }.map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "read print dialog default",
            Some(i64::from(error.code().0)),
        )
    })?;
    let result = read_device_name(dialog.hDevNames);
    if !dialog.hDevMode.is_invalid() {
        let _ = unsafe { GlobalFree(dialog.hDevMode) };
    }
    if !dialog.hDevNames.is_invalid() {
        let _ = unsafe { GlobalFree(dialog.hDevNames) };
    }
    result
}

#[cfg(all(test, windows))]
pub(crate) fn read_print_dialog_default_in_child() -> WindowsResult<Fingerprint> {
    use std::process::Command;

    const HELPER: &str = "windows::printer::tests::print_dialog_default_probe_helper";
    const PREFIX: &str = "DEFAULT_PRINTER_PROBE_HASH=";

    let output = Command::new(
        std::env::current_exe()
            .map_err(|error| WindowsError::io("locate default printer probe helper", &error))?,
    )
    .args(["--ignored", "--exact", HELPER, "--nocapture"])
    .output()
    .map_err(|error| WindowsError::io("run default printer probe helper", &error))?;
    if !output.status.success() {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "default printer probe helper failed",
            output.status.code().map(i64::from),
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "decode default printer probe helper output",
            None,
        )
    })?;
    let encoded = stdout
        .lines()
        .find_map(|line| line.strip_prefix(PREFIX))
        .ok_or_else(|| {
            WindowsError::new(
                WindowsErrorKind::InvalidData,
                "find default printer probe helper result",
                None,
            )
        })?;
    let bytes = hex::decode(encoded).map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "decode default printer probe hash",
            None,
        )
    })?;
    let hash: [u8; 32] = bytes.try_into().map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate default printer probe hash",
            None,
        )
    })?;
    Ok(Fingerprint(hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printer_names_are_redacted_from_debug_output() {
        let private = "Office printer owned by Person";
        let name = PrinterName::new(private.to_owned()).expect("valid test name");
        assert!(!format!("{name:?}").contains(private));
        let inventory = DefaultPrinterInventory {
            default: name.clone(),
            printers: vec![name],
            windows_managed: false,
        };
        assert!(!format!("{inventory:?}").contains(private));
    }

    #[test]
    fn inventory_fingerprint_includes_management_mode_and_exact_default() {
        let one = PrinterName::new("one".to_owned()).expect("name");
        let two = PrinterName::new("two".to_owned()).expect("name");
        let inventory = |default, windows_managed| DefaultPrinterInventory {
            default,
            printers: vec![one.clone(), two.clone()],
            windows_managed,
        };
        assert_ne!(
            inventory(one.clone(), false).fingerprint(),
            inventory(two.clone(), false).fingerprint()
        );
        assert_ne!(
            inventory(one.clone(), false).fingerprint(),
            inventory(one.clone(), true).fingerprint()
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "helper process for independent PRINTDLGEXW default-printer readback"]
    fn print_dialog_default_probe_helper() {
        // 親から子プロセスとして起動される観測ヘルパー。
        //
        // 単体で走らせると親ウィンドウが無く `PRINTDLGEXW` が
        // `ERROR_INVALID_HANDLE` を返す。**それは測れないだけで、失敗ではない。**
        // panic すると ignored 一括実行が赤くなり、
        // 「測れなかった」と「壊れている」が区別できなくなる。
        // 親は `DEFAULT_PRINTER_PROBE_HASH=` の行だけを読むので、
        // 測れないときは理由を出して静かに終わる。
        match print_dialog_default_printer() {
            Ok(name) => println!(
                "DEFAULT_PRINTER_PROBE_HASH={}",
                hex::encode(name.fingerprint().0)
            ),
            Err(error) => println!(
                "DEFAULT_PRINTER_PROBE_UNAVAILABLE reason={:?}",
                error.os_code
            ),
        }
    }
}
