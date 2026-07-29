use std::collections::{BTreeMap, BTreeSet};

use rusqlite::params;
#[cfg(test)]
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::{
    error::{CoreError, CoreResult},
    journal::JournalDatabase,
};

const MAX_SELECTED_CATEGORIES: usize = 5;
const MAX_HISTORY_SNAPSHOTS: i64 = 720;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageCategory {
    Documents,
    Downloads,
    Desktop,
    Pictures,
    Videos,
}

impl StorageCategory {
    const ALL: [Self; 5] = [
        Self::Documents,
        Self::Downloads,
        Self::Desktop,
        Self::Pictures,
        Self::Videos,
    ];

    fn as_db(self) -> &'static str {
        match self {
            Self::Documents => "documents",
            Self::Downloads => "downloads",
            Self::Desktop => "desktop",
            Self::Pictures => "pictures",
            Self::Videos => "videos",
        }
    }

    fn from_db(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.as_db() == value)
    }

    #[cfg(windows)]
    fn resolve(self) -> crate::windows::WindowsResult<std::path::PathBuf> {
        use windows::Win32::UI::Shell::{
            FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Pictures,
            FOLDERID_Videos,
        };

        let folder_id = match self {
            Self::Documents => FOLDERID_Documents,
            Self::Downloads => FOLDERID_Downloads,
            Self::Desktop => FOLDERID_Desktop,
            Self::Pictures => FOLDERID_Pictures,
            Self::Videos => FOLDERID_Videos,
        };
        crate::windows::known_folder_path(folder_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageCategoryPoint {
    pub category: StorageCategory,
    pub total_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
    pub total_bytes_delta: Option<i64>,
    pub file_count_delta: Option<i64>,
    pub skipped_reparse_points: u64,
    pub access_denied_count: u64,
    pub unreadable_entries: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageHistoryPoint {
    pub captured_at_unix_ms: u64,
    pub drive_total_bytes: u64,
    pub drive_total_free_bytes: u64,
    pub drive_available_bytes: u64,
    pub drive_free_delta_bytes: Option<i64>,
    pub categories: Vec<StorageCategoryPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FolderAggregate {
    category: StorageCategory,
    total_bytes: u64,
    file_count: u64,
    directory_count: u64,
    skipped_reparse_points: u64,
    access_denied_count: u64,
    unreadable_entries: u64,
    truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapturedSnapshot {
    captured_at_unix_ms: u64,
    drive_total_bytes: u64,
    drive_total_free_bytes: u64,
    drive_available_bytes: u64,
    categories: Vec<FolderAggregate>,
}

pub fn capture(
    database: &JournalDatabase,
    categories: Vec<StorageCategory>,
    captured_at_unix_ms: u64,
) -> CoreResult<StorageHistoryPoint> {
    let categories = validate_categories(categories)?;
    let captured = capture_snapshot(&categories, captured_at_unix_ms)?;
    database.insert_storage_history(&captured)?;
    database
        .list_storage_history(1)?
        .into_iter()
        .next()
        .ok_or_else(CoreError::storage)
}

pub fn list(database: &JournalDatabase) -> CoreResult<Vec<StorageHistoryPoint>> {
    database.list_storage_history(180)
}

pub fn clear(database: &JournalDatabase) -> CoreResult<u64> {
    database.clear_storage_history()
}

fn validate_categories(categories: Vec<StorageCategory>) -> CoreResult<Vec<StorageCategory>> {
    if categories.is_empty() || categories.len() > MAX_SELECTED_CATEGORIES {
        return Err(CoreError::invalid_request(
            "記録する既知フォルダーを1〜5個選んでください。",
        ));
    }
    let unique = categories.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() != categories.len() {
        return Err(CoreError::invalid_request(
            "同じ既知フォルダーを重複して選べません。",
        ));
    }
    Ok(categories)
}

#[cfg(windows)]
fn capture_snapshot(
    categories: &[StorageCategory],
    captured_at_unix_ms: u64,
) -> CoreResult<CapturedSnapshot> {
    let drive = crate::windows::read_system_drive_space().map_err(|_| {
        CoreError::invalid_request("システムドライブの空き容量を確認できませんでした。")
    })?;
    let mut aggregates = Vec::with_capacity(categories.len());
    for category in categories {
        let aggregate = match category.resolve() {
            Ok(root) => scan_folder(*category, &root),
            Err(_) => FolderAggregate {
                category: *category,
                total_bytes: 0,
                file_count: 0,
                directory_count: 0,
                skipped_reparse_points: 0,
                access_denied_count: 0,
                unreadable_entries: 1,
                truncated: true,
            },
        };
        aggregates.push(aggregate);
    }
    Ok(CapturedSnapshot {
        captured_at_unix_ms,
        drive_total_bytes: drive.total_bytes,
        drive_total_free_bytes: drive.total_free_bytes,
        drive_available_bytes: drive.available_bytes,
        categories: aggregates,
    })
}

#[cfg(not(windows))]
fn capture_snapshot(
    _categories: &[StorageCategory],
    _captured_at_unix_ms: u64,
) -> CoreResult<CapturedSnapshot> {
    Err(CoreError::invalid_request(
        "この機能はWindowsでのみ利用できます。",
    ))
}

#[cfg(windows)]
fn scan_folder(category: StorageCategory, root: &std::path::Path) -> FolderAggregate {
    use std::{
        os::windows::fs::MetadataExt,
        time::{Duration, Instant},
    };
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    use crate::windows::{
        is_local_disk_path, path_has_reparse_component, MAX_TEMP_DEPTH, MAX_TEMP_DIRECTORIES,
        MAX_TEMP_ENTRIES, MAX_TEMP_SCAN_DURATION_MS, MAX_TEMP_TOTAL_BYTES,
    };

    let mut report = FolderAggregate {
        category,
        total_bytes: 0,
        file_count: 0,
        directory_count: 0,
        skipped_reparse_points: 0,
        access_denied_count: 0,
        unreadable_entries: 0,
        truncated: false,
    };
    if !is_local_disk_path(root) {
        report.unreadable_entries = 1;
        report.truncated = true;
        return report;
    }
    match path_has_reparse_component(root) {
        Ok(true) => {
            report.skipped_reparse_points = 1;
            report.truncated = true;
            return report;
        }
        Ok(false) => {}
        Err(error) => {
            count_io_error(&mut report, &error);
            report.truncated = true;
            return report;
        }
    }
    match std::fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 => {
            report.skipped_reparse_points = 1;
            report.truncated = true;
            return report;
        }
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            report.unreadable_entries = 1;
            report.truncated = true;
            return report;
        }
        Err(error) => {
            count_io_error(&mut report, &error);
            report.truncated = true;
            return report;
        }
    }

    let started = Instant::now();
    let max_duration = Duration::from_millis(MAX_TEMP_SCAN_DURATION_MS);
    let mut processed_entries = 0u64;
    let mut pending = vec![(root.to_path_buf(), 0u8)];
    'scan: while let Some((directory, depth)) = pending.pop() {
        if started.elapsed() >= max_duration {
            report.truncated = true;
            break;
        }
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) => {
                count_io_error(&mut report, &error);
                continue;
            }
        };
        for entry in entries {
            if processed_entries >= MAX_TEMP_ENTRIES || started.elapsed() >= max_duration {
                report.truncated = true;
                break 'scan;
            }
            processed_entries = processed_entries.saturating_add(1);
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    count_io_error(&mut report, &error);
                    continue;
                }
            };
            let metadata = match std::fs::symlink_metadata(entry.path()) {
                Ok(metadata) => metadata,
                Err(error) => {
                    count_io_error(&mut report, &error);
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
                    break 'scan;
                } else {
                    pending.push((entry.path(), depth.saturating_add(1)));
                }
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            let Some(next_total) = report.total_bytes.checked_add(metadata.len()) else {
                report.truncated = true;
                break 'scan;
            };
            if next_total > MAX_TEMP_TOTAL_BYTES {
                report.truncated = true;
                break 'scan;
            }
            report.total_bytes = next_total;
            report.file_count = report.file_count.saturating_add(1);
        }
    }
    report
}

