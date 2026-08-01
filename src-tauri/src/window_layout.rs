//! Private, typed storage for one explicitly captured window layout.
//!
//! Window titles can contain file names, mail subjects, or other personal data. They are
//! required to match a saved top-level window safely, but must never appear in logs or
//! diagnostics. The title wrapper and every enclosing snapshot type therefore redact `Debug`.

use std::{
    collections::HashSet,
    fmt,
    io::Read,
    path::{Path, PathBuf},
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    action::ProcessFileIdentity,
    error::{CoreError, CoreResult},
};

pub const MAX_WINDOW_LAYOUT_ENTRIES: usize = 64;
pub const MAX_WINDOW_TITLE_CHARS: usize = 512;
pub const MAX_WINDOW_CLASS_CHARS: usize = 256;
pub const MAX_WINDOW_LABEL_CHARS: usize = 128;
const MAX_WINDOW_LAYOUT_FILE_BYTES: u64 = 256 * 1024;
const WINDOW_LAYOUT_FILE_VERSION: u32 = 1;
const MAX_ABSOLUTE_COORDINATE: i32 = 1_000_000;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SensitiveWindowTitle(String);

impl SensitiveWindowTitle {
    pub(crate) fn new(value: String) -> Option<Self> {
        let length = value.chars().count();
        (length > 0 && length <= MAX_WINDOW_TITLE_CHARS).then_some(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn journal_redacted() -> Self {
        Self("[REDACTED]".to_owned())
    }
}

impl fmt::Debug for SensitiveWindowTitle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted-window-title>")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WindowRect {
    pub fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    pub fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }

