//! Explicit restoration of one privately stored top-level window layout.
//!
//! The save operation lives outside the Action pipeline because capture is a
//! user command, not an OS mutation. Restoration is a normal Persistent Action:
//! it records every currently matched placement, applies the saved placement,
//! verifies it, and can restore the exact pre-apply placements.

use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, RollbackEvidence,
        TroubleshootingStep, ValidationReport, Verification, WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, Fingerprint},
    window_layout::{WindowLayoutBackup, WindowLayoutInvocation, WindowLayoutObservation},
    windows::{
        capture_window_layout_originals, classify_window_layout_transaction,
        observe_original_window_placements, observe_window_layout, restore_window_layout,
        restore_window_placement_entries, verify_captured_window_layout_originals,
        WindowLayoutTransactionState, WindowsErrorKind,
    },
};

use super::common::{
    evidence, fingerprint_state, map_windows_error, validate_backup, validate_backup_for_apply,
    validate_base,
};

pub struct WindowLayoutAction;
pub static WINDOW_LAYOUT_ACTION: WindowLayoutAction = WindowLayoutAction;

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::SetupWindowLayout,
    name: "現在のウィンドウ配置を保存・復元する",
    description: "明示的に保存した通常のアプリウィンドウだけを、公開Win32 APIで位置と最小化・最大化状態まで復元します。アプリは起動せず、登録ゲームや判定不能な窓は操作しません。",
    category: "setup",
    tags: &["windows", "placement", "explicit-save", "non-game"],
    supportedWindowsVersions: &[
        WindowsReleaseFamily::Windows11_24H2,
        WindowsReleaseFamily::Windows11_25H2,
        WindowsReleaseFamily::Windows11_26H1,
    ],
    minimumBuild: 22_631,
    maximumTestedBuild: 26_200,
    riskLevel: ActionRiskLevel::Caution,
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    conflicts: &[],
    dependencies: &[],
    action_version: 1,
    kind: ActionKind::Persistent,
    parameter_schema: r#"{"invocation":"backend-owned saved layout"}"#,
    resource_keys: &["windows:top-level-window-placement"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-enumwindows",
        "https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getwindowplacement",
        "https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwindowplacement",
        "https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getwindowrect",
    ],
    compatibility_key: "setup.window_layout.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低。公開Win32 APIの配置構造と対象フィルターを対象buildで再確認します。",
};

