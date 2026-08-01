use crate::action::{
    ObservationWarning, StartupEntrySource, StartupEntryStatus, StartupInventoryEntry,
    StartupInventoryObservation, SystemDriveSpaceObservation, TempFilesObservation,
};

use super::{WindowsError, WindowsErrorKind, WindowsResult};

const MAX_STARTUP_ENTRIES: usize = 256;
const MAX_STARTUP_VALUE_BYTES: usize = 4 * 1024;
const MAX_STARTUP_NAME_CHARS: usize = 256;
const MAX_WARNINGS: usize = 32;
pub(crate) const MAX_TEMP_ENTRIES: u64 = 5_000;
pub(crate) const MAX_TEMP_DIRECTORIES: u64 = 512;
pub(crate) const MAX_TEMP_DEPTH: u8 = 8;
pub(crate) const MAX_TEMP_SCAN_DURATION_MS: u64 = 300;
pub(crate) const MAX_TEMP_TOTAL_BYTES: u64 = 512 * 1024 * 1024 * 1024;

fn warning(source: &'static str, code: &'static str) -> ObservationWarning {
    ObservationWarning {
        source: source.to_owned(),
        code: code.to_owned(),
    }
}

fn push_warning(warnings: &mut Vec<ObservationWarning>, source: &'static str, code: &'static str) {
    if warnings.len() < MAX_WARNINGS
        && !warnings
            .iter()
            .any(|value| value.source == source && value.code == code)
    {
        warnings.push(warning(source, code));
    }
}

#[cfg(windows)]
fn api_error(operation: &'static str, error: windows::core::Error) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::ApiFailure,
        operation,
        Some(i64::from(error.code().0)),
    )
}

#[cfg(windows)]
fn bounded_name(value: &std::ffi::OsStr) -> String {
    value
        .to_string_lossy()
        .chars()
        .map(|character| {
            if character.is_control() {
                '\u{fffd}'
            } else {
                character
            }
        })
        .take(MAX_STARTUP_NAME_CHARS)
        .collect()
}

#[cfg(windows)]
pub(crate) fn is_local_disk_path(path: &std::path::Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, Prefix};
    use windows::{
        core::PCWSTR,
        Win32::{Storage::FileSystem::GetDriveTypeW, System::WindowsProgramming::DRIVE_FIXED},
    };

    if !path.is_absolute() {
        return false;
    }
    let drive = match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => letter,
            _ => return false,
        },
        _ => return false,
    };
    let root = format!("{}:{}", char::from(drive), std::path::MAIN_SEPARATOR);
    let mut wide = std::ffi::OsStr::new(&root)
        .encode_wide()
        .collect::<Vec<_>>();
    wide.push(0);
    unsafe { GetDriveTypeW(PCWSTR::from_raw(wide.as_ptr())) == DRIVE_FIXED }
}

#[cfg(windows)]
pub(crate) fn path_has_reparse_component(path: &std::path::Path) -> std::io::Result<bool> {
    use std::os::windows::fs::MetadataExt;
    use std::path::Component;
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_) | Component::RootDir) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&current)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(windows)]
fn push_startup_entry(
    report: &mut StartupInventoryObservation,
    entry: StartupInventoryEntry,
) -> bool {
    if report.entries.len() >= MAX_STARTUP_ENTRIES {
        report.truncated = true;
        push_warning(
            &mut report.warnings,
            "startup_inventory",
            "entry_limit_reached",
        );
        return false;
    }
    report.entries.push(entry);
    true
}

#[cfg(windows)]
fn registry_string_is_well_formed(bytes: &[u8]) -> bool {
    if bytes.len() < 2 || bytes.len() % 2 != 0 {
        return false;
    }
    let mut units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    if units.last().copied() != Some(0) {
        return false;
    }
    while units.last().copied() == Some(0) {
        units.pop();
    }
    !units.is_empty()
        && !units.contains(&0)
        && String::from_utf16(&units)
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
}

#[cfg(windows)]
fn registry_entry_status(value: &winreg::RegValue) -> StartupEntryStatus {
    use winreg::enums::{REG_EXPAND_SZ, REG_SZ};

    if value.bytes.len() > MAX_STARTUP_VALUE_BYTES {
        return StartupEntryStatus::RegistryValueTooLarge;
    }
    let value_type = value.vtype.clone() as u32;
    if value_type != REG_SZ as u32 && value_type != REG_EXPAND_SZ as u32 {
        return StartupEntryStatus::UnsupportedRegistryType;
    }
    if !registry_string_is_well_formed(&value.bytes) {
        return StartupEntryStatus::MalformedRegistryValue;
    }
    if value_type == REG_EXPAND_SZ as u32 {
        StartupEntryStatus::RegistryExpandableCommand
    } else {
        StartupEntryStatus::RegistryCommand
    }
}