    fn is_valid(self) -> bool {
        [self.left, self.top, self.right, self.bottom]
            .into_iter()
            .all(|value| value.unsigned_abs() <= MAX_ABSOLUTE_COORDINATE as u32)
            && self.right > self.left
            && self.bottom > self.top
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowPoint {
    pub x: i32,
    pub y: i32,
}

impl WindowPoint {
    fn is_valid(self) -> bool {
        self.x.unsigned_abs() <= MAX_ABSOLUTE_COORDINATE as u32
            && self.y.unsigned_abs() <= MAX_ABSOLUTE_COORDINATE as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedWindowPlacement {
    /// `WINDOWPLACEMENT.flags`, limited to the two durable documented flags.
    pub flags: u32,
    /// The exact documented `WINDOWPLACEMENT.showCmd` captured from Windows.
    pub show_cmd: u32,
    pub min_position: WindowPoint,
    pub max_position: WindowPoint,
    pub normal_position: WindowRect,
}

impl SavedWindowPlacement {
    fn is_valid(self) -> bool {
        // WPF_SETMINPOSITION | WPF_RESTORETOMAXIMIZED. ASYNC is call-time only.
        const DURABLE_FLAGS: u32 = 0x0001 | 0x0002;
        // Documented visible/restorable SW_* values. SW_HIDE is never accepted.
        const ALLOWED_SHOW_COMMANDS: &[u32] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
        self.flags & !DURABLE_FLAGS == 0
            && ALLOWED_SHOW_COMMANDS.contains(&self.show_cmd)
            && self.min_position.is_valid()
            && self.max_position.is_valid()
            && self.normal_position.is_valid()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedWindowPlacementEntry {
    pub entry_id: Uuid,
    pub process_file_identity: ProcessFileIdentity,
    pub application_label: String,
    pub class_name: String,
    pub title: SensitiveWindowTitle,
    pub placement: SavedWindowPlacement,
    /// External observation used by real-machine verification. Restoration uses
    /// `WINDOWPLACEMENT`, never this screen-coordinate rectangle.
    pub observed_rect: WindowRect,
}

impl fmt::Debug for SavedWindowPlacementEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SavedWindowPlacementEntry")
            .field("entry_id", &self.entry_id)
            .field("process_file_identity", &self.process_file_identity)
            .field("application_label", &self.application_label)
            .field("class_name", &self.class_name)
            .field("title", &"<redacted>")
            .field("placement", &self.placement)
            .field("observed_rect", &self.observed_rect)
            .finish()
    }
}

impl SavedWindowPlacementEntry {
    fn is_valid(&self) -> bool {
        let label_length = self.application_label.chars().count();
        let class_length = self.class_name.chars().count();
        self.entry_id != Uuid::nil()
            && label_length > 0
            && label_length <= MAX_WINDOW_LABEL_CHARS
            && !self.application_label.chars().any(char::is_control)
            && class_length > 0
            && class_length <= MAX_WINDOW_CLASS_CHARS
            && !self.class_name.chars().any(char::is_control)
            && !self.title.as_str().is_empty()
            && self.title.as_str().chars().count() <= MAX_WINDOW_TITLE_CHARS
            && self.placement.is_valid()
            && self.observed_rect.is_valid()
    }
}

/// Exact process instance captured immediately before an Action mutation.
///
/// Desired saved layouts intentionally survive an application restart, but a
/// rollback must never move a later process/window that happens to reuse the
/// same executable, class, and title.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalWindowPlacementEntry {
    pub saved: SavedWindowPlacementEntry,
    /// Opaque HWND value captured for this process instance. It is never
    /// rendered and is used only as an additional same-process replacement
    /// guard.
    pub window_handle_token: i64,
    pub process_id: u32,
    pub process_creation_time_100ns: u64,
    /// Random per-window SetPropW marker. It is never rendered, is reused by
    /// timeline backups for the same live HWND, and Windows removes it
    /// automatically when that HWND is destroyed. This distinguishes
    /// same-process HWND reuse without accumulating properties.
    pub window_instance_marker: u64,
}

impl fmt::Debug for OriginalWindowPlacementEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OriginalWindowPlacementEntry")
            .field("saved", &self.saved)
            .field("window_handle_token", &"<opaque>")
            .field("process_id", &self.process_id)
            .field(
                "process_creation_time_100ns",
                &self.process_creation_time_100ns,
            )
            .field("window_instance_marker", &"<opaque>")
            .finish()
    }
}

impl OriginalWindowPlacementEntry {
    pub(crate) fn is_valid(&self) -> bool {
        self.saved.is_valid()
            && self.window_handle_token != 0
            && self.process_id != 0
            && self.process_creation_time_100ns != 0
            && self.window_instance_marker != 0
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowLayoutSnapshot {
    pub snapshot_id: Uuid,
    pub captured_at_unix_ms: u64,
    pub entries: Vec<SavedWindowPlacementEntry>,
    pub excluded_game_windows: u32,
    pub skipped_windows: u32,
}

impl fmt::Debug for WindowLayoutSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowLayoutSnapshot")
            .field("snapshot_id", &self.snapshot_id)
            .field("captured_at_unix_ms", &self.captured_at_unix_ms)
            .field("entry_count", &self.entries.len())
            .field("excluded_game_windows", &self.excluded_game_windows)
            .field("skipped_windows", &self.skipped_windows)
            .finish()
    }
}

impl WindowLayoutSnapshot {
    pub fn validate(&self) -> CoreResult<()> {
        if self.snapshot_id == Uuid::nil()
            || self.captured_at_unix_ms == 0
            || self.entries.is_empty()
            || self.entries.len() > MAX_WINDOW_LAYOUT_ENTRIES
            || self.entries.iter().any(|entry| !entry.is_valid())
        {
            return Err(invalid_layout());
        }
        let unique_ids = self
            .entries
            .iter()
            .map(|entry| entry.entry_id)
            .collect::<HashSet<_>>();
        if unique_ids.len() != self.entries.len() {
            return Err(invalid_layout());
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowLayoutBackup {
    pub desired: WindowLayoutSnapshot,
    pub original_entries: Vec<OriginalWindowPlacementEntry>,
    pub excluded_game_file_identities: Vec<ProcessFileIdentity>,
}

impl fmt::Debug for WindowLayoutBackup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowLayoutBackup")
            .field("desired", &self.desired)
            .field("original_entry_count", &self.original_entries.len())
            .field(
                "excluded_game_file_identity_count",
                &self.excluded_game_file_identities.len(),
            )
            .finish()
    }
}

/// Opaque Action input assembled by the backend from the private saved layout.
///
/// This is journaled so crash recovery never depends on a later replacement of
/// the user's saved layout. Its custom `Debug` deliberately omits window data.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowLayoutInvocation {
    pub desired: WindowLayoutSnapshot,
    pub excluded_game_file_identities: Vec<ProcessFileIdentity>,
}

impl fmt::Debug for WindowLayoutInvocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowLayoutInvocation")
            .field("snapshot_id", &self.desired.snapshot_id)
            .field("window_count", &self.desired.entries.len())
            .field(
                "excluded_game_file_identity_count",
                &self.excluded_game_file_identities.len(),
            )
            .finish()
    }
}

