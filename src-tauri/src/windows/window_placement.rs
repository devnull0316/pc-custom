//! Bounded, fail-closed window-placement observation and restoration.
//!
//! A window title is part of the exact matching key because a process may own several
//! top-level windows. Titles can contain private data, so this module never formats, logs, or
//! returns one outside `SensitiveWindowTitle`, whose `Debug` implementation is redacted.

use crate::{
    action::ProcessFileIdentity,
    backup::Fingerprint,
    window_layout::{
        OriginalWindowPlacementEntry, SavedWindowPlacement, SavedWindowPlacementEntry,
        SensitiveWindowTitle, WindowLayoutIssue, WindowLayoutIssueReason, WindowLayoutObservation,
        WindowLayoutSnapshot, WindowPoint, WindowRect, MAX_WINDOW_CLASS_CHARS,
        MAX_WINDOW_LABEL_CHARS, MAX_WINDOW_LAYOUT_ENTRIES, MAX_WINDOW_TITLE_CHARS,
    },
};

use super::{WindowsError, WindowsErrorKind, WindowsResult};

const MAX_ENUMERATED_VISIBLE_WINDOWS: usize = 2_048;
const MAX_ABSOLUTE_COORDINATE: i32 = 1_000_000;
const DURABLE_PLACEMENT_FLAGS: u32 = 0x0001 | 0x0002;
const WINDOW_RESPONSIVENESS_TIMEOUT_MS: u32 = 500;
const WINDOW_INSTANCE_PROPERTY_NAME: &str =
    "Totonoe.WindowPlacement.Instance.v1.80f532c4";

// Keep the pure eligibility checks available to unit tests on every target.
const STYLE_POPUP: u32 = 0x8000_0000;
const STYLE_CHILD: u32 = 0x4000_0000;
const STYLE_DISABLED: u32 = 0x0800_0000;
const STYLE_CAPTION: u32 = 0x00c0_0000;
const STYLE_SYSTEM_MENU: u32 = 0x0008_0000;
const EX_STYLE_TOOL_WINDOW: u32 = 0x0000_0080;

#[cfg(all(test, windows))]
static ALLOW_OWN_WINDOW_CANDIDATES: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Test-only scope for a real Action lifecycle smoke. Production builds do not
/// contain a switch that can bypass Totonoe's own-process exclusion.
#[cfg(all(test, windows))]
pub(crate) struct AllowOwnWindowCandidatesForTest;

#[cfg(all(test, windows))]
impl Drop for AllowOwnWindowCandidatesForTest {
    fn drop(&mut self) {
        ALLOW_OWN_WINDOW_CANDIDATES.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(all(test, windows))]
pub(crate) fn allow_own_window_candidates_for_test() -> AllowOwnWindowCandidatesForTest {
    assert!(
        !ALLOW_OWN_WINDOW_CANDIDATES.swap(true, std::sync::atomic::Ordering::SeqCst),
        "owned-window test scope must be serialized"
    );
    AllowOwnWindowCandidatesForTest
}

#[derive(Clone)]
struct WindowCandidate {
    handle: isize,
    process_id: u32,
    process_creation_time_100ns: u64,
    window_instance_marker: u64,
    process_file_identity: ProcessFileIdentity,
    application_label: String,
    class_name: String,
    title: SensitiveWindowTitle,
    placement: SavedWindowPlacement,
    observed_rect: WindowRect,
}

struct CandidateReport {
    candidates: Vec<WindowCandidate>,
    excluded_game_windows: u32,
    skipped_windows: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchResolution {
    Missing,
    Ambiguous,
    Unique(usize),
}

/// Privacy-safe ownership classification used by crash recovery.
///
/// `MixedOwned` means every mutable target is still the exact backed-up
/// process/window instance and each one is at either Totonoe's desired
/// placement or its backed-up original placement. A true third placement is
/// never collapsed into this state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowLayoutTransactionState {
    Original,
    Desired,
    MixedOwned,
    Third,
}

/// Capture the current placement of eligible, non-game top-level application windows.
///
/// The result is deliberately capped. Extra eligible windows are counted as skipped instead of
/// growing a persisted snapshot without bound.
#[cfg(windows)]
pub fn capture_window_layout(
    excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<WindowLayoutSnapshot> {
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;

    let report = enumerate_candidates(excluded_game_file_identities, false)?;
    let mut skipped_windows = report.skipped_windows;
    let mut labels = std::collections::HashMap::<String, u32>::new();
    let mut entries = Vec::with_capacity(report.candidates.len().min(MAX_WINDOW_LAYOUT_ENTRIES));

    for candidate in report.candidates {
        if entries.len() == MAX_WINDOW_LAYOUT_ENTRIES {
            skipped_windows = skipped_windows.saturating_add(1);
            continue;
        }
        let ordinal = labels
            .entry(candidate.application_label.clone())
            .and_modify(|value| *value = value.saturating_add(1))
            .or_insert(1);
        entries.push(entry_from_candidate(
            &candidate,
            Uuid::new_v4(),
            numbered_application_label(&candidate.application_label, *ordinal),
        ));
    }

    if entries.is_empty() {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "capture eligible window placement",
            None,
        ));
    }
    let captured_at_unix_ms: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| {
            WindowsError::new(
                WindowsErrorKind::InvalidData,
                "read window placement capture time",
                None,
            )
        })?
        .as_millis()
        .try_into()
        .map_err(|_| {
            WindowsError::new(
                WindowsErrorKind::InvalidData,
                "bound window placement capture time",
                None,
            )
        })?;
    let snapshot = WindowLayoutSnapshot {
        snapshot_id: Uuid::new_v4(),
        captured_at_unix_ms: captured_at_unix_ms.max(1),
        entries,
        excluded_game_windows: report.excluded_game_windows,
        skipped_windows,
    };
    snapshot.validate().map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate captured window placement",
            None,
        )
    })?;
    Ok(snapshot)
}