#[cfg(windows)]
fn enumerate_run_key(
    report: &mut StartupInventoryObservation,
    hive: winreg::HKEY,
    view_flag: u32,
    source: StartupEntrySource,
    source_label: &'static str,
) {
    use std::io::ErrorKind;
    use winreg::enums::KEY_READ;

    let key = match winreg::RegKey::predef(hive).open_subkey_with_flags(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        KEY_READ | view_flag,
    ) {
        Ok(key) => key,
        Err(error) if error.kind() == ErrorKind::NotFound => return,
        Err(_) => {
            push_warning(
                &mut report.warnings,
                source_label,
                "registry_key_unreadable",
            );
            return;
        }
    };

    for value in key.enum_values() {
        let (name, raw_value) = match value {
            Ok(value) => value,
            Err(_) => {
                push_warning(
                    &mut report.warnings,
                    source_label,
                    "registry_value_unreadable",
                );
                continue;
            }
        };
        let was_name_truncated = name.chars().count() > MAX_STARTUP_NAME_CHARS;
        let status = registry_entry_status(&raw_value);
        if matches!(
            status,
            StartupEntryStatus::MalformedRegistryValue
                | StartupEntryStatus::UnsupportedRegistryType
                | StartupEntryStatus::RegistryValueTooLarge
        ) {
            push_warning(&mut report.warnings, source_label, "invalid_registry_value");
        }
        if was_name_truncated {
            push_warning(&mut report.warnings, source_label, "entry_name_truncated");
        }
        if !push_startup_entry(
            report,
            StartupInventoryEntry {
                name: name
                    .chars()
                    .map(|character| {
                        if character.is_control() {
                            '\u{fffd}'
                        } else {
                            character
                        }
                    })
                    .take(MAX_STARTUP_NAME_CHARS)
                    .collect(),
                source,
                status,
            },
        ) {
            break;
        }
    }
}

#[cfg(windows)]
pub(crate) fn known_folder_path(
    folder_id: windows::core::GUID,
) -> WindowsResult<std::path::PathBuf> {
    use windows::Win32::{
        Foundation::HANDLE,
        System::Com::CoTaskMemFree,
        UI::Shell::{SHGetKnownFolderPath, KF_FLAG_DEFAULT},
    };

    let path = unsafe {
        SHGetKnownFolderPath(&folder_id, KF_FLAG_DEFAULT, HANDLE::default())
            .map_err(|error| api_error("resolve known startup folder", error))?
    };
    let converted = unsafe { path.to_string() };
    unsafe { CoTaskMemFree(Some(path.0.cast())) };
    converted.map(std::path::PathBuf::from).map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "decode known startup folder",
            None,
        )
    })
}

#[cfg(windows)]
fn enumerate_startup_folder(
    report: &mut StartupInventoryObservation,
    folder_id: windows::core::GUID,
    source: StartupEntrySource,
    source_label: &'static str,
) {
    use std::os::windows::fs::MetadataExt;
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let folder = match known_folder_path(folder_id) {
        Ok(folder) => folder,
        Err(_) => {
            push_warning(
                &mut report.warnings,
                source_label,
                "known_folder_unavailable",
            );
            return;
        }
    };
    if !is_local_disk_path(&folder) {
        push_warning(
            &mut report.warnings,
            source_label,
            "non_local_startup_folder_not_scanned",
        );
        return;
    }
    match path_has_reparse_component(&folder) {
        Ok(true) => {
            push_warning(
                &mut report.warnings,
                source_label,
                "folder_reparse_not_followed",
            );
            return;
        }
        Ok(false) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => {
            push_warning(
                &mut report.warnings,
                source_label,
                "startup_folder_unreadable",
            );
            return;
        }
    }
    match std::fs::symlink_metadata(&folder) {
        Ok(metadata) if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 => {
            push_warning(
                &mut report.warnings,
                source_label,
                "folder_reparse_not_followed",
            );
            return;
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => {
            push_warning(
                &mut report.warnings,
                source_label,
                "startup_folder_unreadable",
            );
            return;
        }
    }

    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(_) => {
            push_warning(
                &mut report.warnings,
                source_label,
                "startup_folder_unreadable",
            );
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                push_warning(
                    &mut report.warnings,
                    source_label,
                    "folder_entry_unreadable",
                );
                continue;
            }
        };
        let status = match std::fs::symlink_metadata(entry.path()) {
            Ok(metadata) if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 => {
                StartupEntryStatus::ReparsePointNotFollowed
            }
            Ok(_) => StartupEntryStatus::StartupFile,
            Err(_) => {
                push_warning(
                    &mut report.warnings,
                    source_label,
                    "folder_entry_unreadable",
                );
                StartupEntryStatus::StartupFile
            }
        };
        if !push_startup_entry(
            report,
            StartupInventoryEntry {
                name: bounded_name(&entry.file_name()),
                source,
                status,
            },
        ) {
            break;
        }
    }
}