impl WindowLayoutAction {
    fn invocation(parameters: &ActionParameters) -> ActionResult<&WindowLayoutInvocation> {
        let ActionParameters::SetupWindowLayout { invocation } = parameters else {
            return Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                ActionStage::Validate,
                false,
                "action.parameters.id_mismatch",
            ));
        };
        invocation.validate().map_err(|_| {
            ActionError::new(
                ActionErrorCode::InvalidParameters,
                ActionStage::Validate,
                false,
                "action.window_layout.invalid_saved_layout",
            )
        })?;
        Ok(invocation)
    }

    fn payload<'a>(
        invocation: &WindowLayoutInvocation,
        envelope: &'a BackupEnvelope,
        stage: ActionStage,
    ) -> ActionResult<&'a WindowLayoutBackup> {
        let BackupPayload::WindowLayout(payload) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                stage,
                "action.window_layout.backup_kind_mismatch",
            ));
        };
        if payload.desired != invocation.desired {
            return Err(ActionError::recovery_required(
                stage,
                "action.window_layout.backup_parameter_mismatch",
            ));
        }
        Ok(payload)
    }

    fn effective_exclusions(
        invocation: &WindowLayoutInvocation,
        payload: &WindowLayoutBackup,
        stage: ActionStage,
    ) -> ActionResult<Vec<crate::action::ProcessFileIdentity>> {
        let mut result = payload.excluded_game_file_identities.clone();
        let mut seen = result
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        for identity in &invocation.excluded_game_file_identities {
            if seen.insert(*identity) {
                result.push(*identity);
            }
        }
        if result.len() > 800 {
            return Err(ActionError::recovery_required(
                stage,
                "action.window_layout.game_exclusion_limit",
            ));
        }
        Ok(result)
    }

    fn state(context: &ActionContext<'_>, observation: WindowLayoutObservation) -> DetectedState {
        DetectedState::Known {
            value: ObservedValue::WindowLayout(observation),
            evidence: evidence(context, "EnumWindows + GetWindowPlacement + GetWindowRect"),
        }
    }

    fn observe(
        context: &ActionContext<'_>,
        invocation: &WindowLayoutInvocation,
        stage: ActionStage,
    ) -> ActionResult<DetectedState> {
        let observation = observe_window_layout(
            &invocation.desired,
            &invocation.excluded_game_file_identities,
        )
        .map_err(|error| map_windows_error(stage, "action.window_layout.observe_failed", error))?;
        Ok(Self::state(context, observation))
    }

    fn expected_after_restore(
        context: &ActionContext<'_>,
        before: &DetectedState,
        invocation: &WindowLayoutInvocation,
    ) -> ActionResult<DetectedState> {
        let Some(ObservedValue::WindowLayout(observation)) = before.known_value() else {
            return Err(ActionError::new(
                ActionErrorCode::StateUnknown,
                ActionStage::Backup,
                false,
                "action.window_layout.precondition_unknown",
            ));
        };
        let mut expected = observation.clone();
        expected.positioned_window_count = expected.matched_window_count;
        expected.issues.retain(|issue| {
            issue.reason != crate::window_layout::WindowLayoutIssueReason::VerificationMismatch
        });
        expected.geometry_fingerprint = Fingerprint::of_parts([
            b"window-layout-intended".as_slice(),
            invocation.desired.snapshot_id.as_bytes().as_slice(),
        ])
        .to_hex();
        Ok(Self::state(context, expected))
    }

    fn fully_positioned(state: &DetectedState) -> bool {
        matches!(
            state.known_value(),
            Some(ObservedValue::WindowLayout(observation))
                if observation.positioned_window_count == observation.matched_window_count
                    && !observation.has_verification_mismatch()
        )
    }

    fn original_state(
        context: &ActionContext<'_>,
        payload: &WindowLayoutBackup,
        excluded_game_file_identities: &[crate::action::ProcessFileIdentity],
        stage: ActionStage,
    ) -> ActionResult<DetectedState> {
        if payload.original_entries.is_empty() {
            return Ok(Self::state(
                context,
                WindowLayoutObservation {
                    saved_window_count: 0,
                    matched_window_count: 0,
                    positioned_window_count: 0,
                    issues: Vec::new(),
                    geometry_fingerprint: Fingerprint::of_bytes(
                        b"window-layout-no-original-targets",
                    )
                    .to_hex(),
                },
            ));
        }
        let observation = observe_original_window_placements(
            &payload.original_entries,
            excluded_game_file_identities,
        )
        .map_err(|error| {
            map_windows_error(stage, "action.window_layout.observe_original_failed", error)
        })?;
        Ok(Self::state(context, observation))
    }

    fn compensate(
        payload: &WindowLayoutBackup,
        excluded_game_file_identities: &[crate::action::ProcessFileIdentity],
    ) -> ActionResult<()> {
        let observation = restore_window_placement_entries(
            &payload.desired,
            &payload.original_entries,
            excluded_game_file_identities,
        )
        .map_err(|_| {
            ActionError::recovery_required(
                ActionStage::Recovery,
                "action.window_layout.compensation_failed",
            )
        })?;
        if observation.issues.is_empty()
            && observation.positioned_window_count == observation.matched_window_count
        {
            Ok(())
        } else {
            Err(ActionError::recovery_required(
                ActionStage::Recovery,
                "action.window_layout.compensation_unverified",
            ))
        }
    }

    fn external_change(stage: ActionStage) -> ActionError {
        ActionError::new(
            ActionErrorCode::ExternalConflict,
            stage,
            false,
            "action.window_layout.external_change_detected",
        )
    }

    fn transaction_state(
        payload: &WindowLayoutBackup,
        excluded_game_file_identities: &[crate::action::ProcessFileIdentity],
        stage: ActionStage,
    ) -> ActionResult<WindowLayoutTransactionState> {
        classify_window_layout_transaction(
            &payload.desired,
            &payload.original_entries,
            excluded_game_file_identities,
        )
        .map_err(|error| {
            map_windows_error(
                stage,
                "action.window_layout.classify_transaction_failed",
                error,
            )
        })
    }
}