#[cfg(not(windows))]
pub fn capture_window_layout(
    _excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<WindowLayoutSnapshot> {
    Err(WindowsError::unsupported("capture window placement"))
}

/// Observe how many saved windows can still be identified exactly and are at the saved placement.
#[cfg(windows)]
pub fn observe_window_layout(
    desired: &WindowLayoutSnapshot,
    excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<WindowLayoutObservation> {
    validate_snapshot(desired)?;
    let report = enumerate_candidates(excluded_game_file_identities, false)?;
    Ok(observe_entries(
        &desired.entries,
        &report.candidates,
        excluded_game_file_identities,
    ))
}

#[cfg(not(windows))]
pub fn observe_window_layout(
    _desired: &WindowLayoutSnapshot,
    _excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<WindowLayoutObservation> {
    Err(WindowsError::unsupported("observe window placement"))
}

/// Capture the exact pre-apply placement of each currently unambiguous saved target.
///
/// Missing, newly game-excluded, or ambiguous targets are intentionally absent. A subsequent
/// restore reports those conditions without touching an uncertain window.
#[cfg(windows)]
pub fn capture_window_layout_originals(
    desired: &WindowLayoutSnapshot,
    excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<Vec<OriginalWindowPlacementEntry>> {
    validate_snapshot(desired)?;
    let report = enumerate_candidates(excluded_game_file_identities, false)?;
    let mut originals = Vec::with_capacity(desired.entries.len());
    for (desired_index, entry) in desired.entries.iter().enumerate() {
        if excluded_game_file_identities.contains(&entry.process_file_identity) {
            continue;
        }
        if let MatchResolution::Unique(candidate_index) =
            resolve_match(desired_index, &desired.entries, &report.candidates)
        {
            let current = &report.candidates[candidate_index];
            let window_instance_marker = attach_new_window_instance_marker(current.handle)?;
            originals.push(OriginalWindowPlacementEntry {
                saved: SavedWindowPlacementEntry {
                    entry_id: entry.entry_id,
                    process_file_identity: entry.process_file_identity,
                    application_label: entry.application_label.clone(),
                    class_name: entry.class_name.clone(),
                    title: entry.title.clone(),
                    placement: current.placement,
                    observed_rect: current.observed_rect,
                },
                window_handle_token: current.handle as i64,
                process_id: current.process_id,
                process_creation_time_100ns: current.process_creation_time_100ns,
                window_instance_marker,
            });
        }
    }
    Ok(originals)
}

/// Re-read a marked backup capture without creating or moving any marker.
///
/// The second pass is deliberately verification-only: if the original HWND
/// was destroyed and its numeric value was reused, the replacement window does
/// not inherit the random SetPropW marker and the capture is rejected.
#[cfg(windows)]
pub fn verify_captured_window_layout_originals(
    desired: &WindowLayoutSnapshot,
    originals: &[OriginalWindowPlacementEntry],
    excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<bool> {
    validate_snapshot(desired)?;
    validate_originals(desired, originals)?;
    let mut report = enumerate_candidates(excluded_game_file_identities, false)?;
    bind_original_window_markers(&mut report.candidates, originals);

    for (desired_index, desired_entry) in desired.entries.iter().enumerate() {
        let expected = originals
            .iter()
            .find(|original| original.saved.entry_id == desired_entry.entry_id);
        if excluded_game_file_identities.contains(&desired_entry.process_file_identity) {
            if expected.is_some() {
                return Ok(false);
            }
            continue;
        }
        match resolve_match(desired_index, &desired.entries, &report.candidates) {
            MatchResolution::Unique(_) => {
                let Some(expected) = expected else {
                    return Ok(false);
                };
                let Some(candidate) =
                    resolve_original_candidate(expected, &report.candidates)
                else {
                    return Ok(false);
                };
                if !candidate_matches_saved(candidate, &expected.saved) {
                    return Ok(false);
                }
            }
            MatchResolution::Missing | MatchResolution::Ambiguous => {
                if expected.is_some() {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

#[cfg(not(windows))]
pub fn capture_window_layout_originals(
    _desired: &WindowLayoutSnapshot,
    _excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<Vec<OriginalWindowPlacementEntry>> {
    Err(WindowsError::unsupported(
        "capture original window placement",
    ))
}

#[cfg(not(windows))]
pub fn verify_captured_window_layout_originals(
    _desired: &WindowLayoutSnapshot,
    _originals: &[OriginalWindowPlacementEntry],
    _excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<bool> {
    Err(WindowsError::unsupported(
        "verify original window placement capture",
    ))
}

/// Restore one validated saved layout.
#[cfg(windows)]
pub fn restore_window_layout(
    desired: &WindowLayoutSnapshot,
    originals: &[OriginalWindowPlacementEntry],
    excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<WindowLayoutObservation> {
    validate_snapshot(desired)?;
    validate_originals(desired, originals)?;
    mutate_transaction(
        desired,
        originals,
        excluded_game_file_identities,
        MutationDirection::Apply,
        false,
    )
}

#[cfg(not(windows))]
pub fn restore_window_layout(
    _desired: &WindowLayoutSnapshot,
    _originals: &[OriginalWindowPlacementEntry],
    _excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<WindowLayoutObservation> {
    Err(WindowsError::unsupported("restore window placement"))
}

/// Restore a captured set of original placements. This is the compensation/rollback primitive.
#[cfg(windows)]
pub fn restore_window_placement_entries(
    desired: &WindowLayoutSnapshot,
    originals: &[OriginalWindowPlacementEntry],
    excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<WindowLayoutObservation> {
    validate_snapshot(desired)?;
    validate_originals(desired, originals)?;
    mutate_transaction(
        desired,
        originals,
        excluded_game_file_identities,
        MutationDirection::Rollback,
        false,
    )
}

#[cfg(not(windows))]
pub fn restore_window_placement_entries(
    _desired: &WindowLayoutSnapshot,
    _originals: &[OriginalWindowPlacementEntry],
    _excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<WindowLayoutObservation> {
    Err(WindowsError::unsupported(
        "restore original window placement",
    ))
}

/// Observe only exact backed-up process/window instances at their original
/// placements. A later process that reuses the same executable/class/title is
/// reported as missing and is never treated as rollback evidence.
#[cfg(windows)]
pub fn observe_original_window_placements(
    originals: &[OriginalWindowPlacementEntry],
    excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<WindowLayoutObservation> {
    validate_original_entries(originals)?;
    let mut report = enumerate_candidates(excluded_game_file_identities, false)?;
    bind_original_window_markers(&mut report.candidates, originals);
    Ok(observe_original_entries(
        originals,
        &report.candidates,
        excluded_game_file_identities,
    ))
}

#[cfg(not(windows))]
pub fn observe_original_window_placements(
    _originals: &[OriginalWindowPlacementEntry],
    _excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<WindowLayoutObservation> {
    Err(WindowsError::unsupported(
        "observe original window placement",
    ))
}

/// Classify each backed-up target independently so recovery can distinguish a
/// partial Totonoe dispatch from a genuine external third state.
#[cfg(windows)]
pub fn classify_window_layout_transaction(
    desired: &WindowLayoutSnapshot,
    originals: &[OriginalWindowPlacementEntry],
    excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<WindowLayoutTransactionState> {
    validate_snapshot(desired)?;
    validate_originals(desired, originals)?;
    let mut report = enumerate_candidates(excluded_game_file_identities, false)?;
    bind_original_window_markers(&mut report.candidates, originals);
    Ok(classify_transaction_entries(
        desired,
        originals,
        &report.candidates,
        excluded_game_file_identities,
    ))
}

#[cfg(not(windows))]
pub fn classify_window_layout_transaction(
    _desired: &WindowLayoutSnapshot,
    _originals: &[OriginalWindowPlacementEntry],
    _excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowsResult<WindowLayoutTransactionState> {
    Err(WindowsError::unsupported(
        "classify window placement transaction",
    ))
}

#[cfg(windows)]
fn validate_snapshot(snapshot: &WindowLayoutSnapshot) -> WindowsResult<()> {
    snapshot.validate().map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate saved window placement",
            None,
        )
    })
}

#[cfg(windows)]
fn validate_original_entries(entries: &[OriginalWindowPlacementEntry]) -> WindowsResult<()> {
    if entries.len() > MAX_WINDOW_LAYOUT_ENTRIES || entries.iter().any(|entry| !entry.is_valid()) {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate original window placement",
            None,
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_originals(
    desired: &WindowLayoutSnapshot,
    originals: &[OriginalWindowPlacementEntry],
) -> WindowsResult<()> {
    validate_original_entries(originals)?;
    let mut ids = std::collections::HashSet::with_capacity(originals.len());
    for original in originals {
        if !ids.insert(original.saved.entry_id) {
            return Err(invalid_originals());
        }
        let Some(target) = desired
            .entries
            .iter()
            .find(|entry| entry.entry_id == original.saved.entry_id)
        else {
            return Err(invalid_originals());
        };
        if !entry_keys_match(target, &original.saved) {
            return Err(invalid_originals());
        }
    }
    Ok(())
}

#[cfg(windows)]
fn invalid_originals() -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::InvalidData,
        "validate original window placement binding",
        None,
    )
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MutationDirection {
    Apply,
    Rollback,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnedPlacementState {
    Original,
    Desired,
}

#[cfg(windows)]
#[derive(Clone)]
struct PlannedMutation {
    desired: SavedWindowPlacementEntry,
    original: OriginalWindowPlacementEntry,
    candidate: WindowCandidate,
    source_state: OwnedPlacementState,
}

#[cfg(windows)]
fn mutate_transaction(
    desired: &WindowLayoutSnapshot,
    originals: &[OriginalWindowPlacementEntry],
    excluded_game_file_identities: &[ProcessFileIdentity],
    direction: MutationDirection,
    include_own_process_for_test: bool,
) -> WindowsResult<WindowLayoutObservation> {
    let mut report =
        enumerate_candidates(excluded_game_file_identities, include_own_process_for_test)?;
    bind_original_window_markers(&mut report.candidates, originals);
    let mut planned = Vec::with_capacity(originals.len());

    // Resolve and classify every mutable target before the first write. Only
    // entry IDs with a durable original are eligible for this transaction.
    for original in originals {
        let desired_entry = desired
            .entries
            .iter()
            .find(|entry| entry.entry_id == original.saved.entry_id)
            .ok_or_else(invalid_originals)?;
        if excluded_game_file_identities.contains(&desired_entry.process_file_identity) {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "registered game became a window placement target",
                None,
            ));
        }
        let candidate = resolve_original_candidate(original, &report.candidates)
            .ok_or_else(|| {
                WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "resolve exact original window process instance",
                    None,
                )
            })?
            .clone();
        let at_original = candidate_matches_saved(&candidate, &original.saved);
        let at_desired = candidate_matches_saved(&candidate, desired_entry);
        match direction {
            MutationDirection::Apply if !at_original => {
                return Err(WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "window placement changed after backup",
                    None,
                ));
            }
            MutationDirection::Rollback if !at_original && !at_desired => {
                return Err(WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "window placement has an external third state",
                    None,
                ));
            }
            MutationDirection::Apply if at_desired => continue,
            _ => {
                let source_state = if at_original {
                    OwnedPlacementState::Original
                } else {
                    OwnedPlacementState::Desired
                };
                planned.push(PlannedMutation {
                    desired: desired_entry.clone(),
                    original: original.clone(),
                    candidate,
                    source_state,
                });
            }
        }
    }

    let mut dispatched = Vec::with_capacity(planned.len());
    for mutation in planned {
        // Re-read the handle, process instance, matching key, and geometry
        // immediately before every SetWindowPlacement dispatch.
        let Some(current) = refresh_candidate(
            &mutation.candidate,
            &mutation.desired,
            mutation.original.window_instance_marker,
            excluded_game_file_identities,
            include_own_process_for_test,
        )?
        else {
            return mutation_failure_with_compensation(
                WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "window target changed immediately before placement write",
                    None,
                ),
                &dispatched,
                direction,
                excluded_game_file_identities,
                include_own_process_for_test,
            );
        };
        if current.process_id != mutation.original.process_id
            || current.process_creation_time_100ns != mutation.original.process_creation_time_100ns
            || current.window_instance_marker != mutation.original.window_instance_marker
        {
            return mutation_failure_with_compensation(
                WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "window process instance changed immediately before placement write",
                    None,
                ),
                &dispatched,
                direction,
                excluded_game_file_identities,
                include_own_process_for_test,
            );
        }

        let at_original = candidate_matches_saved(&current, &mutation.original.saved);
        let at_desired = candidate_matches_saved(&current, &mutation.desired);
        let (destination, source_state) = match direction {
            MutationDirection::Apply if at_original => (
                mutation.desired.placement,
                OwnedPlacementState::Original,
            ),
            // Reassert the original placement even when the immediate read is
            // already original. If the prior process died while a synchronous
            // SetWindowPlacement call was in flight, recovery must not
            // terminalize the journal from the pre-call observation alone.
            MutationDirection::Rollback if at_original => (
                mutation.original.saved.placement,
                OwnedPlacementState::Original,
            ),
            MutationDirection::Rollback if at_desired => (
                mutation.original.saved.placement,
                OwnedPlacementState::Desired,
            ),
            _ => {
                return mutation_failure_with_compensation(
                    WindowsError::new(
                        WindowsErrorKind::InvalidData,
                        "window geometry changed immediately before placement write",
                        None,
                    ),
                    &dispatched,
                    direction,
                    excluded_game_file_identities,
                    include_own_process_for_test,
                );
            }
        };
        if let Err(error) = set_saved_placement(
            current.handle,
            mutation.original.window_instance_marker,
            destination,
        ) {
            return mutation_failure_with_compensation(
                error,
                &dispatched,
                direction,
                excluded_game_file_identities,
                include_own_process_for_test,
            );
        }
        dispatched.push(PlannedMutation {
            candidate: current,
            source_state,
            ..mutation
        });
    }

    if !verify_mutations(
        &dispatched,
        direction,
        excluded_game_file_identities,
        include_own_process_for_test,
    ) {
        return mutation_failure_with_compensation(
            WindowsError::new(
                WindowsErrorKind::InvalidData,
                "verify SetWindowPlacement result",
                None,
            ),
            &dispatched,
            direction,
            excluded_game_file_identities,
            include_own_process_for_test,
        );
    }

    let mut final_report =
        enumerate_candidates(excluded_game_file_identities, include_own_process_for_test)?;
    bind_original_window_markers(&mut final_report.candidates, originals);
    let final_classification = classify_transaction_entries(
        desired,
        originals,
        &final_report.candidates,
        excluded_game_file_identities,
    );
    let final_owned_state = match direction {
        MutationDirection::Apply => matches!(
            final_classification,
            WindowLayoutTransactionState::Desired | WindowLayoutTransactionState::Original
        ),
        MutationDirection::Rollback => {
            final_classification == WindowLayoutTransactionState::Original
        }
    };
    if !final_owned_state {
        return mutation_failure_with_compensation(
            WindowsError::new(
                WindowsErrorKind::InvalidData,
                "window placement changed during final verification",
                None,
            ),
            &dispatched,
            direction,
            excluded_game_file_identities,
            include_own_process_for_test,
        );
    }
    Ok(match direction {
        MutationDirection::Apply => observe_entries(
            &desired.entries,
            &final_report.candidates,
            excluded_game_file_identities,
        ),
        MutationDirection::Rollback => observe_original_entries(
            originals,
            &final_report.candidates,
            excluded_game_file_identities,
        ),
    })
}

#[cfg(windows)]
fn mutation_failure_with_compensation(
    error: WindowsError,
    dispatched: &[PlannedMutation],
    _direction: MutationDirection,
    excluded_game_file_identities: &[ProcessFileIdentity],
    include_own_process_for_test: bool,
) -> WindowsResult<WindowLayoutObservation> {
    if compensate_dispatched(
        dispatched,
        excluded_game_file_identities,
        include_own_process_for_test,
    ) {
        Err(error)
    } else {
        Err(WindowsError::new(
            WindowsErrorKind::RecoveryRequired,
            "compensate partial window placement transaction",
            error.os_code,
        ))
    }
}

#[cfg(windows)]
fn compensate_dispatched(
    dispatched: &[PlannedMutation],
    excluded_game_file_identities: &[ProcessFileIdentity],
    include_own_process_for_test: bool,
) -> bool {
    let mut all_dispatched = true;
    for mutation in dispatched.iter().rev() {
        let Ok(Some(current)) = refresh_candidate(
            &mutation.candidate,
            &mutation.desired,
            mutation.original.window_instance_marker,
            excluded_game_file_identities,
            include_own_process_for_test,
        ) else {
            all_dispatched = false;
            continue;
        };
        if current.process_id != mutation.original.process_id
            || current.process_creation_time_100ns != mutation.original.process_creation_time_100ns
            || current.window_instance_marker != mutation.original.window_instance_marker
        {
            all_dispatched = false;
            continue;
        }
        let at_original = candidate_matches_saved(&current, &mutation.original.saved);
        let at_desired = candidate_matches_saved(&current, &mutation.desired);
        if !at_original && !at_desired {
            // Do not overwrite a real external change.
            all_dispatched = false;
            continue;
        }
        let compensation = compensation_placement(mutation);
        // Dispatch the inverse even if the current read already reports it.
        // A crash may have interrupted the previous synchronous call after the
        // journal crossed APPLYING but before its outcome was durable.
        if set_saved_placement(
            current.handle,
            mutation.original.window_instance_marker,
            compensation,
        )
        .is_err()
        {
            all_dispatched = false;
        }
    }
    all_dispatched
        && verify_compensations(
            dispatched,
            excluded_game_file_identities,
            include_own_process_for_test,
        )
}

#[cfg(windows)]
fn verify_mutations(
    mutations: &[PlannedMutation],
    direction: MutationDirection,
    excluded_game_file_identities: &[ProcessFileIdentity],
    include_own_process_for_test: bool,
) -> bool {
    use std::{
        thread::sleep,
        time::{Duration, Instant},
    };

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let verified = mutations.iter().all(|mutation| {
            let Ok(Some(current)) = refresh_candidate(
                &mutation.candidate,
                &mutation.desired,
                mutation.original.window_instance_marker,
                excluded_game_file_identities,
                include_own_process_for_test,
            ) else {
                return false;
            };
            if current.process_id != mutation.original.process_id
                || current.process_creation_time_100ns
                    != mutation.original.process_creation_time_100ns
                || current.window_instance_marker != mutation.original.window_instance_marker
            {
                return false;
            }
            match direction {
                MutationDirection::Apply => candidate_matches_saved(&current, &mutation.desired),
                MutationDirection::Rollback => {
                    candidate_matches_saved(&current, &mutation.original.saved)
                }
            }
        });
        if verified {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        sleep(Duration::from_millis(40));
    }
}

#[cfg(windows)]
fn verify_compensations(
    mutations: &[PlannedMutation],
    excluded_game_file_identities: &[ProcessFileIdentity],
    include_own_process_for_test: bool,
) -> bool {
    use std::{
        thread::sleep,
        time::{Duration, Instant},
    };

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let verified = mutations.iter().all(|mutation| {
            let Ok(Some(current)) = refresh_candidate(
                &mutation.candidate,
                &mutation.desired,
                mutation.original.window_instance_marker,
                excluded_game_file_identities,
                include_own_process_for_test,
            ) else {
                return false;
            };
            if current.process_id != mutation.original.process_id
                || current.process_creation_time_100ns
                    != mutation.original.process_creation_time_100ns
                || current.window_instance_marker != mutation.original.window_instance_marker
            {
                return false;
            }
            candidate_matches_compensation_source(&current, mutation)
        });
        if verified {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        sleep(Duration::from_millis(40));
    }
}

#[cfg(windows)]
fn compensation_placement(mutation: &PlannedMutation) -> SavedWindowPlacement {
    match mutation.source_state {
        OwnedPlacementState::Original => mutation.original.saved.placement,
        OwnedPlacementState::Desired => mutation.desired.placement,
    }
}

#[cfg(windows)]
fn candidate_matches_compensation_source(
    candidate: &WindowCandidate,
    mutation: &PlannedMutation,
) -> bool {
    match mutation.source_state {
        OwnedPlacementState::Original => {
            candidate_matches_saved(candidate, &mutation.original.saved)
        }
        OwnedPlacementState::Desired => candidate_matches_saved(candidate, &mutation.desired),
    }
}

#[cfg(windows)]
fn enumerate_candidates(
    excluded_game_file_identities: &[ProcessFileIdentity],
    include_own_process_for_test: bool,
) -> WindowsResult<CandidateReport> {
    use std::collections::HashMap;

    #[cfg(all(test, windows))]
    let include_own_process_for_test = include_own_process_for_test
        || ALLOW_OWN_WINDOW_CANDIDATES.load(std::sync::atomic::Ordering::SeqCst);

    let raw_windows = enumerate_visible_windows()?;
    let process_report = super::snapshot_process_identities()?;
    let identities = process_report
        .processes
        .iter()
        .map(|identity| (identity.process_id, identity))
        .collect::<HashMap<_, _>>();
    let own_process_id = std::process::id();
    let mut result = CandidateReport {
        candidates: Vec::new(),
        excluded_game_windows: 0,
        skipped_windows: 0,
    };

    for handle in raw_windows {
        match read_candidate(
            handle,
            &identities,
            excluded_game_file_identities,
            own_process_id,
            include_own_process_for_test,
        ) {
            CandidateRead::Included(candidate) => result.candidates.push(candidate),
            CandidateRead::ExcludedGame => {
                result.excluded_game_windows = result.excluded_game_windows.saturating_add(1);
            }
            CandidateRead::Skipped => {
                result.skipped_windows = result.skipped_windows.saturating_add(1);
            }
        }
    }
    Ok(result)
}

#[cfg(windows)]
enum CandidateRead {
    Included(WindowCandidate),
    ExcludedGame,
    Skipped,
}

#[cfg(windows)]
fn read_candidate(
    handle: isize,
    identities: &std::collections::HashMap<u32, &super::ProcessIdentity>,
    excluded_game_file_identities: &[ProcessFileIdentity],
    own_process_id: u32,
    include_own_process_for_test: bool,
) -> CandidateRead {
    use std::path::Path;
    use windows::Win32::{
        Foundation::{GetLastError, HWND, WIN32_ERROR},
        Graphics::{
            Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED},
            Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
        },
        UI::WindowsAndMessaging::{
            GetClassNameW, GetWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW,
            GetWindowThreadProcessId, GWL_EXSTYLE, GWL_STYLE, GW_OWNER,
        },
    };

    let window = HWND(handle as *mut core::ffi::c_void);
    let style = unsafe { GetWindowLongW(window, GWL_STYLE) as u32 };
    let extended_style = unsafe { GetWindowLongW(window, GWL_EXSTYLE) as u32 };
    if !is_standard_app_style(style, extended_style) {
        return CandidateRead::Skipped;
    }

    // A null owner is a normal GetWindow result represented as Err by windows-rs. Reset and read
    // last-error so a genuine API failure is still fail-closed.
    unsafe { windows::Win32::Foundation::SetLastError(WIN32_ERROR(0)) };
    if unsafe { GetWindow(window, GW_OWNER) }.is_ok() || unsafe { GetLastError() } != WIN32_ERROR(0)
    {
        return CandidateRead::Skipped;
    }

    let mut cloaked = 0u32;
    if unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    }
    .is_err()
        || cloaked != 0
    {
        return CandidateRead::Skipped;
    }

    let (placement, observed_rect) = match read_placement_and_rect(handle) {
        Ok(value) => value,
        Err(_) => return CandidateRead::Skipped,
    };
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    if monitor.0.is_null() {
        return CandidateRead::Skipped;
    }
    let mut monitor_info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut monitor_info) }.as_bool() {
        return CandidateRead::Skipped;
    }
    // A documented maximized standard application legitimately fills its
    // monitor and must remain capturable so showCmd=SW_SHOWMAXIMIZED can round
    // trip. Other monitor-filling windows are conservatively excluded.
    if exclude_monitor_filling_window(
        placement.show_cmd,
        observed_rect,
        rect_from_win32(monitor_info.rcMonitor),
    ) {
        return CandidateRead::Skipped;
    }

    let mut process_id = 0u32;
    if unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) } == 0
        || process_id == 0
        || (!include_own_process_for_test && process_id == own_process_id)
    {
        return CandidateRead::Skipped;
    }
    let Some(identity) = identities.get(&process_id) else {
        // The executable identity is unavailable or vanished: never fall back to PID/path/title.
        return CandidateRead::Skipped;
    };
    if !snapshot_process_instance_is_current(identity) {
        // The process snapshot can race PID reuse. Reject the stale identity
        // before reading a potentially private title or applying a game
        // exclusion decision to the wrong process instance.
        return CandidateRead::Skipped;
    }
    if excluded_game_file_identities.contains(&identity.file_identity) {
        // This check intentionally precedes GetWindowTextW.
        return CandidateRead::ExcludedGame;
    }

    let mut class_buffer = vec![0u16; MAX_WINDOW_CLASS_CHARS + 1];
    let class_length = unsafe { GetClassNameW(window, &mut class_buffer) };
    if class_length <= 0 || class_length as usize > MAX_WINDOW_CLASS_CHARS {
        return CandidateRead::Skipped;
    }
    class_buffer.truncate(class_length as usize);
    let Ok(class_name) = String::from_utf16(&class_buffer) else {
        return CandidateRead::Skipped;
    };
    if class_name.trim().is_empty() || class_name.chars().any(char::is_control) {
        return CandidateRead::Skipped;
    }

    let reported_title_length = unsafe { GetWindowTextLengthW(window) };
    if reported_title_length <= 0 || reported_title_length as usize > MAX_WINDOW_TITLE_CHARS {
        return CandidateRead::Skipped;
    }
    let mut title_buffer = vec![0u16; MAX_WINDOW_TITLE_CHARS + 1];
    let title_length = unsafe { GetWindowTextW(window, &mut title_buffer) };
    if title_length <= 0 || title_length as usize > MAX_WINDOW_TITLE_CHARS {
        return CandidateRead::Skipped;
    }
    title_buffer.truncate(title_length as usize);
    let Ok(title_text) = String::from_utf16(&title_buffer) else {
        return CandidateRead::Skipped;
    };
    if title_text.trim().is_empty() {
        return CandidateRead::Skipped;
    }
    let Some(title) = SensitiveWindowTitle::new(title_text) else {
        return CandidateRead::Skipped;
    };

    let Some(application_label) = Path::new(&identity.canonical_path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| {
            let length = name.chars().count();
            length > 0
                && length <= MAX_WINDOW_LABEL_CHARS
                && !name.chars().any(char::is_control)
        })
        .map(str::to_owned)
    else {
        return CandidateRead::Skipped;
    };

    let mut final_process_id = 0u32;
    if unsafe { GetWindowThreadProcessId(window, Some(&mut final_process_id)) } == 0
        || final_process_id != process_id
        || !snapshot_process_instance_is_current(identity)
    {
        // Bind the final candidate to the same HWND/PID/creation-time tuple
        // after class/title reads as well. A vanished or reused instance is
        // never persisted under the stale executable identity.
        return CandidateRead::Skipped;
    }

    CandidateRead::Included(WindowCandidate {
        handle,
        process_id,
        process_creation_time_100ns: identity.creation_time_100ns,
        window_instance_marker: 0,
        process_file_identity: identity.file_identity,
        application_label,
        class_name,
        title,
        placement,
        observed_rect,
    })
}

#[cfg(windows)]
fn snapshot_process_instance_is_current(identity: &super::ProcessIdentity) -> bool {
    matches!(
        super::process_instance_status(identity.process_id, identity.creation_time_100ns),
        Ok(super::ProcessInstanceStatus::Running)
    )
}

#[cfg(windows)]
fn attach_new_window_instance_marker(handle: isize) -> WindowsResult<u64> {
    use rand::{rngs::OsRng, RngCore};
    use windows::Win32::{
        Foundation::{HANDLE, HWND},
        UI::WindowsAndMessaging::SetPropW,
    };

    let existing = read_window_instance_marker(handle);
    if existing != 0 {
        return Ok(existing);
    }

    let mut marker_bytes = [0u8; 8];
    OsRng.try_fill_bytes(&mut marker_bytes).map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "generate private window instance marker",
            None,
        )
    })?;
    let mut marker = u64::from_le_bytes(marker_bytes) & (isize::MAX as u64);
    if marker == 0 {
        marker = 1;
    }
    let property_name = windows::core::HSTRING::from(WINDOW_INSTANCE_PROPERTY_NAME);
    let window = HWND(handle as *mut core::ffi::c_void);
    unsafe {
        SetPropW(
            window,
            &property_name,
            HANDLE(marker as usize as *mut core::ffi::c_void),
        )
    }
    .map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::AccessDenied,
            "attach private window instance marker",
            Some(i64::from(error.code().0)),
        )
    })?;
    if !window_instance_marker_matches(handle, marker) {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "verify private window instance marker",
            None,
        ));
    }
    Ok(marker)
}