#[cfg(windows)]
pub fn read_startup_inventory() -> WindowsResult<StartupInventoryObservation> {
    use windows::Win32::UI::Shell::{FOLDERID_CommonStartup, FOLDERID_Startup};
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, KEY_WOW64_64KEY};

    let mut report = StartupInventoryObservation {
        entries: Vec::new(),
        warnings: Vec::new(),
        truncated: false,
    };
    enumerate_run_key(
        &mut report,
        HKEY_CURRENT_USER,
        KEY_WOW64_64KEY,
        StartupEntrySource::CurrentUserRun,
        "hkcu_run",
    );
    enumerate_run_key(
        &mut report,
        HKEY_LOCAL_MACHINE,
        KEY_WOW64_64KEY,
        StartupEntrySource::LocalMachineRun64,
        "hklm_run_64",
    );
    enumerate_run_key(
        &mut report,
        HKEY_LOCAL_MACHINE,
        KEY_WOW64_32KEY,
        StartupEntrySource::LocalMachineRun32,
        "hklm_run_32",
    );
    enumerate_startup_folder(
        &mut report,
        FOLDERID_Startup,
        StartupEntrySource::UserStartupFolder,
        "user_startup_folder",
    );
    enumerate_startup_folder(
        &mut report,
        FOLDERID_CommonStartup,
        StartupEntrySource::CommonStartupFolder,
        "common_startup_folder",
    );
    Ok(report)
}

#[cfg(not(windows))]
pub fn read_startup_inventory() -> WindowsResult<StartupInventoryObservation> {
    Err(WindowsError::unsupported("read startup inventory"))
}

#[cfg(windows)]
fn windows_directory() -> WindowsResult<std::path::PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;

    let mut buffer = vec![0u16; 32_768];
    let length = unsafe { GetWindowsDirectoryW(Some(&mut buffer)) } as usize;
    if length == 0 {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "resolve Windows directory",
            None,
        ));
    }
    if length >= buffer.len() {
        return Err(WindowsError::new(
            WindowsErrorKind::ResourceLimit,
            "read bounded Windows directory",
            None,
        ));
    }
    Ok(std::path::PathBuf::from(OsString::from_wide(
        &buffer[..length],
    )))
}

#[cfg(windows)]
pub fn read_system_drive_space() -> WindowsResult<SystemDriveSpaceObservation> {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, Prefix};
    use windows::{core::PCWSTR, Win32::Storage::FileSystem::GetDiskFreeSpaceExW};

    let windows_directory = windows_directory()?;
    let drive = match windows_directory.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => letter,
            _ => {
                return Err(WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "validate Windows system drive",
                    None,
                ))
            }
        },
        _ => {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "validate Windows system drive",
                None,
            ))
        }
    };
    let root = format!("{}:{}", char::from(drive), std::path::MAIN_SEPARATOR);
    let mut wide = std::ffi::OsStr::new(&root)
        .encode_wide()
        .collect::<Vec<_>>();
    wide.push(0);
    let mut available_bytes = 0u64;
    let mut total_bytes = 0u64;
    let mut total_free_bytes = 0u64;
    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR::from_raw(wide.as_ptr()),
            Some(&mut available_bytes),
            Some(&mut total_bytes),
            Some(&mut total_free_bytes),
        )
        .map_err(|error| api_error("read system drive free space", error))?;
    }
    Ok(SystemDriveSpaceObservation {
        volume: format!("{}:", char::from(drive)),
        available_bytes,
        total_bytes,
        total_free_bytes,
    })
}

#[cfg(not(windows))]
pub fn read_system_drive_space() -> WindowsResult<SystemDriveSpaceObservation> {
    Err(WindowsError::unsupported("read system drive free space"))
}