/// The generic engine classifier asks this only after the normal original and
/// fully-applied verification paths both failed. It lets crash recovery resume
/// a rollback when every exact backed-up target is independently at either the
/// desired or original placement.
pub(crate) fn classify_recoverable_window_layout(
    parameters: &ActionParameters,
    envelope: &BackupEnvelope,
) -> ActionResult<WindowLayoutTransactionState> {
    let invocation = WindowLayoutAction::invocation(parameters)?;
    let payload = WindowLayoutAction::payload(invocation, envelope, ActionStage::Recovery)?;
    let exclusions =
        WindowLayoutAction::effective_exclusions(invocation, payload, ActionStage::Recovery)?;
    WindowLayoutAction::transaction_state(payload, &exclusions, ActionStage::Recovery)
}

impl Action for WindowLayoutAction {
    fn metadata(&self) -> &'static ActionMetadata {
        &METADATA
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Detect)?;
        let invocation = Self::invocation(parameters)?;
        Self::observe(context, invocation, ActionStage::Detect)
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, true, ActionStage::Validate)?;
        let _ = Self::invocation(parameters)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        self.validate(context, parameters)?;
        let invocation = Self::invocation(parameters)?;
        let before = Self::observe(context, invocation, ActionStage::Backup)?;
        let originals = capture_window_layout_originals(
            &invocation.desired,
            &invocation.excluded_game_file_identities,
        )
        .map_err(|error| {
            map_windows_error(
                ActionStage::Backup,
                "action.window_layout.backup_failed",
                error,
            )
        })?;
        let stable = verify_captured_window_layout_originals(
            &invocation.desired,
            &originals,
            &invocation.excluded_game_file_identities,
        )
        .map_err(|error| {
            map_windows_error(
                ActionStage::Backup,
                "action.window_layout.backup_failed",
                error,
            )
        })?;
        if !stable {
            return Err(Self::external_change(ActionStage::Backup));
        }
        let intended = Self::expected_after_restore(context, &before, invocation)?;
        Ok(BackupDraft {
            precondition_fingerprint: fingerprint_state(&before, ActionStage::Backup)?,
            intended_fingerprint: fingerprint_state(&intended, ActionStage::Backup)?,
            payload: BackupPayload::WindowLayout(WindowLayoutBackup {
                desired: invocation.desired.clone(),
                original_entries: originals,
                excluded_game_file_identities: invocation.excluded_game_file_identities.clone(),
            }),
        })
    }

    fn apply(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<AppliedEvidence> {
        self.validate(context, parameters)?;
        validate_backup_for_apply(&METADATA, context, envelope)?;
        let invocation = Self::invocation(parameters)?;
        let payload = Self::payload(invocation, envelope, ActionStage::Apply)?;
        let exclusions = Self::effective_exclusions(invocation, payload, ActionStage::Apply)?;
        let before = Self::observe(context, invocation, ActionStage::Apply)?;
        if fingerprint_state(&before, ActionStage::Apply)? != envelope.precondition_fingerprint {
            return Err(Self::external_change(ActionStage::Apply));
        }

        let observation =
            match restore_window_layout(&payload.desired, &payload.original_entries, &exclusions) {
                Ok(observation) => observation,
                Err(error) => {
                    if error.kind == WindowsErrorKind::RecoveryRequired {
                        return Err(ActionError::recovery_required(
                            ActionStage::Apply,
                            "action.window_layout.compensation_failed",
                        ));
                    }
                    return Err(map_windows_error(
                        ActionStage::Apply,
                        "action.window_layout.apply_failed",
                        error,
                    ));
                }
            };
        if observation.has_verification_mismatch()
            || observation.positioned_window_count != observation.matched_window_count
        {
            Self::compensate(payload, &exclusions)?;
            return Err(ActionError::new(
                ActionErrorCode::StateUnknown,
                ActionStage::Apply,
                false,
                "action.window_layout.apply_verification_mismatch",
            ));
        }
        let state = Self::state(context, observation);
        Ok(AppliedEvidence {
            applied_fingerprint: fingerprint_state(&state, ActionStage::Apply)?,
            state,
        })
    }

    fn verify_applied(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyApplied)?;
        let invocation = Self::invocation(parameters)?;
        let payload = Self::payload(invocation, envelope, ActionStage::VerifyApplied)?;
        let exclusions =
            Self::effective_exclusions(invocation, payload, ActionStage::VerifyApplied)?;
        let observed = Self::observe(context, invocation, ActionStage::VerifyApplied)?;
        let fingerprint = fingerprint_state(&observed, ActionStage::VerifyApplied)?;
        let transaction_state =
            Self::transaction_state(payload, &exclusions, ActionStage::VerifyApplied)?;
        Ok(Verification {
            verified: matches!(
                transaction_state,
                WindowLayoutTransactionState::Desired | WindowLayoutTransactionState::Original
            ) && Self::fully_positioned(&observed)
                && envelope.applied_fingerprint == Some(fingerprint),
            observed,
        })
    }

    fn rollback(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Rollback)?;
        validate_backup(&METADATA, context, envelope, ActionStage::Rollback)?;
        let invocation = Self::invocation(parameters)?;
        let payload = Self::payload(invocation, envelope, ActionStage::Rollback)?;
        let exclusions = Self::effective_exclusions(invocation, payload, ActionStage::Rollback)?;
        match Self::transaction_state(payload, &exclusions, ActionStage::Rollback)? {
            // Even Original must pass through the primitive after the journal
            // crossed APPLYING. A crash may have interrupted a synchronous
            // SetWindowPlacement call before its outcome became durable.
            WindowLayoutTransactionState::Original
            | WindowLayoutTransactionState::Desired
            | WindowLayoutTransactionState::MixedOwned
            | WindowLayoutTransactionState::OriginalWithExternal
            | WindowLayoutTransactionState::AppliedWithExternal => {}
            WindowLayoutTransactionState::Third => {
                return Err(Self::external_change(ActionStage::Rollback));
            }
        }
        let outcome = restore_window_placement_entries(
            &payload.desired,
            &payload.original_entries,
            &exclusions,
        )
        .map_err(|_| {
            ActionError::recovery_required(
                ActionStage::Rollback,
                "action.window_layout.rollback_failed",
            )
        })?;
        if outcome.has_verification_mismatch() {
            return Err(ActionError::recovery_required(
                ActionStage::Rollback,
                "action.window_layout.rollback_unverified",
            ));
        }
        let state = Self::original_state(context, payload, &exclusions, ActionStage::Rollback)?;
        Ok(RollbackEvidence {
            restored_fingerprint: fingerprint_state(&state, ActionStage::Rollback)?,
            state,
        })
    }

    fn verify_rolled_back(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyRolledBack)?;
        let invocation = Self::invocation(parameters)?;
        let payload = Self::payload(invocation, envelope, ActionStage::VerifyRolledBack)?;
        let exclusions =
            Self::effective_exclusions(invocation, payload, ActionStage::VerifyRolledBack)?;
        let observed =
            Self::original_state(context, payload, &exclusions, ActionStage::VerifyRolledBack)?;
        let transaction_state =
            Self::transaction_state(payload, &exclusions, ActionStage::VerifyRolledBack)?;
        let verified = match observed.known_value() {
            Some(ObservedValue::WindowLayout(value)) => {
                matches!(
                    transaction_state,
                    WindowLayoutTransactionState::Original
                        | WindowLayoutTransactionState::OriginalWithExternal
                ) && !value.has_verification_mismatch()
                    && value
                        .positioned_window_count
                        .saturating_add(u32::try_from(value.issues.len()).unwrap_or(u32::MAX))
                        == value.saved_window_count
            }
            _ => false,
        };
        Ok(Verification { verified, observed })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        let invocation = Self::invocation(parameters)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: format!(
                "明示保存した{}個の対象ウィンドウを、保存時の位置と表示状態へ復元します。閉じている対象は報告し、アプリは起動しません。",
                invocation.desired.entries.len()
            ),
            method:
                "EnumWindows + GetWindowPlacement/GetWindowRect + SetWindowPlacement"
                    .to_owned(),
            resources: METADATA
                .resource_keys
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "適用後の配置がPCカスタムの復元結果のままの場合だけ、直前に保存した各ウィンドウの位置と表示状態へ戻します。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[]
    }
}

