use crate::action::ProcessFileIdentity;

use super::{wmi_process_ids, WindowsError, WindowsErrorKind, WindowsResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub process_id: u32,
    pub creation_time_100ns: u64,
    pub canonical_path: String,
    pub file_identity: ProcessFileIdentity,
    pub corroborated_by_wmi: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSnapshotReport {
    pub processes: Vec<ProcessIdentity>,
    pub inaccessible_or_vanished: usize,
    pub inaccessible_executable_names: Vec<String>,
    pub first_identity_error_code: Option<i64>,
    pub wmi_available: bool,
    pub wmi_failure_operation: Option<&'static str>,
    pub wmi_failure_code: Option<i64>,
}

#[cfg(windows)]
fn file_identity_from_path(path: &std::path::Path) -> WindowsResult<ProcessFileIdentity> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION},
    };

    let file = std::fs::File::open(path)
        .map_err(|error| WindowsError::io("open file for identity", &error))?;
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut information)
    }
    .map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "GetFileInformationByHandle",
            Some(i64::from(error.code().0)),
        )
    })?;
    let file_index = (u64::from(information.nFileIndexHigh) << 32)
        | u64::from(information.nFileIndexLow);
    let mut file_id = [0u8; 16];
    file_id[..8].copy_from_slice(&file_index.to_le_bytes());
    Ok(ProcessFileIdentity {
        volume_serial_number: u64::from(information.dwVolumeSerialNumber),
        file_id,
    })
}

#[cfg(windows)]
pub fn registered_file_identity(path: &str) -> WindowsResult<(String, ProcessFileIdentity)> {
    use std::{os::windows::fs::MetadataExt, path::Path};
    use windows::Win32::Storage::FileSystem::{
        GetDriveTypeW, FILE_ATTRIBUTE_REPARSE_POINT,
    };
    use windows::Win32::System::WindowsProgramming::DRIVE_FIXED;

    let candidate = Path::new(path);
    if !candidate.is_absolute()
        || path.starts_with(r"\\")
        || path.starts_with(r"\\?\")
        || path.starts_with(r"\\.\")
    {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "reject non-local process binding path",
            None,
        ));
    }
    let original_metadata = std::fs::symlink_metadata(candidate)
        .map_err(|error| WindowsError::io("read process binding metadata", &error))?;
    if original_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        || !original_metadata.is_file()
    {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "reject process binding reparse or non-file",
            None,
        ));
    }

    let canonical = std::fs::canonicalize(candidate)
        .map_err(|error| WindowsError::io("canonicalize process binding", &error))?;
    let canonical_text = canonical.to_str().ok_or_else(|| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate process binding Unicode path",
            None,
        )
    })?;
    let canonical_text = canonical_text
        .strip_prefix(r"\\?\")
        .unwrap_or(canonical_text);
    let drive_prefix = canonical_text
        .get(..3)
        .filter(|prefix| prefix.as_bytes().get(1) == Some(&b':'))
        .ok_or_else(|| {
            WindowsError::new(
                WindowsErrorKind::InvalidData,
                "validate process binding drive",
                None,
            )
        })?;
    let drive_wide: Vec<u16> = format!("{}\0", drive_prefix).encode_utf16().collect();
    if unsafe { GetDriveTypeW(windows::core::PCWSTR(drive_wide.as_ptr())) } != DRIVE_FIXED {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "reject non-fixed process binding drive",
            None,
        ));
    }
    let file_identity = file_identity_from_path(&canonical)?;
    Ok((canonical_text.to_owned(), file_identity))
}

#[cfg(not(windows))]
pub fn registered_file_identity(_path: &str) -> WindowsResult<(String, ProcessFileIdentity)> {
    Err(WindowsError::unsupported("validate process binding identity"))
}

#[cfg(windows)]
struct OwnedHandle(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if let Err(error) = unsafe { windows::Win32::Foundation::CloseHandle(self.0) } {
            eprintln!(
                "CloseHandle for process observation failed (OS code {})",
                error.code().0
            );
        }
    }
}