#[cfg(windows)]
fn user_temp_path() -> WindowsResult<std::path::PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows::Win32::Storage::FileSystem::GetTempPath2W;
    use windows::Win32::UI::Shell::FOLDERID_LocalAppData;

    let mut buffer = vec![0u16; 32_768];
    let length = unsafe { GetTempPath2W(Some(&mut buffer)) } as usize;
    if length == 0 {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "resolve user temp directory",
            None,
        ));
    }
    if length >= buffer.len() {
        return Err(WindowsError::new(
            WindowsErrorKind::ResourceLimit,
            "read bounded user temp directory",
            None,
        ));
    }
    let path = std::path::PathBuf::from(OsString::from_wide(&buffer[..length]));
    let local_app_data = known_folder_path(FOLDERID_LocalAppData)?;
    if !path.is_absolute()
        || !is_local_disk_path(&path)
        || !is_expected_user_temp_path(&path, &local_app_data)
    {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate user temp directory",
            None,
        ));
    }
    Ok(path)
}

#[cfg(windows)]
fn is_expected_user_temp_path(path: &std::path::Path, local_app_data: &std::path::Path) -> bool {
    fn normalized(path: &std::path::Path) -> String {
        path.as_os_str()
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_owned()
    }

    normalized(path).eq_ignore_ascii_case(&normalized(&local_app_data.join("Temp")))
}

#[cfg(windows)]
pub fn read_user_temp_inventory() -> WindowsResult<TempFilesObservation> {
    use std::os::windows::fs::MetadataExt;
    use std::time::{Duration, Instant};
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    const MAX_SCAN_DURATION: Duration = Duration::from_millis(MAX_TEMP_SCAN_DURATION_MS);

    let root = user_temp_path()?;
    let mut report = TempFilesObservation {
        file_count: 0,
        directory_count: 0,
        total_bytes: 0,
        skipped_reparse_points: 0,
        unreadable_entries: 0,
        warnings: Vec::new(),
        truncated: false,
    };
    match path_has_reparse_component(&root) {
        Ok(true) => {
            report.skipped_reparse_points = 1;
            report.truncated = true;
            push_warning(
                &mut report.warnings,
                "user_temp",
                "root_reparse_not_followed",
            );
            return Ok(report);
        }
        Ok(false) => {}
        Err(_) => {
            report.unreadable_entries = 1;
            report.truncated = true;
            push_warning(&mut report.warnings, "user_temp", "temp_root_unreadable");
            return Ok(report);
        }
    }
    match std::fs::symlink_metadata(&root) {
        Ok(metadata) if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 => {
            report.skipped_reparse_points = 1;
            report.truncated = true;
            push_warning(
                &mut report.warnings,
                "user_temp",
                "root_reparse_not_followed",
            );
            return Ok(report);
        }
        Ok(_) => {}
        Err(_) => {
            report.unreadable_entries = 1;
            report.truncated = true;
            push_warning(&mut report.warnings, "user_temp", "temp_root_unreadable");
            return Ok(report);
        }
    }

    let started = Instant::now();
    let mut processed_entries = 0u64;
    let mut pending = vec![(root, 0u8)];
    while let Some((directory, depth)) = pending.pop() {
        if started.elapsed() >= MAX_SCAN_DURATION {
            report.truncated = true;
            push_warning(&mut report.warnings, "user_temp", "time_budget_reached");
            break;
        }
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(_) => {
                report.unreadable_entries = report.unreadable_entries.saturating_add(1);
                report.truncated = true;
                push_warning(&mut report.warnings, "user_temp", "directory_unreadable");
                continue;
            }
        };
        for entry in entries {
            if processed_entries >= MAX_TEMP_ENTRIES || started.elapsed() >= MAX_SCAN_DURATION {
                report.truncated = true;
                push_warning(&mut report.warnings, "user_temp", "scan_budget_reached");
                pending.clear();
                break;
            }
            processed_entries += 1;
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    report.unreadable_entries = report.unreadable_entries.saturating_add(1);
                    report.truncated = true;
                    push_warning(&mut report.warnings, "user_temp", "entry_unreadable");
                    continue;
                }
            };
            let metadata = match std::fs::symlink_metadata(entry.path()) {
                Ok(metadata) => metadata,
                Err(_) => {
                    report.unreadable_entries = report.unreadable_entries.saturating_add(1);
                    report.truncated = true;
                    push_warning(&mut report.warnings, "user_temp", "metadata_unreadable");
                    continue;
                }
            };
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
                report.skipped_reparse_points = report.skipped_reparse_points.saturating_add(1);
                continue;
            }
            if metadata.is_dir() {
                report.directory_count = report.directory_count.saturating_add(1);
                if depth >= MAX_TEMP_DEPTH || report.directory_count >= MAX_TEMP_DIRECTORIES {
                    report.truncated = true;
                    push_warning(&mut report.warnings, "user_temp", "directory_limit_reached");
                } else {
                    pending.push((entry.path(), depth + 1));
                }
                continue;
            }

            let size = metadata.len();
            let Some(next_total) = report.total_bytes.checked_add(size) else {
                report.truncated = true;
                push_warning(&mut report.warnings, "user_temp", "byte_limit_reached");
                pending.clear();
                break;
            };
            if next_total > MAX_TEMP_TOTAL_BYTES {
                report.truncated = true;
                push_warning(&mut report.warnings, "user_temp", "byte_limit_reached");
                pending.clear();
                break;
            }
            report.total_bytes = next_total;
            report.file_count = report.file_count.saturating_add(1);
        }
    }
    Ok(report)
}