#[cfg(windows)]
fn window_instance_marker_matches(handle: isize, marker: u64) -> bool {
    marker != 0 && read_window_instance_marker(handle) == marker
}

#[cfg(windows)]
fn read_window_instance_marker(handle: isize) -> u64 {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::GetPropW,
    };

    let property_name = windows::core::HSTRING::from(WINDOW_INSTANCE_PROPERTY_NAME);
    let value = unsafe {
        GetPropW(
            HWND(handle as *mut core::ffi::c_void),
            &property_name,
        )
    };
    value.0 as usize as u64
}

#[cfg(windows)]
fn bind_original_window_markers(
    candidates: &mut [WindowCandidate],
    originals: &[OriginalWindowPlacementEntry],
) {
    for candidate in candidates {
        candidate.window_instance_marker = 0;
        let mut matching = originals.iter().filter(|original| {
            candidate_key_matches_entry(candidate, &original.saved)
                && candidate.handle as i64 == original.window_handle_token
                && candidate.process_id == original.process_id
                && candidate.process_creation_time_100ns
                    == original.process_creation_time_100ns
                && window_instance_marker_matches(
                    candidate.handle,
                    original.window_instance_marker,
                )
        });
        if let (Some(original), None) = (matching.next(), matching.next()) {
            candidate.window_instance_marker = original.window_instance_marker;
        }
    }
}