#[cfg(windows)]
fn count_io_error(report: &mut FolderAggregate, error: &std::io::Error) {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        report.access_denied_count = report.access_denied_count.saturating_add(1);
    } else {
        report.unreadable_entries = report.unreadable_entries.saturating_add(1);
    }
}

impl JournalDatabase {
    fn insert_storage_history(&self, snapshot: &CapturedSnapshot) -> CoreResult<()> {
        self.with_immediate_transaction(|database| {
            database.execute(
                "INSERT INTO storage_history_snapshots(
                    captured_at_unix_ms, drive_total_bytes, drive_total_free_bytes,
                    drive_available_bytes
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    to_i64(snapshot.captured_at_unix_ms),
                    to_i64(snapshot.drive_total_bytes),
                    to_i64(snapshot.drive_total_free_bytes),
                    to_i64(snapshot.drive_available_bytes),
                ],
            )?;
            let snapshot_id = database.last_insert_rowid();
            for category in &snapshot.categories {
                database.execute(
                    "INSERT INTO storage_history_categories(
                        snapshot_id, category, total_bytes, file_count, directory_count,
                        skipped_reparse_points, access_denied_count, unreadable_entries,
                        truncated
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        snapshot_id,
                        category.category.as_db(),
                        to_i64(category.total_bytes),
                        to_i64(category.file_count),
                        to_i64(category.directory_count),
                        to_i64(category.skipped_reparse_points),
                        to_i64(category.access_denied_count),
                        to_i64(category.unreadable_entries),
                        if category.truncated { 1 } else { 0 },
                    ],
                )?;
            }
            database.execute(
                "DELETE FROM storage_history_snapshots
                 WHERE snapshot_id IN (
                   SELECT snapshot_id FROM storage_history_snapshots
                   ORDER BY captured_at_unix_ms DESC, snapshot_id DESC
                   LIMIT -1 OFFSET ?1
                 )",
                [MAX_HISTORY_SNAPSHOTS],
            )?;
            Ok(())
        })
    }

    fn list_storage_history(&self, limit: u32) -> CoreResult<Vec<StorageHistoryPoint>> {
        self.with_connection(|database| {
            let mut snapshot_statement = database.prepare(
                "SELECT snapshot_id, captured_at_unix_ms, drive_total_bytes,
                        drive_total_free_bytes, drive_available_bytes
                 FROM (
                   SELECT snapshot_id, captured_at_unix_ms, drive_total_bytes,
                          drive_total_free_bytes, drive_available_bytes
                   FROM storage_history_snapshots
                   ORDER BY captured_at_unix_ms DESC, snapshot_id DESC
                   LIMIT ?1
                 )
                 ORDER BY captured_at_unix_ms ASC, snapshot_id ASC",
            )?;
            let snapshots = snapshot_statement
                .query_map([i64::from(limit.min(720))], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        from_i64(row.get::<_, i64>(1)?),
                        from_i64(row.get::<_, i64>(2)?),
                        from_i64(row.get::<_, i64>(3)?),
                        from_i64(row.get::<_, i64>(4)?),
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let mut previous_drive_free = None;
            let mut previous_categories: BTreeMap<StorageCategory, (u64, u64)> = BTreeMap::new();
            let mut result = Vec::with_capacity(snapshots.len());
            for (snapshot_id, captured_at, drive_total, drive_free, drive_available) in snapshots {
                let mut category_statement = database.prepare(
                    "SELECT category, total_bytes, file_count, directory_count,
                            skipped_reparse_points, access_denied_count,
                            unreadable_entries, truncated
                     FROM storage_history_categories
                     WHERE snapshot_id = ?1 ORDER BY category",
                )?;
                let raw_categories = category_statement
                    .query_map([snapshot_id], |row| {
                        let category_text: String = row.get(0)?;
                        let category =
                            StorageCategory::from_db(&category_text).ok_or_else(|| {
                                rusqlite::Error::InvalidColumnType(
                                    0,
                                    "category".to_owned(),
                                    rusqlite::types::Type::Text,
                                )
                            })?;
                        Ok((
                            category,
                            from_i64(row.get::<_, i64>(1)?),
                            from_i64(row.get::<_, i64>(2)?),
                            from_i64(row.get::<_, i64>(3)?),
                            from_i64(row.get::<_, i64>(4)?),
                            from_i64(row.get::<_, i64>(5)?),
                            from_i64(row.get::<_, i64>(6)?),
                            row.get::<_, i64>(7)? != 0,
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let mut categories = Vec::with_capacity(raw_categories.len());
                for (
                    category,
                    total_bytes,
                    file_count,
                    directory_count,
                    skipped_reparse_points,
                    access_denied_count,
                    unreadable_entries,
                    truncated,
                ) in raw_categories
                {
                    let previous = previous_categories.insert(category, (total_bytes, file_count));
                    categories.push(StorageCategoryPoint {
                        category,
                        total_bytes,
                        file_count,
                        directory_count,
                        total_bytes_delta: previous.map(|value| signed_delta(total_bytes, value.0)),
                        file_count_delta: previous.map(|value| signed_delta(file_count, value.1)),
                        skipped_reparse_points,
                        access_denied_count,
                        unreadable_entries,
                        truncated,
                    });
                }
                let drive_free_delta_bytes =
                    previous_drive_free.map(|previous| signed_delta(drive_free, previous));
                previous_drive_free = Some(drive_free);
                result.push(StorageHistoryPoint {
                    captured_at_unix_ms: captured_at,
                    drive_total_bytes: drive_total,
                    drive_total_free_bytes: drive_free,
                    drive_available_bytes: drive_available,
                    drive_free_delta_bytes,
                    categories,
                });
            }
            result.reverse();
            Ok(result)
        })
    }

    fn clear_storage_history(&self) -> CoreResult<u64> {
        self.with_immediate_transaction(|database| {
            let count: i64 = database.query_row(
                "SELECT COUNT(*) FROM storage_history_snapshots",
                [],
                |row| row.get(0),
            )?;
            database.execute("DELETE FROM storage_history_snapshots", [])?;
            Ok(u64::try_from(count).unwrap_or(u64::MAX))
        })
    }

    #[cfg(test)]
    fn storage_history_text_values(&self) -> CoreResult<Vec<String>> {
        self.with_connection(|database| {
            let mut values = Vec::new();
            for table in ["storage_history_snapshots", "storage_history_categories"] {
                let exists = database
                    .query_row(
                        "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                        [table],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                if let Some(sql) = exists {
                    values.push(sql);
                }
            }
            let mut statement =
                database.prepare("SELECT category FROM storage_history_categories")?;
            values.extend(
                statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?,
            );
            Ok(values)
        })
    }
}

fn signed_delta(current: u64, previous: u64) -> i64 {
    if current >= previous {
        i64::try_from(current - previous).unwrap_or(i64::MAX)
    } else {
        -i64::try_from(previous - current).unwrap_or(i64::MAX)
    }
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn from_i64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot(captured_at_unix_ms: u64, free: u64, bytes: u64) -> CapturedSnapshot {
        CapturedSnapshot {
            captured_at_unix_ms,
            drive_total_bytes: 1_000_000,
            drive_total_free_bytes: free,
            drive_available_bytes: free,
            categories: vec![FolderAggregate {
                category: StorageCategory::Downloads,
                total_bytes: bytes,
                file_count: 2,
                directory_count: 1,
                skipped_reparse_points: 0,
                access_denied_count: 0,
                unreadable_entries: 0,
                truncated: false,
            }],
        }
    }

    #[test]
    fn history_deltas_and_clear_are_not_action_timeline_entries() {
        let database = JournalDatabase::open_in_memory().expect("history database");
        database
            .insert_storage_history(&sample_snapshot(10, 900_000, 10_000))
            .expect("first snapshot");
        database
            .insert_storage_history(&sample_snapshot(20, 850_000, 60_000))
            .expect("second snapshot");

        let history = database.list_storage_history(10).expect("list history");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].drive_free_delta_bytes, Some(-50_000));
        assert_eq!(history[0].categories[0].total_bytes_delta, Some(50_000));
        assert!(
            database.list_timeline(10).expect("timeline").is_empty(),
            "storage observations never enter the Action timeline"
        );
        assert_eq!(database.clear_storage_history().expect("clear"), 2);
        assert!(database.list_storage_history(10).expect("empty").is_empty());
    }

    #[test]
    fn storage_schema_and_rows_have_no_name_or_path_storage() {
        let database = JournalDatabase::open_in_memory().expect("history database");
        database
            .insert_storage_history(&sample_snapshot(10, 900_000, 10_000))
            .expect("snapshot");
        let values = database
            .storage_history_text_values()
            .expect("inspect storage history tables")
            .join("\n")
            .to_ascii_lowercase();
        assert!(!values.contains("file_name"));
        assert!(!values.contains("filename"));
        assert!(!values.contains("path"));
        assert!(!values.contains("totonoe-private-marker"));
        assert!(values.contains("downloads"));
    }

    #[test]
    fn selected_categories_are_fixed_and_unique() {
        assert!(validate_categories(Vec::new()).is_err());
        assert!(
            validate_categories(vec![StorageCategory::Documents, StorageCategory::Documents,])
                .is_err()
        );
        assert_eq!(
            validate_categories(StorageCategory::ALL.to_vec())
                .expect("all fixed categories")
                .len(),
            5
        );
    }

    #[test]
    fn access_denied_is_counted_separately() {
        #[cfg(windows)]
        {
            let mut report = sample_snapshot(1, 1, 1).categories.remove(0);
            count_io_error(
                &mut report,
                &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            );
            assert_eq!(report.access_denied_count, 1);
            assert_eq!(report.unreadable_entries, 0);
        }
    }
}

#[cfg(all(test, windows))]
mod outward_tests {
    use std::{
        fs::File,
        io::{self, Write},
        os::windows::ffi::OsStrExt,
        path::Path,
        process::Command,
        thread,
        time::Duration,
    };

    use tempfile::TempDir;
    use windows::{core::PCWSTR, Win32::Storage::FileSystem::GetCompressedFileSizeW};

    use super::*;
    use crate::windows::{read_system_drive_space, MAX_TEMP_ENTRIES};

    const TEST_FILE_BYTES: u64 = 64 * 1024 * 1024;
    const DRIVE_TOLERANCE_BYTES: u64 = 64 * 1024 * 1024;
    const PRIVATE_FILE_MARKER: &str = "totonoe-private-marker-file.bin";
    const PRIVATE_PATH_MARKER: &str = "totonoe-private-marker-path";

    struct TestWorkspace {
        directory: Option<TempDir>,
    }

    impl TestWorkspace {
        fn new() -> Self {
            Self {
                directory: Some(tempfile::tempdir().expect("create isolated storage workspace")),
            }
        }

        fn root(&self) -> &Path {
            self.directory
                .as_ref()
                .expect("workspace remains present")
                .path()
        }
    }

    impl Drop for TestWorkspace {
        fn drop(&mut self) {
            if let Some(directory) = self.directory.take() {
                directory
                    .close()
                    .expect("remove isolated storage workspace");
            }
        }
    }

    fn external_available_space() -> u64 {
        let output = Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "([System.IO.DriveInfo]::new($env:SystemDrive)).AvailableFreeSpace",
            ])
            .output()
            .expect("run independent drive-space observer");
        assert!(
            output.status.success(),
            "independent drive observer succeeds"
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .rev()
            .find_map(|line| line.trim().parse::<u64>().ok())
            .expect("independent drive observer returns bytes")
    }

    fn write_allocated_file(path: &Path, length: u64) {
        let mut file = File::create(path).expect("create known-size test file");
        let mut chunk = vec![0u8; 1024 * 1024];
        let mut state = 0x7a39_d271_u32;
        for byte in &mut chunk {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *byte = state as u8;
        }
        for _ in 0..(length / chunk.len() as u64) {
            file.write_all(&chunk).expect("write known-size data");
        }
        file.sync_all().expect("flush known-size data");
    }

    fn allocation_size(path: &Path) -> u64 {
        let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
        wide.push(0);
        let mut high = 0u32;
        let low =
            unsafe { GetCompressedFileSizeW(PCWSTR::from_raw(wide.as_ptr()), Some(&mut high)) };
        (u64::from(high) << 32) | u64::from(low)
    }

    fn create_directory_cycle(root: &Path, link: &Path) -> io::Result<()> {
        match std::os::windows::fs::symlink_dir(root, link) {
            Ok(()) => Ok(()),
            Err(_) => {
                let status = Command::new("cmd.exe")
                    .arg("/d")
                    .arg("/c")
                    .arg("mklink")
                    .arg("/J")
                    .arg(link)
                    .arg(root)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()?;
                if status.success() {
                    Ok(())
                } else {
                    Err(io::Error::other("cannot create test reparse point"))
                }
            }
        }
    }

    fn wait_for_freed_space(previous: u64) -> u64 {
        let mut observed = external_available_space();
        for _ in 0..20 {
            if observed >= previous {
                break;
            }
            thread::sleep(Duration::from_millis(50));
            observed = external_available_space();
        }
        observed
    }

    fn within(value: u64, expected: u64, tolerance: u64) -> bool {
        value.abs_diff(expected) <= tolerance
    }

    /// 実利用者領域には触れず、Drop管理の専用領域だけで外向き効果を測る。
    #[test]
    #[ignore = "creates and removes bounded test data and invokes an independent OS observer"]
    fn storage_history_outward_verification() {
        let workspace = TestWorkspace::new();
        let root = workspace.root();
        let category = StorageCategory::Downloads;

        let drive_before = external_available_space();
        let before = scan_folder(category, root);
        assert!(!before.truncated);
        assert_eq!(before.file_count, 0);
        println!(
            "EVIDENCE: storage_history capture=before category_bytes={} files={} drive_free={}",
            before.total_bytes, before.file_count, drive_before
        );

        let private_directory = root.join(PRIVATE_PATH_MARKER);
        std::fs::create_dir(&private_directory).expect("create private marker directory");
        let test_file = private_directory.join(PRIVATE_FILE_MARKER);
        write_allocated_file(&test_file, TEST_FILE_BYTES);
        let allocated = allocation_size(&test_file);
        assert!(allocated > 0);
        let after_create = scan_folder(category, root);
        let drive_after_create = external_available_space();
        let category_growth = after_create.total_bytes - before.total_bytes;
        let drive_consumed = drive_before.saturating_sub(drive_after_create);
        assert_eq!(category_growth, TEST_FILE_BYTES);
        assert!(
            within(category_growth, allocated, 1024 * 1024),
            "category growth stays within allocation-size tolerance"
        );
        assert!(
            drive_after_create <= drive_before,
            "independent drive observer sees the expected direction"
        );
        assert!(
            within(drive_consumed, allocated, DRIVE_TOLERANCE_BYTES),
            "independent drive delta is within a bounded approximation"
        );
        println!(
            "EVIDENCE: storage_history capture=created category_delta={} allocation={} drive_delta={}",
            category_growth,
            allocated,
            signed_delta(drive_after_create, drive_before)
        );

        std::fs::remove_file(&test_file).expect("remove known-size test file");
        let after_delete = scan_folder(category, root);
        let drive_after_delete = wait_for_freed_space(drive_after_create);
        let category_return = after_create.total_bytes - after_delete.total_bytes;
        let drive_return = drive_after_delete.saturating_sub(drive_after_create);
        assert_eq!(category_return, TEST_FILE_BYTES);
        assert!(
            drive_after_delete >= drive_after_create,
            "independent drive observer returns in the opposite direction"
        );
        assert!(
            within(drive_return, allocated, DRIVE_TOLERANCE_BYTES),
            "released drive space is within a bounded approximation"
        );
        println!(
            "EVIDENCE: storage_history capture=deleted category_delta={} drive_delta={}",
            signed_delta(after_delete.total_bytes, after_create.total_bytes),
            signed_delta(drive_after_delete, drive_after_create)
        );

        let cycle = private_directory.join("cycle");
        create_directory_cycle(root, &cycle).expect("create reparse cycle");
        let with_cycle = scan_folder(category, root);
        assert_eq!(with_cycle.skipped_reparse_points, 1);
        assert!(!with_cycle.truncated);
        println!(
            "EVIDENCE: storage_history reparse skipped={} truncated={} files={}",
            with_cycle.skipped_reparse_points, with_cycle.truncated, with_cycle.file_count
        );
        std::fs::remove_dir(&cycle).expect("unlink reparse cycle");

        for index in 0..=MAX_TEMP_ENTRIES {
            File::create(private_directory.join(format!("{index:05}.tmp")))
                .expect("create bounded-budget entry");
        }
        let budget = scan_folder(category, root);
        assert!(budget.truncated, "budget hit is never reported as complete");
        assert!(budget.file_count <= MAX_TEMP_ENTRIES);
        println!(
            "EVIDENCE: storage_history budget files={} directories={} bytes={} truncated={}",
            budget.file_count, budget.directory_count, budget.total_bytes, budget.truncated
        );

        let database = JournalDatabase::open_in_memory().expect("privacy inspection database");
        let drive = read_system_drive_space().expect("read drive aggregate");
        database
            .insert_storage_history(&CapturedSnapshot {
                captured_at_unix_ms: 1,
                drive_total_bytes: drive.total_bytes,
                drive_total_free_bytes: drive.total_free_bytes,
                drive_available_bytes: drive.available_bytes,
                categories: vec![budget.clone()],
            })
            .expect("save aggregate-only snapshot");
        let stored_text = database
            .storage_history_text_values()
            .expect("read persisted text values");
        let joined = stored_text.join("\n");
        assert!(!joined.contains(PRIVATE_FILE_MARKER));
        assert!(!joined.contains(PRIVATE_PATH_MARKER));
        assert!(!joined.contains(root.as_os_str().to_string_lossy().as_ref()));
        println!(
            "EVIDENCE: storage_history db_rows=1 stored_text_values={} names=0 paths=0",
            stored_text.len()
        );
    }
}