#[cfg(not(windows))]
pub fn read_user_temp_inventory() -> WindowsResult<TempFilesObservation> {
    Err(WindowsError::unsupported("read user temp inventory"))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn registry_string_validation_is_bounded_and_strict() {
        let valid = "notepad.exe\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert!(registry_string_is_well_formed(&valid));
        assert!(!registry_string_is_well_formed(&valid[..valid.len() - 2]));
        assert!(!registry_string_is_well_formed(&[0, 0]));
    }

    #[test]
    fn only_local_disk_paths_are_scannable() {
        assert!(is_local_disk_path(std::path::Path::new(r"C:\Windows\Temp")));
        assert!(!is_local_disk_path(std::path::Path::new(
            r"\\server\share\Temp"
        )));
        assert!(!is_local_disk_path(std::path::Path::new(r"relative\Temp")));
        assert!(!is_local_disk_path(std::path::Path::new(
            r"C:relative\Temp"
        )));
    }

    #[test]
    fn destructive_temp_cleanup_accepts_only_local_app_data_temp() {
        let local_app_data = std::path::Path::new(r"C:\Users\example\AppData\Local");
        assert!(is_expected_user_temp_path(
            std::path::Path::new(r"C:\Users\example\AppData\Local\Temp\"),
            local_app_data
        ));
        assert!(!is_expected_user_temp_path(
            std::path::Path::new(r"C:\Users\example\Documents"),
            local_app_data
        ));
    }

    #[test]
    fn handle_based_temp_deletion_rejects_files_outside_the_trusted_root() {
        let root_directory = tempfile::tempdir().expect("trusted temp root");
        let outside_directory = tempfile::tempdir().expect("outside directory");
        let inside = root_directory.path().join("inside.tmp");
        let outside = outside_directory.path().join("outside.txt");
        std::fs::write(&inside, b"inside").expect("write inside file");
        std::fs::write(&outside, b"outside").expect("write outside file");
        let root = TrustedTempRoot::open(root_directory.path().to_path_buf())
            .expect("open trusted root handle");

        let deleted = delete_validated_temp_file(&root, &inside, 0)
            .expect("delete an opened in-root file by handle");
        assert_eq!(deleted, 6);
        assert!(!inside.exists());

        let error = delete_validated_temp_file(&root, &outside, 0)
            .expect_err("outside file must not be deleted");
        assert_eq!(error.kind, WindowsErrorKind::ExternalConflict);
        assert_eq!(
            std::fs::read(&outside).expect("outside file remains"),
            b"outside"
        );
    }
}

// ---------------------------------------------------------------------------
// 一時ファイルの削除（BRIEF §3「一時ファイルの確認と削除」）
//
// 安全契約:
// - 対象は現在ユーザーの一時フォルダー配下の**ファイルのみ**。ディレクトリは消さない。
// - reparse point（シンボリックリンク等）は辿らず、対象にもしない。
// - **一定日数より古いものだけ**。preview→実行の間に増えたファイルは条件上入らない。
// - 実行時も同じ検証を通してから1件ずつ削除する。
// - 使用中などで消せないものは理由つきで報告し、処理は止めない。**元に戻せない操作**。
// - UIへ渡すのはファイル名と大きさだけで、フルパスは出さない。
// ---------------------------------------------------------------------------

/// 既定の下限日数。これより新しい一時ファイルは削除候補にしない。
pub const TEMP_CLEANUP_MIN_AGE_DAYS: u64 = 7;
const MAX_CLEANUP_CANDIDATES: usize = 500;
const SECONDS_PER_DAY: u64 = 60 * 60 * 24;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TempCleanupCandidate {
    pub file_name: String,
    pub size_bytes: u64,
    pub age_days: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TempCleanupPlan {
    pub min_age_days: u64,
    pub candidates: Vec<TempCleanupCandidate>,
    pub total_bytes: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TempCleanupSkip {
    pub file_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TempCleanupOutcome {
    pub deleted_count: u64,
    pub freed_bytes: u64,
    pub skipped: Vec<TempCleanupSkip>,
    pub truncated: bool,
}

/// 秒数を日数へ。端数は切り捨て（若く見積もる＝安全側）。
pub fn seconds_to_days(seconds: u64) -> u64 {
    seconds / SECONDS_PER_DAY
}

#[cfg(windows)]
fn temp_cleanup_walk<F>(
    root: &std::path::Path,
    min_age_days: u64,
    mut visit: F,
) -> WindowsResult<bool>
where
    F: FnMut(&std::path::Path, &str, u64, u64),
{
    use std::os::windows::fs::MetadataExt;
    use std::time::{Duration, Instant, SystemTime};
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    const MAX_SCAN_DURATION: Duration = Duration::from_millis(500);

    if path_has_reparse_component(root).unwrap_or(true) {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "temp root has a reparse component",
            None,
        ));
    }
    let started = Instant::now();
    let now = SystemTime::now();
    let mut truncated = false;
    let mut visited = 0usize;
    let mut directories = 0u64;
    let mut pending = vec![(root.to_path_buf(), 0u8)];

    while let Some((directory, depth)) = pending.pop() {
        if started.elapsed() >= MAX_SCAN_DURATION || visited >= MAX_CLEANUP_CANDIDATES {
            truncated = true;
            break;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            truncated = true;
            continue;
        };
        for entry in entries.flatten() {
            if started.elapsed() >= MAX_SCAN_DURATION || visited >= MAX_CLEANUP_CANDIDATES {
                truncated = true;
                pending.clear();
                break;
            }
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                truncated = true;
                continue;
            };
            // reparse point は辿らず、削除対象にもしない。
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
                continue;
            }
            if metadata.is_dir() {
                if depth < MAX_TEMP_DEPTH && directories < MAX_TEMP_DIRECTORIES {
                    directories = directories.saturating_add(1);
                    pending.push((path, depth.saturating_add(1)));
                } else {
                    truncated = true;
                }
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            // 更新時刻が読めない、または未来日付のものは触らない。
            let age_days = metadata
                .modified()
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .map(|elapsed| seconds_to_days(elapsed.as_secs()));
            let Some(age_days) = age_days else {
                continue;
            };
            if age_days < min_age_days {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            visited = visited.saturating_add(1);
            visit(&path, file_name, metadata.len(), age_days);
        }
    }
    Ok(truncated)
}

/// 削除候補の一覧（適用前に必ず提示する）。read-only。
#[cfg(windows)]
pub fn plan_user_temp_cleanup(min_age_days: u64) -> WindowsResult<TempCleanupPlan> {
    let mut candidates = Vec::new();
    let mut total_bytes = 0u64;
    let root = user_temp_path()?;
    let truncated = temp_cleanup_walk(&root, min_age_days, |_path, file_name, size, age_days| {
        total_bytes = total_bytes.saturating_add(size);
        candidates.push(TempCleanupCandidate {
            file_name: file_name.to_owned(),
            size_bytes: size,
            age_days,
        });
    })?;
    Ok(TempCleanupPlan {
        min_age_days,
        candidates,
        total_bytes,
        truncated,
    })
}

#[cfg(windows)]
struct TrustedTempRoot {
    path: std::path::PathBuf,
    final_path: String,
    _handle: std::fs::File,
}

#[cfg(windows)]
impl TrustedTempRoot {
    fn open(path: std::path::PathBuf) -> WindowsResult<Self> {
        use std::os::windows::fs::MetadataExt;
        use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

        if path_has_reparse_component(&path).unwrap_or(true) {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "open trusted temp root without reparse components",
                None,
            ));
        }
        let handle = open_temp_handle(&path, false, true)?;
        let metadata = handle
            .metadata()
            .map_err(|error| WindowsError::io("read trusted temp root", &error))?;
        if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "validate trusted temp root handle",
                None,
            ));
        }
        let final_path = final_path_for_handle(&handle)?;
        Ok(Self {
            path,
            final_path,
            _handle: handle,
        })
    }
}