#[cfg(windows)]
fn enumerate_visible_windows() -> WindowsResult<Vec<isize>> {
    use windows::Win32::{
        Foundation::{BOOL, HWND, LPARAM, TRUE},
        UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible},
    };

    struct Enumeration {
        handles: Vec<isize>,
        overflow: bool,
    }

    unsafe extern "system" fn callback(window: HWND, parameter: LPARAM) -> BOOL {
        let enumeration = &mut *(parameter.0 as *mut Enumeration);
        if !IsWindowVisible(window).as_bool() {
            return TRUE;
        }
        if enumeration.handles.len() == MAX_ENUMERATED_VISIBLE_WINDOWS {
            enumeration.overflow = true;
            return TRUE;
        }
        enumeration.handles.push(window.0 as isize);
        TRUE
    }

    let mut enumeration = Enumeration {
        handles: Vec::new(),
        overflow: false,
    };
    unsafe {
        EnumWindows(
            Some(callback),
            LPARAM(&mut enumeration as *mut Enumeration as isize),
        )
    }
    .map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "EnumWindows for window placement",
            Some(i64::from(error.code().0)),
        )
    })?;
    if enumeration.overflow {
        return Err(WindowsError::new(
            WindowsErrorKind::ResourceLimit,
            "bound visible window enumeration",
            None,
        ));
    }
    Ok(enumeration.handles)
}

#[cfg(windows)]
fn refresh_candidate(
    candidate: &WindowCandidate,
    desired: &SavedWindowPlacementEntry,
    window_instance_marker: u64,
    excluded_game_file_identities: &[ProcessFileIdentity],
    include_own_process_for_test: bool,
) -> WindowsResult<Option<WindowCandidate>> {
    use std::collections::HashMap;

    #[cfg(all(test, windows))]
    let include_own_process_for_test = include_own_process_for_test
        || ALLOW_OWN_WINDOW_CANDIDATES.load(std::sync::atomic::Ordering::SeqCst);

    if super::process_instance_status(candidate.process_id, candidate.process_creation_time_100ns)?
        != super::ProcessInstanceStatus::Running
    {
        return Ok(None);
    }
    // The running process instance has already been bound to its executable
    // file identity. Reuse that binding for this immediate HWND re-read rather
    // than taking another system-wide process snapshot for every target.
    let identity = super::ProcessIdentity {
        process_id: candidate.process_id,
        creation_time_100ns: candidate.process_creation_time_100ns,
        canonical_path: candidate.application_label.clone(),
        file_identity: candidate.process_file_identity,
        corroborated_by_wmi: false,
    };
    let identities = HashMap::from([(identity.process_id, &identity)]);
    let current = read_candidate(
        candidate.handle,
        &identities,
        excluded_game_file_identities,
        std::process::id(),
        include_own_process_for_test,
    );
    Ok(match current {
        CandidateRead::Included(mut value)
            if candidate_key_matches_entry(&value, desired)
                && value.process_id == candidate.process_id
                && value.process_creation_time_100ns == candidate.process_creation_time_100ns
                && window_instance_marker_matches(value.handle, window_instance_marker) =>
        {
            value.window_instance_marker = window_instance_marker;
            Some(value)
        }
        _ => None,
    })
}