impl WindowLayoutInvocation {
    pub fn validate(&self) -> CoreResult<()> {
        self.desired.validate()?;
        // A normal invocation keeps the stored and freshly resolved identity
        // for every bounded profile. Recovery unions that journaled set with
        // the current profile set, so the conservative bound is twice that
        // normal maximum.
        if self.excluded_game_file_identities.len() > 800 {
            return Err(invalid_layout());
        }
        let unique = self
            .excluded_game_file_identities
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        if unique.len() != self.excluded_game_file_identities.len() {
            return Err(invalid_layout());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowLayoutIssueReason {
    NotRunning,
    AmbiguousMatch,
    GameExcluded,
    ExternalChange,
    VerificationMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowLayoutIssue {
    /// Safe UI identifier such as `chrome.exe（2）`; never a window title.
    pub target: String,
    pub reason: WindowLayoutIssueReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowLayoutObservation {
    pub saved_window_count: u32,
    pub matched_window_count: u32,
    pub positioned_window_count: u32,
    pub issues: Vec<WindowLayoutIssue>,
    /// Privacy-safe hash of exact matched process instances, placements, and
    /// rectangles. It is used by stale-preview and external-change checks but
    /// never rendered as a UI item.
    pub geometry_fingerprint: String,
}

impl WindowLayoutObservation {
    pub fn has_verification_mismatch(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.reason == WindowLayoutIssueReason::VerificationMismatch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowLayoutStatus {
    pub saved: bool,
    pub snapshot_id: Option<Uuid>,
    pub saved_at_unix_ms: Option<u64>,
    pub window_count: u32,
    pub excluded_game_windows: u32,
    pub skipped_windows: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowLayoutFile {
    version: u32,
    snapshot: WindowLayoutSnapshot,
}

#[derive(Debug)]
pub struct WindowLayoutStore {
    path: PathBuf,
    snapshot: Mutex<Option<WindowLayoutSnapshot>>,
}

impl WindowLayoutStore {
    pub fn open(path: PathBuf) -> CoreResult<Self> {
        let snapshot = match std::fs::File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_WINDOW_LAYOUT_FILE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| CoreError::storage())?;
                if bytes.len() as u64 > MAX_WINDOW_LAYOUT_FILE_BYTES {
                    return Err(CoreError::new(
                        "WINDOW_LAYOUT_FILE_TOO_LARGE",
                        "BOOTSTRAP",
                        false,
                        "ウィンドウ配置の保存データが読込上限を超えています。",
                    ));
                }
                let file: WindowLayoutFile = serde_json::from_slice(&bytes).map_err(|_| {
                    CoreError::new(
                        "WINDOW_LAYOUT_FILE_CORRUPT",
                        "BOOTSTRAP",
                        false,
                        "ウィンドウ配置の保存データを安全に検証できません。",
                    )
                })?;
                if file.version != WINDOW_LAYOUT_FILE_VERSION {
                    return Err(CoreError::new(
                        "WINDOW_LAYOUT_FILE_VERSION",
                        "BOOTSTRAP",
                        false,
                        "ウィンドウ配置の保存データの版が対応外です。",
                    ));
                }
                file.snapshot.validate()?;
                Some(file.snapshot)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(CoreError::storage()),
        };
        Ok(Self {
            path,
            snapshot: Mutex::new(snapshot),
        })
    }

    pub fn status(&self) -> WindowLayoutStatus {
        let guard = self.snapshot.lock();
        status_for_snapshot(guard.as_ref())
    }

    pub fn current_snapshot_id(&self) -> CoreResult<Uuid> {
        self.snapshot
            .lock()
            .as_ref()
            .map(|snapshot| snapshot.snapshot_id)
            .ok_or_else(|| {
                CoreError::invalid_request(
                    "先に、現在開いているウィンドウの配置を明示的に保存してください。",
                )
            })
    }

    pub fn snapshot(&self, snapshot_id: Uuid) -> CoreResult<WindowLayoutSnapshot> {
        self.snapshot
            .lock()
            .as_ref()
            .filter(|snapshot| snapshot.snapshot_id == snapshot_id)
            .cloned()
            .ok_or_else(|| {
                CoreError::invalid_request(
                    "保存済みの配置が更新されています。現在の配置を確認し直してください。",
                )
            })
    }

    pub fn replace(&self, snapshot: WindowLayoutSnapshot) -> CoreResult<WindowLayoutStatus> {
        // The file and in-memory snapshot are one serialized state transition.
        // Holding the same lock through serialization, durable replacement, and
        // memory update prevents concurrent saves from diverging disk and memory.
        let mut guard = self.snapshot.lock();
        snapshot.validate()?;
        let file = WindowLayoutFile {
            version: WINDOW_LAYOUT_FILE_VERSION,
            snapshot: snapshot.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|_| CoreError::storage())?;
        if bytes.len() as u64 > MAX_WINDOW_LAYOUT_FILE_BYTES {
            return Err(CoreError::invalid_request(
                "保存対象のウィンドウが配置データの上限を超えています。",
            ));
        }
        replace_file_atomically(&self.path, &bytes)?;
        *guard = Some(snapshot);
        Ok(status_for_snapshot(guard.as_ref()))
    }
}

fn status_for_snapshot(snapshot: Option<&WindowLayoutSnapshot>) -> WindowLayoutStatus {
    match snapshot {
        Some(snapshot) => WindowLayoutStatus {
            saved: true,
            snapshot_id: Some(snapshot.snapshot_id),
            saved_at_unix_ms: Some(snapshot.captured_at_unix_ms),
            window_count: u32::try_from(snapshot.entries.len()).unwrap_or(u32::MAX),
            excluded_game_windows: snapshot.excluded_game_windows,
            skipped_windows: snapshot.skipped_windows,
        },
        None => WindowLayoutStatus {
            saved: false,
            snapshot_id: None,
            saved_at_unix_ms: None,
            window_count: 0,
            excluded_game_windows: 0,
            skipped_windows: 0,
        },
    }
}

fn invalid_layout() -> CoreError {
    CoreError::new(
        "WINDOW_LAYOUT_INVALID",
        "VALIDATE",
        false,
        "保存済みのウィンドウ配置を安全に検証できません。",
    )
}

fn replace_file_atomically(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("window-layout.json");
    let temporary = path.with_file_name(format!("{file_name}.{}.tmp", Uuid::new_v4()));
    std::fs::write(&temporary, bytes).map_err(|_| CoreError::storage())?;
    if let Err(error) = replace_file(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> CoreResult<()> {
    use windows::{
        core::HSTRING,
        Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        },
    };

    let from = HSTRING::from(from.as_os_str());
    let to = HSTRING::from(to.as_os_str());
    unsafe {
        MoveFileExW(
            &from,
            &to,
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|_| CoreError::storage())
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> CoreResult<()> {
    std::fs::rename(from, to).map_err(|_| CoreError::storage())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str) -> SavedWindowPlacementEntry {
        SavedWindowPlacementEntry {
            entry_id: Uuid::new_v4(),
            process_file_identity: ProcessFileIdentity {
                volume_serial_number: 1,
                file_id: [2; 16],
            },
            application_label: "test-app.exe".to_owned(),
            class_name: "PcCustomWindowLayoutTest".to_owned(),
            title: SensitiveWindowTitle::new(title.to_owned()).expect("bounded test title"),
            placement: SavedWindowPlacement {
                flags: 0,
                show_cmd: 1,
                min_position: WindowPoint { x: -1, y: -1 },
                max_position: WindowPoint { x: -1, y: -1 },
                normal_position: WindowRect {
                    left: 100,
                    top: 100,
                    right: 500,
                    bottom: 400,
                },
            },
            observed_rect: WindowRect {
                left: 100,
                top: 100,
                right: 500,
                bottom: 400,
            },
        }
    }

    fn snapshot(title: &str) -> WindowLayoutSnapshot {
        WindowLayoutSnapshot {
            snapshot_id: Uuid::new_v4(),
            captured_at_unix_ms: 1,
            entries: vec![entry(title)],
            excluded_game_windows: 1,
            skipped_windows: 2,
        }
    }

    #[test]
    fn debug_output_never_contains_the_private_window_title() {
        let private = "private mail subject 8f3f11";
        let value = snapshot(private);
        assert!(!format!("{value:?}").contains(private));
        assert!(!format!("{:?}", value.entries[0]).contains(private));
        let backup = WindowLayoutBackup {
            desired: value.clone(),
            original_entries: value
                .entries
                .iter()
                .cloned()
                .map(|saved| OriginalWindowPlacementEntry {
                    saved,
                    window_handle_token: 7,
                    process_id: 42,
                    process_creation_time_100ns: 84,
                    window_instance_marker: 1,
                })
                .collect(),
            excluded_game_file_identities: Vec::new(),
        };
        assert!(!format!("{backup:?}").contains(private));
    }

    #[test]
    fn snapshot_validation_rejects_duplicate_entry_ids_and_unbounded_titles() {
        let mut duplicate = snapshot("first");
        let mut second = entry("second");
        second.entry_id = duplicate.entries[0].entry_id;
        duplicate.entries.push(second);
        assert!(duplicate.validate().is_err());

        let too_long = "x".repeat(MAX_WINDOW_TITLE_CHARS + 1);
        assert!(SensitiveWindowTitle::new(too_long).is_none());
    }

    #[test]
    fn store_round_trips_and_replaces_the_single_saved_layout() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("window-layout.json");
        let store = WindowLayoutStore::open(path.clone()).expect("open empty store");
        assert!(!store.status().saved);

        let first = snapshot("fixed test title one");
        store.replace(first.clone()).expect("save first layout");
        assert_eq!(
            store
                .snapshot(first.snapshot_id)
                .expect("read first layout"),
            first
        );

        let second = snapshot("fixed test title two");
        store.replace(second.clone()).expect("replace layout");
        assert!(store.snapshot(first.snapshot_id).is_err());
        assert_eq!(
            WindowLayoutStore::open(path)
                .expect("reopen store")
                .snapshot(second.snapshot_id)
                .expect("read second layout"),
            second
        );
    }

    #[test]
    fn concurrent_replacements_leave_memory_and_disk_on_the_same_generation() {
        use std::sync::{Arc, Barrier};

        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("window-layout.json");
        let store = Arc::new(WindowLayoutStore::open(path.clone()).expect("open empty store"));
        let barrier = Arc::new(Barrier::new(3));
        let mut threads = Vec::new();
        for index in 0..2 {
            let store = store.clone();
            let barrier = barrier.clone();
            threads.push(std::thread::spawn(move || {
                let value = snapshot(&format!("fixed concurrent title {index}"));
                barrier.wait();
                store.replace(value).expect("serialized replacement");
            }));
        }
        barrier.wait();
        for thread in threads {
            thread.join().expect("replacement thread");
        }

        let in_memory = store.current_snapshot_id().expect("in-memory snapshot");
        let on_disk = WindowLayoutStore::open(path)
            .expect("reopen final store")
            .current_snapshot_id()
            .expect("on-disk snapshot");
        assert_eq!(in_memory, on_disk);
    }
}