#[cfg(windows)]
fn open_temp_handle(
    path: &std::path::Path,
    request_delete: bool,
    directory: bool,
) -> WindowsResult<std::fs::File> {
    use std::os::windows::{ffi::OsStrExt, io::FromRawHandle};
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{
                CreateFileW, DELETE, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS,
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
                FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
            },
        },
    };

    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    wide.push(0);
    let access = FILE_READ_ATTRIBUTES.0 | if request_delete { DELETE.0 } else { 0 };
    let flags = if directory {
        FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT
    } else {
        FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT
    };
    let handle = unsafe {
        CreateFileW(
            PCWSTR::from_raw(wide.as_ptr()),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            flags,
            HANDLE::default(),
        )
    }
    .map_err(|error| api_error("open temp cleanup object", error))?;
    Ok(unsafe { std::fs::File::from_raw_handle(handle.0) })
}

#[cfg(windows)]
fn final_path_for_handle(file: &std::fs::File) -> WindowsResult<String> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{
        Foundation::{GetLastError, HANDLE},
        Storage::FileSystem::{GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED},
    };

    let mut buffer = vec![0u16; 512];
    loop {
        let length = unsafe {
            GetFinalPathNameByHandleW(
                HANDLE(file.as_raw_handle()),
                &mut buffer,
                FILE_NAME_NORMALIZED,
            )
        } as usize;
        if length == 0 {
            return Err(WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "resolve final temp cleanup path",
                Some(i64::from(unsafe { GetLastError() }.0)),
            ));
        }
        if length < buffer.len() {
            return String::from_utf16(&buffer[..length]).map_err(|_| {
                WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "decode final temp cleanup path",
                    None,
                )
            });
        }
        if length > 32_768 {
            return Err(WindowsError::new(
                WindowsErrorKind::ResourceLimit,
                "read bounded final temp cleanup path",
                None,
            ));
        }
        buffer.resize(length, 0);
    }
}