#[cfg(windows)]
fn read_placement_and_rect(handle: isize) -> WindowsResult<(SavedWindowPlacement, WindowRect)> {
    use windows::Win32::{
        Foundation::{HWND, RECT},
        UI::WindowsAndMessaging::{GetWindowPlacement, GetWindowRect, WINDOWPLACEMENT},
    };

    let window = HWND(handle as *mut core::ffi::c_void);
    let mut placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    unsafe { GetWindowPlacement(window, &mut placement) }.map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "GetWindowPlacement",
            Some(i64::from(error.code().0)),
        )
    })?;
    let mut rect = RECT::default();
    unsafe { GetWindowRect(window, &mut rect) }.map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "GetWindowRect",
            Some(i64::from(error.code().0)),
        )
    })?;
    let saved = SavedWindowPlacement {
        flags: placement.flags.0,
        show_cmd: placement.showCmd,
        min_position: WindowPoint {
            x: placement.ptMinPosition.x,
            y: placement.ptMinPosition.y,
        },
        max_position: WindowPoint {
            x: placement.ptMaxPosition.x,
            y: placement.ptMaxPosition.y,
        },
        normal_position: rect_from_win32(placement.rcNormalPosition),
    };
    let observed_rect = rect_from_win32(rect);
    if !placement_is_valid(saved) || !rect_is_valid(observed_rect) {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate GetWindowPlacement result",
            None,
        ));
    }
    Ok((saved, observed_rect))
}

#[cfg(windows)]
fn set_saved_placement(
    handle: isize,
    window_instance_marker: u64,
    saved: SavedWindowPlacement,
) -> WindowsResult<()> {
    use windows::Win32::{
        Foundation::{GetLastError, HWND, LPARAM, POINT, RECT, WPARAM},
        UI::WindowsAndMessaging::{
            SendMessageTimeoutW, SetWindowPlacement, SEND_MESSAGE_TIMEOUT_FLAGS,
            SMTO_ABORTIFHUNG, SMTO_BLOCK, SMTO_ERRORONEXIT, WINDOWPLACEMENT,
            WINDOWPLACEMENT_FLAGS, WM_NULL,
        },
    };

    if !placement_is_valid(saved) {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate SetWindowPlacement input",
            None,
        ));
    }
    let window = HWND(handle as *mut core::ffi::c_void);
    let mut message_result = 0usize;
    unsafe { windows::Win32::Foundation::SetLastError(Default::default()) };
    let responsive = unsafe {
        SendMessageTimeoutW(
            window,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SEND_MESSAGE_TIMEOUT_FLAGS(
                SMTO_ABORTIFHUNG.0 | SMTO_BLOCK.0 | SMTO_ERRORONEXIT.0,
            ),
            WINDOW_RESPONSIVENESS_TIMEOUT_MS,
            Some(&mut message_result),
        )
    };
    if responsive.0 == 0 {
        let error = unsafe { GetLastError() };
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "window responsiveness probe before SetWindowPlacement",
            if error.0 == 0 {
                None
            } else {
                Some(i64::from(error.0))
            },
        ));
    }
    if !window_instance_marker_matches(handle, window_instance_marker) {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "revalidate exact window instance after responsiveness probe",
            None,
        ));
    }

    let placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        // Do not use WPF_ASYNCWINDOWPLACEMENT. An invisible queued write cannot
        // be classified after a crash and can race a third party's later
        // placement intent. A bounded WM_NULL liveness probe precedes this
        // synchronous documented API call; a post-probe hang fails by stopping
        // the transaction rather than leaving an unobservable queued write.
        flags: WINDOWPLACEMENT_FLAGS(saved.flags),
        showCmd: saved.show_cmd,
        ptMinPosition: POINT {
            x: saved.min_position.x,
            y: saved.min_position.y,
        },
        ptMaxPosition: POINT {
            x: saved.max_position.x,
            y: saved.max_position.y,
        },
        rcNormalPosition: RECT {
            left: saved.normal_position.left,
            top: saved.normal_position.top,
            right: saved.normal_position.right,
            bottom: saved.normal_position.bottom,
        },
    };
    unsafe { SetWindowPlacement(window, &placement) }.map_err(|error| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "SetWindowPlacement",
            Some(i64::from(error.code().0)),
        )
    })
}