#[cfg(all(test, windows))]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::{
        action::ProcessFileIdentity,
        compatibility::OsIdentity,
        window_layout::{
            SavedWindowPlacement, SavedWindowPlacementEntry, SensitiveWindowTitle,
            WindowLayoutSnapshot, WindowPoint, WindowRect,
        },
        windows::{
            allow_own_window_candidates_for_test, capture_window_entry_for_test,
            read_window_placement_for_test,
        },
    };

    fn context<'a>(os: &'a OsIdentity, transaction_id: Uuid, item_id: Uuid) -> ActionContext<'a> {
        ActionContext {
            os_identity: os,
            transaction_id,
            item_id,
            observed_at_unix_ms: 1,
            is_elevated: false,
        }
    }

    fn missing_invocation(private_title: &str) -> WindowLayoutInvocation {
        WindowLayoutInvocation {
            desired: WindowLayoutSnapshot {
                snapshot_id: Uuid::new_v4(),
                captured_at_unix_ms: 1,
                entries: vec![SavedWindowPlacementEntry {
                    entry_id: Uuid::new_v4(),
                    process_file_identity: ProcessFileIdentity {
                        volume_serial_number: u64::MAX,
                        file_id: [0xf3; 16],
                    },
                    application_label: "missing-test.exe".to_owned(),
                    class_name: "PcCustomMissingWindowClass".to_owned(),
                    title: SensitiveWindowTitle::new(private_title.to_owned())
                        .expect("bounded private title"),
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
                }],
                excluded_game_windows: 0,
                skipped_windows: 0,
            },
            excluded_game_file_identities: Vec::new(),
        }
    }

    #[test]
    fn missing_target_pipeline_reports_without_title_or_mutation() {
        let private = "private document title 4e63e9";
        let parameters = ActionParameters::SetupWindowLayout {
            invocation: missing_invocation(private),
        };
        let os = OsIdentity::from_test_build(26_100);
        let transaction_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let context = context(&os, transaction_id, item_id);

        let before = WINDOW_LAYOUT_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect missing target");
        let draft = WINDOW_LAYOUT_ACTION
            .create_backup(&context, &parameters)
            .expect("backup missing target");
        assert!(!format!("{draft:?}").contains(private));
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            1,
            os.base_build,
        );
        let applied = WINDOW_LAYOUT_ACTION
            .apply(&context, &parameters, &envelope)
            .expect("missing target is a structured nonfatal result");
        envelope.record_applied(applied.applied_fingerprint);
        assert!(
            WINDOW_LAYOUT_ACTION
                .verify_applied(&context, &parameters, &envelope)
                .expect("verify stable missing result")
                .verified
        );
        WINDOW_LAYOUT_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("no matched targets means no rollback writes");
        assert!(
            WINDOW_LAYOUT_ACTION
                .verify_rolled_back(&context, &parameters, &envelope)
                .expect("verify no-op rollback")
                .verified
        );
        let after = WINDOW_LAYOUT_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect unchanged missing target");
        assert_eq!(before.known_value(), after.known_value());
        assert!(!format!("{parameters:?}").contains(private));
    }

    #[test]
    fn window_layout_is_explicit_only_public_api_action() {
        assert_eq!(METADATA.kind, ActionKind::Persistent);
        assert_eq!(METADATA.method_class, MethodClass::PublicApi);
        assert!(!METADATA.auto_apply_eligible);
        METADATA
            .validate_static_contract()
            .expect("valid window layout metadata");
    }

    struct OwnedActionTestWindow(windows::Win32::Foundation::HWND);

    impl OwnedActionTestWindow {
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
            .expect("create an owned Action lifecycle window");
            Self(window)
        }

        fn handle(&self) -> isize {
            self.0 .0 as isize
        }
    }

    impl Drop for OwnedActionTestWindow {
        fn drop(&mut self) {
            let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(self.0) };
        }
    }

    /// Mutates only a window created and owned by this test process. The
    /// production own-process exclusion remains compiled in; a cfg(test)-only
    /// scope makes this exact Action lifecycle observable.
    #[test]
    #[ignore = "real-machine Action apply, detect, rollback, detect lifecycle smoke"]
    fn owned_window_action_apply_detect_rollback_detect_restores_preapply_geometry() {
        use std::{thread::sleep, time::Duration};
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER},
        };

        let _scope = allow_own_window_candidates_for_test();
        let private_title =
            windows::core::HSTRING::from(format!("totonoe-action-lifecycle-{}", Uuid::new_v4()));
        let owned = OwnedActionTestWindow::create(&private_title);
        sleep(Duration::from_millis(150));

        let desired_entry = capture_window_entry_for_test(owned.handle(), "owned-action-smoke.exe")
            .expect("capture saved placement A");
        let saved_rect = desired_entry.observed_rect;
        let parameters = ActionParameters::SetupWindowLayout {
            invocation: WindowLayoutInvocation {
                desired: WindowLayoutSnapshot {
                    snapshot_id: Uuid::new_v4(),
                    captured_at_unix_ms: 1,
                    entries: vec![desired_entry],
                    excluded_game_windows: 0,
                    skipped_windows: 0,
                },
                excluded_game_file_identities: Vec::new(),
            },
        };

        unsafe {
            SetWindowPos(
                HWND(owned.handle() as *mut core::ffi::c_void),
                HWND::default(),
                saved_rect.left + 190,
                saved_rect.top + 140,
                saved_rect.width(),
                saved_rect.height(),
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
        .expect("move test window to pre-apply placement B");
        sleep(Duration::from_millis(150));
        let (preapply_placement, preapply_rect) =
            read_window_placement_for_test(owned.handle()).expect("observe placement B");
        assert_ne!(preapply_rect, saved_rect);

        let os = OsIdentity::from_test_build(26_100);
        let transaction_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let context = context(&os, transaction_id, item_id);
        let before = WINDOW_LAYOUT_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect B before apply");
        let draft = WINDOW_LAYOUT_ACTION
            .create_backup(&context, &parameters)
            .expect("backup exact placement B");
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            1,
            os.base_build,
        );

        let applied = WINDOW_LAYOUT_ACTION
            .apply(&context, &parameters, &envelope)
            .expect("apply saved placement A");
        envelope.record_applied(applied.applied_fingerprint);
        let detected_applied = WINDOW_LAYOUT_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect A after apply");
        assert_eq!(applied.state.known_value(), detected_applied.known_value());
        assert!(
            WINDOW_LAYOUT_ACTION
                .verify_applied(&context, &parameters, &envelope)
                .expect("verify Action applied A")
                .verified
        );
        let (_, applied_rect) =
            read_window_placement_for_test(owned.handle()).expect("externally observe A");
        assert_eq!(applied_rect, saved_rect);

        WINDOW_LAYOUT_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("rollback to exact placement B");
        let detected_rolled_back = WINDOW_LAYOUT_ACTION
            .detect_current_state(&context, &parameters)
            .expect("detect B after rollback");
        assert_eq!(before.known_value(), detected_rolled_back.known_value());
        assert!(
            WINDOW_LAYOUT_ACTION
                .verify_rolled_back(&context, &parameters, &envelope)
                .expect("verify Action rolled back B")
                .verified
        );
        let (rolled_back_placement, rolled_back_rect) =
            read_window_placement_for_test(owned.handle()).expect("externally observe restored B");
        assert_eq!(rolled_back_rect, preapply_rect);
        assert_eq!(rolled_back_placement.show_cmd, preapply_placement.show_cmd);

        println!(
            "EVIDENCE window_action: saved_A=({},{} {}x{}), preapply_B=({},{} {}x{}), applied_A=({},{} {}x{}), rolled_back_B=({},{} {}x{})",
            saved_rect.left,
            saved_rect.top,
            saved_rect.width(),
            saved_rect.height(),
            preapply_rect.left,
            preapply_rect.top,
            preapply_rect.width(),
            preapply_rect.height(),
            applied_rect.left,
            applied_rect.top,
            applied_rect.width(),
            applied_rect.height(),
            rolled_back_rect.left,
            rolled_back_rect.top,
            rolled_back_rect.width(),
            rolled_back_rect.height(),
        );
    }
}