#[cfg(windows)]
fn final_path_is_below(root: &str, candidate: &str) -> bool {
    let root = root.trim_end_matches(['\\', '/']);
    let Some(prefix) = candidate.get(..root.len()) else {
        return false;
    };
    if !prefix.eq_ignore_ascii_case(root) {
        return false;
    }
    candidate
        .get(root.len()..)
        .and_then(|suffix| suffix.chars().next())
        .is_some_and(|separator| matches!(separator, '\\' | '/'))
}

#[cfg(windows)]
fn delete_validated_temp_file(
    root: &TrustedTempRoot,
    path: &std::path::Path,
    min_age_days: u64,
) -> WindowsResult<u64> {
    use std::{os::windows::fs::MetadataExt, os::windows::io::AsRawHandle, time::SystemTime};
    use windows::Win32::{
        Foundation::{BOOLEAN, HANDLE},
        Storage::FileSystem::{
            FileDispositionInfo, SetFileInformationByHandle, FILE_ATTRIBUTE_REPARSE_POINT,
            FILE_DISPOSITION_INFO,
        },
    };

    // Open the leaf without following a leaf reparse point, then make every
    // decision and the final disposition against this same handle. If an
    // ancestor was swapped for a junction before open, the final path check
    // below rejects the resulting object; swaps after open cannot retarget it.
    let file = open_temp_handle(path, true, false)?;
    let metadata = file
        .metadata()
        .map_err(|error| WindowsError::io("revalidate temp cleanup file", &error))?;
    let age_days = metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map(|elapsed| seconds_to_days(elapsed.as_secs()));
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
        || !age_days.is_some_and(|age| age >= min_age_days)
    {
        return Err(WindowsError::new(
            WindowsErrorKind::ExternalConflict,
            "revalidate temp cleanup candidate",
            None,
        ));
    }
    let final_path = final_path_for_handle(&file)?;
    if !final_path_is_below(&root.final_path, &final_path) {
        return Err(WindowsError::new(
            WindowsErrorKind::ExternalConflict,
            "reject temp cleanup candidate outside trusted root",
            None,
        ));
    }

    let disposition = FILE_DISPOSITION_INFO {
        DeleteFile: BOOLEAN(1),
    };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(|error| api_error("delete validated temp cleanup file by handle", error))?;
    Ok(metadata.len())
}