#[cfg(windows)]
pub fn snapshot_process_identities() -> WindowsResult<ProcessSnapshotReport> {
    use windows::{
        core::HRESULT,
        Win32::{
            Foundation::ERROR_NO_MORE_FILES,
            System::Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
        },
    };

    let snapshot = OwnedHandle(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.map_err(|error| {
            WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "CreateToolhelp32Snapshot",
                Some(i64::from(error.code().0)),
            )
        })?,
    );
    let (wmi_ids, wmi_failure_operation, wmi_failure_code) = match wmi_process_ids() {
        Ok(ids) => (ids, None, None),
        Err(error) => (
            Default::default(),
            Some(error.operation),
            error.os_code,
        ),
    };
    let wmi_available = wmi_failure_operation.is_none();

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe { Process32FirstW(snapshot.0, &mut entry) }.map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "Process32FirstW",
            Some(i64::from(error.code().0)),
        )
    })?;

    let mut report = ProcessSnapshotReport {
        processes: Vec::new(),
        inaccessible_executable_names: Vec::new(),
        inaccessible_or_vanished: 0,
        first_identity_error_code: None,
        wmi_available,
        wmi_failure_operation,
        wmi_failure_code,
    };
    loop {
        let process_id = entry.th32ProcessID;
        if process_id != 0 {
            match identity_for_process(process_id, wmi_ids.contains(&process_id)) {
                Ok(identity) => report.processes.push(identity),
                Err(error) => {
                    let end = entry
                        .szExeFile
                        .iter()
                        .position(|value| *value == 0)
                        .unwrap_or(entry.szExeFile.len());
                    report.inaccessible_executable_names.push(String::from_utf16_lossy(&entry.szExeFile[..end]));
                    report.inaccessible_or_vanished += 1;
                    if report.first_identity_error_code.is_none() {
                        report.first_identity_error_code = error.os_code;
                    }
                }
            }
        }
        match unsafe { Process32NextW(snapshot.0, &mut entry) } {
            Ok(()) => {}
            Err(error)
                if error.code() == HRESULT::from_win32(ERROR_NO_MORE_FILES.0) =>
            {
                break;
            }
            Err(error) => {
                return Err(WindowsError::new(
                    WindowsErrorKind::ApiFailure,
                    "Process32NextW",
                    Some(i64::from(error.code().0)),
                ));
            }
        }
    }
    Ok(report)
}

#[cfg(windows)]
fn identity_for_process(process_id: u32, corroborated_by_wmi: bool) -> WindowsResult<ProcessIdentity> {
    use std::os::windows::fs::MetadataExt;
    use windows::Win32::{
        Foundation::FILETIME,
        Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, SYNCHRONIZE},
        System::Threading::{
            GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_ACCESS_RIGHTS,
            PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    };

    let process = OwnedHandle(
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_ACCESS_RIGHTS(SYNCHRONIZE.0),
                false,
                process_id,
            )
        }
        .map_err(|error| {
            WindowsError::new(
                if error.code()
                    == windows::core::HRESULT::from_win32(windows::Win32::Foundation::ERROR_ACCESS_DENIED.0) {
                    WindowsErrorKind::AccessDenied
                } else {
                    WindowsErrorKind::ApiFailure
                },
                "OpenProcess for identity",
                Some(i64::from(error.code().0)),
            )
        })?,
    );

    let mut capacity = 512usize;
    let image_path = loop {
        if capacity > 32_768 {
            return Err(WindowsError::new(
                WindowsErrorKind::ResourceLimit,
                "QueryFullProcessImageNameW bounded path",
                None,
            ));
        }
        let mut buffer = vec![0u16; capacity];
        let mut length = buffer.len() as u32;
        match unsafe {
            QueryFullProcessImageNameW(
                process.0,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut length,
            )
        } {
            Ok(()) => {
                buffer.truncate(length as usize);
                break String::from_utf16(&buffer).map_err(|_| {
                    WindowsError::new(
                        WindowsErrorKind::InvalidData,
                        "decode process image path",
                        None,
                    )
                })?;
            }
            Err(error) if error.code()
                == windows::core::HRESULT::from_win32(windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER.0) => {
                capacity *= 2;
            }
            Err(error) => {
                return Err(WindowsError::new(
                    WindowsErrorKind::ApiFailure,
                    "QueryFullProcessImageNameW",
                    Some(i64::from(error.code().0)),
                ));
            }
        }
    };

    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetProcessTimes(
            process.0,
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    }
    .map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "GetProcessTimes",
            Some(i64::from(error.code().0)),
        )
    })?;
    let creation_time_100ns =
        (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);

    let canonical = std::fs::canonicalize(&image_path)
        .map_err(|error| WindowsError::io("canonicalize process image", &error))?;
    let metadata = std::fs::metadata(&canonical)
        .map_err(|error| WindowsError::io("read process image identity", &error))?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 || !metadata.is_file() {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "reject process image reparse or non-file",
            None,
        ));
    }
    let file_identity = file_identity_from_path(&canonical)?;
    let canonical_path = canonical
        .to_str()
        .ok_or_else(|| {
            WindowsError::new(
                WindowsErrorKind::InvalidData,
                "validate process image Unicode path",
                None,
            )
        })?;
    let canonical_path = canonical_path
        .strip_prefix(r"\\?\")
        .unwrap_or(canonical_path)
        .to_owned();

    Ok(ProcessIdentity {
        process_id,
        creation_time_100ns,
        canonical_path,
        file_identity,
        corroborated_by_wmi,
    })
}

#[cfg(not(windows))]
pub fn snapshot_process_identities() -> WindowsResult<ProcessSnapshotReport> {
    Err(WindowsError::unsupported("snapshot process identities"))
}