#[cfg(windows)]
fn rect_from_win32(rect: windows::Win32::Foundation::RECT) -> WindowRect {
    WindowRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

fn observe_entries(
    desired: &[SavedWindowPlacementEntry],
    candidates: &[WindowCandidate],
    excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowLayoutObservation {
    let mut observation = WindowLayoutObservation {
        saved_window_count: bounded_count(desired.len()),
        matched_window_count: 0,
        positioned_window_count: 0,
        issues: Vec::new(),
        geometry_fingerprint: geometry_fingerprint(
            desired,
            candidates,
            excluded_game_file_identities,
            None,
        ),
    };
    for (desired_index, entry) in desired.iter().enumerate() {
        if excluded_game_file_identities.contains(&entry.process_file_identity) {
            observation
                .issues
                .push(issue(entry, WindowLayoutIssueReason::GameExcluded));
            continue;
        }
        match resolve_match(desired_index, desired, candidates) {
            MatchResolution::Missing => observation
                .issues
                .push(issue(entry, WindowLayoutIssueReason::NotRunning)),
            MatchResolution::Ambiguous => observation
                .issues
                .push(issue(entry, WindowLayoutIssueReason::AmbiguousMatch)),
            MatchResolution::Unique(candidate_index) => {
                observation.matched_window_count =
                    observation.matched_window_count.saturating_add(1);
                let candidate = &candidates[candidate_index];
                if placement_matches(entry.placement, candidate.placement)
                    && rect_matches(entry.observed_rect, candidate.observed_rect)
                {
                    observation.positioned_window_count =
                        observation.positioned_window_count.saturating_add(1);
                } else {
                    observation
                        .issues
                        .push(issue(entry, WindowLayoutIssueReason::VerificationMismatch));
                }
            }
        }
    }
    observation
}

fn observe_original_entries(
    originals: &[OriginalWindowPlacementEntry],
    candidates: &[WindowCandidate],
    excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowLayoutObservation {
    let saved = originals
        .iter()
        .map(|entry| entry.saved.clone())
        .collect::<Vec<_>>();
    let mut observation = WindowLayoutObservation {
        saved_window_count: bounded_count(originals.len()),
        matched_window_count: 0,
        positioned_window_count: 0,
        issues: Vec::new(),
        geometry_fingerprint: geometry_fingerprint(
            &saved,
            candidates,
            excluded_game_file_identities,
            Some(originals),
        ),
    };
    for original in originals {
        let entry = &original.saved;
        if excluded_game_file_identities.contains(&entry.process_file_identity) {
            observation
                .issues
                .push(issue(entry, WindowLayoutIssueReason::GameExcluded));
            continue;
        }
        let Some(candidate) = resolve_original_candidate(original, candidates) else {
            observation
                .issues
                .push(issue(entry, WindowLayoutIssueReason::NotRunning));
            continue;
        };
        observation.matched_window_count = observation.matched_window_count.saturating_add(1);
        if candidate_matches_saved(candidate, entry) {
            observation.positioned_window_count =
                observation.positioned_window_count.saturating_add(1);
        } else {
            observation
                .issues
                .push(issue(entry, WindowLayoutIssueReason::VerificationMismatch));
        }
    }
    observation
}

fn classify_transaction_entries(
    desired: &WindowLayoutSnapshot,
    originals: &[OriginalWindowPlacementEntry],
    candidates: &[WindowCandidate],
    excluded_game_file_identities: &[ProcessFileIdentity],
) -> WindowLayoutTransactionState {
    if originals.is_empty() {
        return WindowLayoutTransactionState::Original;
    }
    let mut original_count = 0usize;
    let mut desired_count = 0usize;
    let mut neutral_count = 0usize;
    for original in originals {
        let Some(desired_entry) = desired
            .entries
            .iter()
            .find(|entry| entry.entry_id == original.saved.entry_id)
        else {
            return WindowLayoutTransactionState::Third;
        };
        if excluded_game_file_identities.contains(&desired_entry.process_file_identity) {
            return WindowLayoutTransactionState::Third;
        }
        let Some(candidate) = resolve_original_candidate(original, candidates) else {
            return WindowLayoutTransactionState::Third;
        };
        let at_original = candidate_matches_saved(candidate, &original.saved);
        let at_desired = candidate_matches_saved(candidate, desired_entry);
        match (at_original, at_desired) {
            (true, true) => neutral_count += 1,
            (true, false) => original_count += 1,
            (false, true) => desired_count += 1,
            (false, false) => return WindowLayoutTransactionState::Third,
        }
    }
    match (original_count, desired_count) {
        (0, 0) if neutral_count > 0 => WindowLayoutTransactionState::Original,
        (_, 0) => WindowLayoutTransactionState::Original,
        (0, _) => WindowLayoutTransactionState::Desired,
        _ => WindowLayoutTransactionState::MixedOwned,
    }
}

fn resolve_original_candidate<'a>(
    original: &OriginalWindowPlacementEntry,
    candidates: &'a [WindowCandidate],
) -> Option<&'a WindowCandidate> {
    let mut matching = candidates.iter().filter(|candidate| {
        candidate_key_matches_entry(candidate, &original.saved)
            && candidate.handle as i64 == original.window_handle_token
            && candidate.process_id == original.process_id
            && candidate.process_creation_time_100ns == original.process_creation_time_100ns
            && candidate.window_instance_marker == original.window_instance_marker
    });
    match (matching.next(), matching.next()) {
        (Some(candidate), None) => Some(candidate),
        _ => None,
    }
}

fn candidate_matches_saved(candidate: &WindowCandidate, saved: &SavedWindowPlacementEntry) -> bool {
    placement_matches(saved.placement, candidate.placement)
        && rect_matches(saved.observed_rect, candidate.observed_rect)
}

/// Hash exact geometry without feeding a title, class name, or executable path
/// into the digest. `entry_id` binds each opaque target; matched candidate
/// instance and placement bytes make every external move visible.
fn geometry_fingerprint(
    desired: &[SavedWindowPlacementEntry],
    candidates: &[WindowCandidate],
    excluded_game_file_identities: &[ProcessFileIdentity],
    originals: Option<&[OriginalWindowPlacementEntry]>,
) -> String {
    let mut bytes = Vec::with_capacity(desired.len().saturating_mul(128));
    bytes.extend_from_slice(&(desired.len() as u64).to_le_bytes());
    for entry in desired {
        bytes.extend_from_slice(entry.entry_id.as_bytes());
        if excluded_game_file_identities.contains(&entry.process_file_identity) {
            bytes.push(1);
            continue;
        }
        let required_instance = originals.and_then(|values| {
            values
                .iter()
                .find(|value| value.saved.entry_id == entry.entry_id)
                .map(|value| {
                    (
                        value.process_id,
                        value.process_creation_time_100ns,
                        value.window_handle_token,
                        value.window_instance_marker,
                    )
                })
        });
        let mut matching = candidates
            .iter()
            .filter(|candidate| {
                candidate_key_matches_entry(candidate, entry)
                    && required_instance.is_none_or(
                        |(process_id, creation_time, handle, marker)| {
                        candidate.process_id == process_id
                            && candidate.process_creation_time_100ns == creation_time
                            && candidate.handle as i64 == handle
                            && candidate.window_instance_marker == marker
                    },
                    )
            })
            .collect::<Vec<_>>();
        matching.sort_by_key(|candidate| {
            (
                candidate.process_id,
                candidate.process_creation_time_100ns,
                candidate.handle,
            )
        });
        bytes.push(if matching.is_empty() {
            2
        } else if matching.len() == 1 {
            3
        } else {
            4
        });
        bytes.extend_from_slice(&(matching.len() as u64).to_le_bytes());
        for candidate in matching {
            bytes.extend_from_slice(&candidate.process_id.to_le_bytes());
            bytes.extend_from_slice(&candidate.process_creation_time_100ns.to_le_bytes());
            bytes.extend_from_slice(&(candidate.handle as i64).to_le_bytes());
            if originals.is_some() {
                bytes.extend_from_slice(&candidate.window_instance_marker.to_le_bytes());
            }
            append_placement_bytes(&mut bytes, candidate.placement);
            append_rect_bytes(&mut bytes, candidate.observed_rect);
        }
    }
    Fingerprint::of_bytes(&bytes).to_hex()
}

fn append_placement_bytes(bytes: &mut Vec<u8>, placement: SavedWindowPlacement) {
    bytes.extend_from_slice(&placement.flags.to_le_bytes());
    bytes.extend_from_slice(&placement.show_cmd.to_le_bytes());
    bytes.extend_from_slice(&placement.min_position.x.to_le_bytes());
    bytes.extend_from_slice(&placement.min_position.y.to_le_bytes());
    bytes.extend_from_slice(&placement.max_position.x.to_le_bytes());
    bytes.extend_from_slice(&placement.max_position.y.to_le_bytes());
    append_rect_bytes(bytes, placement.normal_position);
}

fn append_rect_bytes(bytes: &mut Vec<u8>, rect: WindowRect) {
    bytes.extend_from_slice(&rect.left.to_le_bytes());
    bytes.extend_from_slice(&rect.top.to_le_bytes());
    bytes.extend_from_slice(&rect.right.to_le_bytes());
    bytes.extend_from_slice(&rect.bottom.to_le_bytes());
}

fn resolve_match(
    desired_index: usize,
    desired: &[SavedWindowPlacementEntry],
    candidates: &[WindowCandidate],
) -> MatchResolution {
    let entry = &desired[desired_index];
    if desired
        .iter()
        .enumerate()
        .any(|(index, other)| index != desired_index && entry_keys_match(entry, other))
    {
        return MatchResolution::Ambiguous;
    }
    let mut matching = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate_key_matches_entry(candidate, entry))
        .map(|(index, _)| index);
    match (matching.next(), matching.next()) {
        (None, _) => MatchResolution::Missing,
        (Some(_), Some(_)) => MatchResolution::Ambiguous,
        (Some(index), None) => MatchResolution::Unique(index),
    }
}

fn entry_keys_match(left: &SavedWindowPlacementEntry, right: &SavedWindowPlacementEntry) -> bool {
    left.process_file_identity == right.process_file_identity
        && left.class_name == right.class_name
        && left.title == right.title
}

fn candidate_key_matches_entry(
    candidate: &WindowCandidate,
    entry: &SavedWindowPlacementEntry,
) -> bool {
    candidate.process_file_identity == entry.process_file_identity
        && candidate.class_name == entry.class_name
        && candidate.title == entry.title
}

fn entry_from_candidate(
    candidate: &WindowCandidate,
    entry_id: uuid::Uuid,
    application_label: String,
) -> SavedWindowPlacementEntry {
    SavedWindowPlacementEntry {
        entry_id,
        process_file_identity: candidate.process_file_identity,
        application_label,
        class_name: candidate.class_name.clone(),
        title: candidate.title.clone(),
        placement: candidate.placement,
        observed_rect: candidate.observed_rect,
    }
}

#[cfg(all(test, windows))]
pub(crate) fn capture_window_entry_for_test(
    handle: isize,
    application_label: &str,
) -> WindowsResult<SavedWindowPlacementEntry> {
    let report = enumerate_candidates(&[], true)?;
    let candidate = report
        .candidates
        .iter()
        .find(|candidate| candidate.handle == handle)
        .ok_or_else(|| {
            WindowsError::new(
                WindowsErrorKind::InvalidData,
                "capture owned test window placement",
                None,
            )
        })?;
    Ok(entry_from_candidate(
        candidate,
        uuid::Uuid::new_v4(),
        application_label.to_owned(),
    ))
}

#[cfg(all(test, windows))]
pub(crate) fn read_window_placement_for_test(
    handle: isize,
) -> WindowsResult<(SavedWindowPlacement, WindowRect)> {
    read_placement_and_rect(handle)
}

fn issue(entry: &SavedWindowPlacementEntry, reason: WindowLayoutIssueReason) -> WindowLayoutIssue {
    WindowLayoutIssue {
        target: entry.application_label.clone(),
        reason,
    }
}

fn numbered_application_label(base: &str, ordinal: u32) -> String {
    if ordinal <= 1 {
        return base.to_owned();
    }
    let suffix = format!("（{ordinal}）");
    let available = MAX_WINDOW_LABEL_CHARS.saturating_sub(suffix.chars().count());
    let mut result = base.chars().take(available).collect::<String>();
    result.push_str(&suffix);
    result
}

fn is_standard_app_style(style: u32, extended_style: u32) -> bool {
    style & STYLE_CAPTION == STYLE_CAPTION
        && style & STYLE_SYSTEM_MENU == STYLE_SYSTEM_MENU
        && style & (STYLE_POPUP | STYLE_CHILD | STYLE_DISABLED) == 0
        && extended_style & EX_STYLE_TOOL_WINDOW == 0
}

fn is_fullscreen(window: WindowRect, monitor: WindowRect) -> bool {
    window.left <= monitor.left
        && window.top <= monitor.top
        && window.right >= monitor.right
        && window.bottom >= monitor.bottom
}

fn exclude_monitor_filling_window(
    show_command: u32,
    window: WindowRect,
    monitor: WindowRect,
) -> bool {
    show_command != 3 && is_fullscreen(window, monitor)
}

fn placement_is_valid(placement: SavedWindowPlacement) -> bool {
    placement.flags & !DURABLE_PLACEMENT_FLAGS == 0
        && valid_show_command(placement.show_cmd)
        && point_is_valid(placement.min_position)
        && point_is_valid(placement.max_position)
        && rect_is_valid(placement.normal_position)
}

fn valid_show_command(show_command: u32) -> bool {
    matches!(show_command, 1..=11)
}

fn point_is_valid(point: WindowPoint) -> bool {
    coordinate_is_valid(point.x) && coordinate_is_valid(point.y)
}

fn rect_is_valid(rect: WindowRect) -> bool {
    coordinate_is_valid(rect.left)
        && coordinate_is_valid(rect.top)
        && coordinate_is_valid(rect.right)
        && coordinate_is_valid(rect.bottom)
        && rect.right > rect.left
        && rect.bottom > rect.top
}

fn coordinate_is_valid(value: i32) -> bool {
    (-MAX_ABSOLUTE_COORDINATE..=MAX_ABSOLUTE_COORDINATE).contains(&value)
}

fn placement_matches(expected: SavedWindowPlacement, actual: SavedWindowPlacement) -> bool {
    expected == actual
}