/// 同じ条件で再走査し、1件ずつ削除する。**元に戻せない**。
#[cfg(windows)]
pub fn delete_user_temp_files(min_age_days: u64) -> WindowsResult<TempCleanupOutcome> {
    let mut deleted_count = 0u64;
    let mut freed_bytes = 0u64;
    let mut skipped: Vec<TempCleanupSkip> = Vec::new();
    let root = TrustedTempRoot::open(user_temp_path()?)?;
    let truncated = temp_cleanup_walk(&root.path, min_age_days, |path, file_name, _size, _age| {
        match delete_validated_temp_file(&root, path, min_age_days) {
            Ok(size) => {
                deleted_count = deleted_count.saturating_add(1);
                freed_bytes = freed_bytes.saturating_add(size);
            }
            Err(error) => {
                if skipped.len() < MAX_WARNINGS {
                    let reason = match error.kind {
                        WindowsErrorKind::AccessDenied => "使用中か、権限がありません",
                        WindowsErrorKind::ExternalConflict => "対象が変わったため触りませんでした",
                        _ => "削除できませんでした",
                    };
                    skipped.push(TempCleanupSkip {
                        file_name: file_name.to_owned(),
                        reason: reason.to_owned(),
                    });
                }
            }
        }
    })?;
    Ok(TempCleanupOutcome {
        deleted_count,
        freed_bytes,
        skipped,
        truncated,
    })
}

#[cfg(not(windows))]
pub fn plan_user_temp_cleanup(_min_age_days: u64) -> WindowsResult<TempCleanupPlan> {
    Err(WindowsError::unsupported("plan temp cleanup"))
}

#[cfg(not(windows))]
pub fn delete_user_temp_files(_min_age_days: u64) -> WindowsResult<TempCleanupOutcome> {
    Err(WindowsError::unsupported("delete temp files"))
}

#[cfg(test)]
mod temp_cleanup_tests {
    use super::*;

    #[test]
    fn age_conversion_floors_to_whole_days() {
        assert_eq!(seconds_to_days(0), 0);
        assert_eq!(seconds_to_days(SECONDS_PER_DAY - 1), 0, "1日未満は0日扱い");
        assert_eq!(seconds_to_days(SECONDS_PER_DAY), 1);
        assert_eq!(seconds_to_days(SECONDS_PER_DAY * 7 + 3600), 7);
    }

    #[test]
    fn default_threshold_keeps_recent_files() {
        // 既定は7日。作成直後(0日)のファイルは条件上どうやっても候補にならない。
        assert_eq!(TEMP_CLEANUP_MIN_AGE_DAYS, 7);
        assert!(seconds_to_days(3600) < TEMP_CLEANUP_MIN_AGE_DAYS);
        assert!(seconds_to_days(SECONDS_PER_DAY * 6) < TEMP_CLEANUP_MIN_AGE_DAYS);
        assert!(seconds_to_days(SECONDS_PER_DAY * 8) >= TEMP_CLEANUP_MIN_AGE_DAYS);
    }

    #[cfg(windows)]
    #[test]
    fn planning_is_read_only_and_never_reports_recent_files() {
        let plan = plan_user_temp_cleanup(TEMP_CLEANUP_MIN_AGE_DAYS).expect("plan temp cleanup");
        assert_eq!(plan.min_age_days, TEMP_CLEANUP_MIN_AGE_DAYS);
        assert!(plan.candidates.len() <= MAX_CLEANUP_CANDIDATES);
        for candidate in &plan.candidates {
            assert!(
                candidate.age_days >= TEMP_CLEANUP_MIN_AGE_DAYS,
                "下限日数より新しいファイルを候補にしない"
            );
            assert!(!candidate.file_name.contains('\\'), "フルパスを渡さない");
            assert!(!candidate.file_name.contains('/'), "フルパスを渡さない");
        }
        // 直前に作った一時ファイルは候補に入らず、計画作成では消えない。
        let probe = std::env::temp_dir().join("totonoe-cleanup-probe.tmp");
        std::fs::write(&probe, b"probe").expect("write probe");
        let after = plan_user_temp_cleanup(TEMP_CLEANUP_MIN_AGE_DAYS).expect("re-plan");
        assert!(
            !after
                .candidates
                .iter()
                .any(|candidate| candidate.file_name == "totonoe-cleanup-probe.tmp"),
            "作成直後のファイルは削除候補にならない"
        );
        assert!(probe.exists(), "計画作成では削除しない");
        std::fs::remove_file(&probe).expect("clean up probe");
    }
}