fn rect_matches(expected: WindowRect, actual: WindowRect) -> bool {
    const PIXEL_TOLERANCE: i32 = 2;
    coordinate_difference(expected.left, actual.left) <= PIXEL_TOLERANCE
        && coordinate_difference(expected.top, actual.top) <= PIXEL_TOLERANCE
        && coordinate_difference(expected.right, actual.right) <= PIXEL_TOLERANCE
        && coordinate_difference(expected.bottom, actual.bottom) <= PIXEL_TOLERANCE
}

fn coordinate_difference(left: i32, right: i32) -> i32 {
    left.saturating_sub(right)
        .abs()
        .max(right.saturating_sub(left).abs())
}

fn bounded_count(length: usize) -> u32 {
    u32::try_from(length).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn identity(value: u8) -> ProcessFileIdentity {
        ProcessFileIdentity {
            volume_serial_number: u64::from(value),
            file_id: [value; 16],
        }
    }

    fn marker(value: u8) -> u64 {
        u64::from(value)
    }

    fn placement(show_cmd: u32, left: i32) -> SavedWindowPlacement {
        SavedWindowPlacement {
            flags: 0,
            show_cmd,
            min_position: WindowPoint { x: -1, y: -1 },
            max_position: WindowPoint { x: -1, y: -1 },
            normal_position: WindowRect {
                left,
                top: 100,
                right: left + 400,
                bottom: 400,
            },
        }
    }

    fn entry(file_identity: ProcessFileIdentity, title: &str) -> SavedWindowPlacementEntry {
        SavedWindowPlacementEntry {
            entry_id: Uuid::new_v4(),
            process_file_identity: file_identity,
            application_label: "safe-app.exe".to_owned(),
            class_name: "TotonoePlacementTest".to_owned(),
            title: SensitiveWindowTitle::new(title.to_owned()).expect("bounded fixed title"),
            placement: placement(1, 100),
            observed_rect: WindowRect {
                left: 100,
                top: 100,
                right: 500,
                bottom: 400,
            },
        }
    }

    fn candidate(file_identity: ProcessFileIdentity, title: &str) -> WindowCandidate {
        WindowCandidate {
            handle: 1,
            process_id: 10,
            process_creation_time_100ns: 20,
            window_instance_marker: marker(1),
            process_file_identity: file_identity,
            application_label: "safe-app.exe".to_owned(),
            class_name: "TotonoePlacementTest".to_owned(),
            title: SensitiveWindowTitle::new(title.to_owned()).expect("bounded fixed title"),
            placement: placement(1, 100),
            observed_rect: WindowRect {
                left: 100,
                top: 100,
                right: 500,
                bottom: 400,
            },
        }
    }

    #[test]
    fn private_title_is_redacted_and_never_used_as_an_issue_target() {
        let private = "private document title 38b91";
        let saved = entry(identity(1), private);
        assert!(!format!("{:?}", saved.title).contains(private));
        let observed = observe_entries(&[saved], &[], &[]);
        assert_eq!(observed.issues.len(), 1);
        assert_eq!(observed.issues[0].target, "safe-app.exe");
        assert!(!format!("{observed:?}").contains(private));
    }

    #[test]
    fn style_filter_requires_a_standard_enabled_non_popup_window() {
        let standard = STYLE_CAPTION | STYLE_SYSTEM_MENU;
        assert!(is_standard_app_style(standard, 0));
        assert!(!is_standard_app_style(standard | STYLE_DISABLED, 0));
        assert!(!is_standard_app_style(standard | STYLE_CHILD, 0));
        assert!(!is_standard_app_style(standard | STYLE_POPUP, 0));
        assert!(!is_standard_app_style(standard, EX_STYLE_TOOL_WINDOW));
        assert!(!is_standard_app_style(STYLE_CAPTION, 0));
    }

    #[test]
    fn fullscreen_rect_is_excluded_but_a_normal_rect_is_not() {
        let monitor = WindowRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        assert!(is_fullscreen(monitor, monitor));
        assert!(is_fullscreen(
            WindowRect {
                left: -1,
                top: -1,
                right: 1921,
                bottom: 1081,
            },
            monitor
        ));
        assert!(!is_fullscreen(
            WindowRect {
                left: 100,
                top: 100,
                right: 900,
                bottom: 700,
            },
            monitor
        ));
        assert!(
            !exclude_monitor_filling_window(3, monitor, monitor),
            "a standard maximized window must remain eligible for showCmd round-trip"
        );
        assert!(exclude_monitor_filling_window(1, monitor, monitor));
    }

    #[test]
    fn show_command_validation_preserves_normal_minimized_and_maximized_states() {
        assert!(placement_is_valid(placement(1, 100)));
        assert!(placement_is_valid(placement(2, 100)));
        assert!(placement_is_valid(placement(3, 100)));
        assert!(placement_is_valid(placement(11, 100)));
        assert!(!placement_is_valid(placement(0, 100)));
        assert!(!placement_is_valid(placement(12, 100)));
    }

    #[test]
    fn exact_match_reports_missing_and_ambiguous_without_guessing() {
        let saved = entry(identity(1), "same fixed title");
        assert_eq!(
            resolve_match(0, std::slice::from_ref(&saved), &[]),
            MatchResolution::Missing
        );
        let one = candidate(identity(1), "same fixed title");
        assert_eq!(
            resolve_match(0, std::slice::from_ref(&saved), std::slice::from_ref(&one)),
            MatchResolution::Unique(0)
        );
        assert_eq!(
            resolve_match(0, std::slice::from_ref(&saved), &[one.clone(), one]),
            MatchResolution::Ambiguous
        );

        let duplicate_saved = saved.clone();
        assert_eq!(
            resolve_match(0, &[saved, duplicate_saved], &[]),
            MatchResolution::Ambiguous
        );
    }

    #[test]
    fn game_exclusion_wins_before_any_missing_match_diagnostic() {
        let saved = entry(identity(7), "never formatted");
        let observed = observe_entries(
            std::slice::from_ref(&saved),
            &[],
            &[saved.process_file_identity],
        );
        assert_eq!(
            observed.issues[0].reason,
            WindowLayoutIssueReason::GameExcluded
        );
    }

    #[test]
    fn geometry_fingerprint_changes_for_an_external_move_with_same_match_counts() {
        let saved = entry(identity(3), "fixed geometry title");
        let at_saved = candidate(identity(3), "fixed geometry title");
        let first = observe_entries(
            std::slice::from_ref(&saved),
            std::slice::from_ref(&at_saved),
            &[],
        );
        let mut moved = at_saved;
        moved.placement.normal_position.left += 80;
        moved.placement.normal_position.right += 80;
        moved.observed_rect.left += 80;
        moved.observed_rect.right += 80;
        let second = observe_entries(
            std::slice::from_ref(&saved),
            std::slice::from_ref(&moved),
            &[],
        );
        assert_eq!(first.matched_window_count, second.matched_window_count);
        assert_ne!(first.geometry_fingerprint, second.geometry_fingerprint);
    }

    #[test]
    fn original_observation_rejects_a_replacement_process_with_the_same_window_key() {
        let saved = entry(identity(4), "fixed replacement title");
        let original = OriginalWindowPlacementEntry {
            saved: saved.clone(),
            window_handle_token: 1,
            process_id: 10,
            process_creation_time_100ns: 20,
            window_instance_marker: marker(1),
        };
        let mut replacement = candidate(identity(4), "fixed replacement title");
        replacement.process_id = 11;
        replacement.process_creation_time_100ns = 21;
        let observed = observe_original_entries(&[original], &[replacement], &[]);
        assert_eq!(observed.matched_window_count, 0);
        assert_eq!(
            observed.issues[0].reason,
            WindowLayoutIssueReason::NotRunning
        );
    }

    #[test]
    fn per_entry_classification_recognizes_only_original_desired_mixes() {
        let first_desired = entry(identity(5), "first fixed title");
        let mut second_desired = entry(identity(6), "second fixed title");
        second_desired.placement = placement(1, 700);
        second_desired.observed_rect = second_desired.placement.normal_position;
        let desired = WindowLayoutSnapshot {
            snapshot_id: Uuid::new_v4(),
            captured_at_unix_ms: 1,
            entries: vec![first_desired.clone(), second_desired.clone()],
            excluded_game_windows: 0,
            skipped_windows: 0,
        };
        let mut first_original_entry = first_desired.clone();
        first_original_entry.placement = placement(1, 300);
        first_original_entry.observed_rect = first_original_entry.placement.normal_position;
        let first_original = OriginalWindowPlacementEntry {
            saved: first_original_entry.clone(),
            window_handle_token: 1,
            process_id: 10,
            process_creation_time_100ns: 20,
            window_instance_marker: marker(1),
        };
        let mut second_original_entry = second_desired.clone();
        second_original_entry.placement = placement(1, 900);
        second_original_entry.observed_rect = second_original_entry.placement.normal_position;
        let second_original = OriginalWindowPlacementEntry {
            saved: second_original_entry.clone(),
            window_handle_token: 2,
            process_id: 12,
            process_creation_time_100ns: 22,
            window_instance_marker: marker(2),
        };
        let mut first_candidate = candidate(identity(5), "first fixed title");
        first_candidate.placement = first_original_entry.placement;
        first_candidate.observed_rect = first_original_entry.observed_rect;
        let mut second_candidate = candidate(identity(6), "second fixed title");
        second_candidate.handle = 2;
        second_candidate.process_id = 12;
        second_candidate.process_creation_time_100ns = 22;
        second_candidate.window_instance_marker = marker(2);
        second_candidate.placement = second_desired.placement;
        second_candidate.observed_rect = second_desired.observed_rect;

        assert_eq!(
            classify_transaction_entries(
                &desired,
                &[first_original, second_original],
                &[first_candidate, second_candidate],
                &[],
            ),
            WindowLayoutTransactionState::MixedOwned
        );
    }

    #[cfg(windows)]
    #[test]
    fn rollback_fence_compensation_returns_each_target_to_its_own_source_state() {
        let mut desired = entry(identity(8), "fixed compensation title");
        desired.placement = placement(1, 700);
        desired.observed_rect = desired.placement.normal_position;
        let mut original_saved = desired.clone();
        original_saved.placement = placement(1, 300);
        original_saved.observed_rect = original_saved.placement.normal_position;
        let original = OriginalWindowPlacementEntry {
            saved: original_saved.clone(),
            window_handle_token: 1,
            process_id: 10,
            process_creation_time_100ns: 20,
            window_instance_marker: marker(1),
        };
        let mut original_candidate = candidate(identity(8), "fixed compensation title");
        original_candidate.placement = original_saved.placement;
        original_candidate.observed_rect = original_saved.observed_rect;
        let mut mutation = PlannedMutation {
            desired: desired.clone(),
            original,
            candidate: original_candidate.clone(),
            source_state: OwnedPlacementState::Original,
        };

        assert_eq!(
            compensation_placement(&mutation),
            original_saved.placement
        );
        assert!(candidate_matches_compensation_source(
            &original_candidate,
            &mutation
        ));
        assert_ne!(compensation_placement(&mutation), desired.placement);

        mutation.source_state = OwnedPlacementState::Desired;
        let mut desired_candidate = original_candidate;
        desired_candidate.placement = desired.placement;
        desired_candidate.observed_rect = desired.observed_rect;
        assert_eq!(compensation_placement(&mutation), desired.placement);
        assert!(candidate_matches_compensation_source(
            &desired_candidate,
            &mutation
        ));
    }

    #[cfg(windows)]
    struct OwnedTestWindow(windows::Win32::Foundation::HWND);

    #[cfg(windows)]
    impl OwnedTestWindow {
        fn create(title: &windows::core::HSTRING) -> Self {
            use windows::Win32::{
                Foundation::{HINSTANCE, HWND},
                UI::WindowsAndMessaging::{
                    CreateWindowExW, HMENU, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
                },
            };
            let window = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    windows::core::w!("STATIC"),
                    title,
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    120,
                    140,
                    520,
                    360,
                    HWND::default(),
                    HMENU::default(),
                    HINSTANCE::default(),
                    None,
                )
            }
            .expect("create an owned placement smoke window");
            Self(window)
        }

        fn handle(&self) -> isize {
            self.0 .0 as isize
        }
    }

    #[cfg(windows)]
    impl Drop for OwnedTestWindow {
        fn drop(&mut self) {
            let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.0) };
        }
    }

    #[cfg(windows)]
    #[test]
    fn destroyed_window_marker_is_not_reattached_during_second_capture() {
        let _own_window_scope = allow_own_window_candidates_for_test();
        let private_title =
            windows::core::HSTRING::from(format!("totonoe-marker-reuse-{}", Uuid::new_v4()));
        let first = OwnedTestWindow::create(&private_title);
        let first_candidate = enumerate_candidates(&[], false)
            .expect("enumerate first owned marker window")
            .candidates
            .into_iter()
            .find(|candidate| candidate.handle == first.handle())
            .expect("first owned marker window is eligible");
        let desired_entry =
            entry_from_candidate(&first_candidate, Uuid::new_v4(), "marker-test.exe".to_owned());
        let desired = WindowLayoutSnapshot {
            snapshot_id: Uuid::new_v4(),
            captured_at_unix_ms: 1,
            entries: vec![desired_entry],
            excluded_game_windows: 0,
            skipped_windows: 0,
        };
        let mut originals =
            capture_window_layout_originals(&desired, &[]).expect("mark the first exact HWND");
        assert_eq!(originals.len(), 1);
        let marker = originals[0].window_instance_marker;
        assert_ne!(marker, 0);
        assert_eq!(
            attach_new_window_instance_marker(first.handle())
                .expect("reuse one bounded property for the same live HWND"),
            marker
        );
        drop(first);

        let replacement = OwnedTestWindow::create(&private_title);
        assert_eq!(
            read_window_instance_marker(replacement.handle()),
            0,
            "DestroyWindow must remove the per-window property"
        );
        // Simulate the numeric HWND slot being reused while PID, process
        // creation time, executable identity, class, and title all remain the
        // same. The verification-only pass must not attach the old marker to
        // this replacement.
        originals[0].window_handle_token = replacement.handle() as i64;
        assert!(
            !verify_captured_window_layout_originals(&desired, &originals, &[])
                .expect("verify replacement without writing a marker")
        );
        assert_eq!(read_window_instance_marker(replacement.handle()), 0);
    }

    #[cfg(windows)]
    #[test]
    fn stale_snapshot_creation_time_is_rejected_before_candidate_capture() {
        use std::collections::HashMap;

        let private_title =
            windows::core::HSTRING::from(format!("totonoe-stale-instance-{}", Uuid::new_v4()));
        let owned = OwnedTestWindow::create(&private_title);
        let process_id = std::process::id();
        let report = super::super::snapshot_process_identities()
            .expect("take a real process identity snapshot");
        let current = report
            .processes
            .into_iter()
            .find(|identity| identity.process_id == process_id)
            .expect("snapshot contains the test process");
        let stale = super::super::ProcessIdentity {
            creation_time_100ns: current.creation_time_100ns.wrapping_add(1),
            ..current
        };
        let identities = HashMap::from([(process_id, &stale)]);

        assert!(matches!(
            read_candidate(owned.handle(), &identities, &[], process_id, true),
            CandidateRead::Skipped
        ));
    }

    /// Creates and mutates only a window owned by this test process. No existing user window is
    /// selected or moved. Output contains geometry/state only, never the matching title.
    #[cfg(windows)]
    #[test]
    #[ignore = "real-machine save, move, and SetWindowPlacement restoration smoke"]
    fn owned_window_save_move_restore_has_external_rect_and_state_evidence() {
        use std::{thread::sleep, time::Duration};
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER},
        };

        let private_title =
            windows::core::HSTRING::from(format!("totonoe-owned-placement-{}", Uuid::new_v4()));
        let owned = OwnedTestWindow::create(&private_title);
        sleep(Duration::from_millis(150));

        let report = enumerate_candidates(&[], true).expect("enumerate owned test window");
        let candidate = report
            .candidates
            .into_iter()
            .find(|candidate| candidate.handle == owned.handle())
            .expect("owned test window is eligible");
        let saved_entry =
            entry_from_candidate(&candidate, Uuid::new_v4(), "owned-smoke.exe".to_owned());
        let snapshot = WindowLayoutSnapshot {
            snapshot_id: Uuid::new_v4(),
            captured_at_unix_ms: 1,
            entries: vec![saved_entry],
            excluded_game_windows: 0,
            skipped_windows: 0,
        };
        let directory = tempfile::tempdir().expect("private temporary layout directory");
        let store = crate::window_layout::WindowLayoutStore::open(
            directory.path().join("window-layout.json"),
        )
        .expect("open private layout store");
        store
            .replace(snapshot.clone())
            .expect("persist owned test window placement");
        let persisted = store
            .snapshot(snapshot.snapshot_id)
            .expect("read persisted owned placement");
        let saved_entry = persisted
            .entries
            .into_iter()
            .next()
            .expect("persisted owned entry");
        let before_rect = saved_entry.observed_rect;
        let before_show_cmd = saved_entry.placement.show_cmd;

        unsafe {
            SetWindowPos(
                HWND(owned.handle() as *mut core::ffi::c_void),
                HWND::default(),
                before_rect.left + 180,
                before_rect.top + 130,
                before_rect.width(),
                before_rect.height(),
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
        .expect("move owned test window");
        sleep(Duration::from_millis(120));
        let (_, moved_rect) =
            read_placement_and_rect(owned.handle()).expect("observe moved test window");
        assert!(
            !rect_matches(before_rect, moved_rect),
            "test window really moved"
        );

        let moved_report = enumerate_candidates(&[], true).expect("enumerate moved owned window");
        let moved_candidate = moved_report
            .candidates
            .into_iter()
            .find(|candidate| candidate.handle == owned.handle())
            .expect("moved owned window remains eligible");
        let window_instance_marker =
            attach_new_window_instance_marker(moved_candidate.handle)
                .expect("mark the exact owned test window instance");
        let original = OriginalWindowPlacementEntry {
            saved: SavedWindowPlacementEntry {
                entry_id: saved_entry.entry_id,
                process_file_identity: saved_entry.process_file_identity,
                application_label: saved_entry.application_label.clone(),
                class_name: saved_entry.class_name.clone(),
                title: saved_entry.title.clone(),
                placement: moved_candidate.placement,
                observed_rect: moved_candidate.observed_rect,
            },
            window_handle_token: moved_candidate.handle as i64,
            process_id: moved_candidate.process_id,
            process_creation_time_100ns: moved_candidate.process_creation_time_100ns,
            window_instance_marker,
        };
        let restored =
            mutate_transaction(&snapshot, &[original], &[], MutationDirection::Apply, true)
                .expect("restore only the owned test window");
        assert_eq!(restored.matched_window_count, 1);
        assert_eq!(restored.positioned_window_count, 1);
        assert!(restored.issues.is_empty());
        let (after_placement, after_rect) =
            read_placement_and_rect(owned.handle()).expect("observe restored test window");
        assert!(rect_matches(before_rect, after_rect));
        assert_eq!(after_placement.show_cmd, before_show_cmd);
        println!(
            "EVIDENCE window_placement: saved=({},{} {}x{} show={}), moved=({},{}), restored=({},{} {}x{} show={})",
            before_rect.left,
            before_rect.top,
            before_rect.width(),
            before_rect.height(),
            before_show_cmd,
            moved_rect.left,
            moved_rect.top,
            after_rect.left,
            after_rect.top,
            after_rect.width(),
            after_rect.height(),
            after_placement.show_cmd
        );
    }
}
