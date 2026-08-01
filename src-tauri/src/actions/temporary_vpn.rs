use std::{collections::BTreeMap, sync::LazyLock};

use parking_lot::Mutex;
use uuid::Uuid;

use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, RollbackEvidence,
        TemporaryVpnObservation, TroubleshootingStep, ValidationReport, Verification,
        WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, Fingerprint, TemporaryVpnBackup},
    windows::{
        connect_registered_vpn, disconnect_owned_vpn, read_vpn_inventory, VpnConnectionHandle,
        VpnEntryName, VpnInventory, WindowsErrorKind,
    },
};

use super::common::{
    evidence, map_windows_error, validate_backup, validate_backup_for_apply, validate_base,
};

pub struct TemporaryVpnAction;
pub static TEMPORARY_VPN_ACTION: TemporaryVpnAction = TemporaryVpnAction;

#[derive(Clone, Copy)]
struct OwnedConnection {
    name_hash: Fingerprint,
    handle: VpnConnectionHandle,
}

static OWNED_CONNECTIONS: LazyLock<Mutex<BTreeMap<Uuid, OwnedConnection>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

static METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::SessionTemporaryVpn,
    name: "仕事用VPNを一時接続",
    description: "Windowsに登録済みのVPNを選び、必要な作業の間だけ接続します。開始前から接続中なら何もせず、自分が今回接続した場合だけ終了時に切断します。",
    category: "session",
    tags: &["VPN", "一時接続", "Windows公開API"],
    supportedWindowsVersions: &[
        WindowsReleaseFamily::Windows11_24H2,
        WindowsReleaseFamily::Windows11_25H2,
    ],
    minimumBuild: 26_100,
    maximumTestedBuild: 26_200,
    riskLevel: ActionRiskLevel::Caution,
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    conflicts: &[],
    dependencies: &[],
    action_version: 1,
    kind: ActionKind::Session,
    parameter_schema: r#"{"connection":"registered-windows-vpn-entry-name"}"#,
    resource_keys: &["windows:network:ras-vpn"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/ras/nf-ras-rasenumentriesw",
        "https://learn.microsoft.com/windows/win32/api/ras/nf-ras-rasenumconnectionsw",
        "https://learn.microsoft.com/windows/win32/api/ras/nf-ras-rasdialw",
        "https://learn.microsoft.com/windows/win32/api/ras/nf-ras-rashangupw",
    ],
    compatibility_key: "session.temporary_vpn.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低",
};

impl TemporaryVpnAction {
    fn target(parameters: &ActionParameters, stage: ActionStage) -> ActionResult<&VpnEntryName> {
        let ActionParameters::SessionTemporaryVpn { connection } = parameters else {
            return Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                stage,
                false,
                "action.parameters.id_mismatch",
            ));
        };
        if !connection.is_valid() {
            return Err(ActionError::new(
                ActionErrorCode::InvalidParameters,
                stage,
                false,
                "action.temporary_vpn.selection_required",
            ));
        }
        Ok(connection)
    }

    fn read(stage: ActionStage) -> ActionResult<VpnInventory> {
        read_vpn_inventory().map_err(|error| {
            map_windows_error(stage, "action.temporary_vpn.read_inventory_failed", error)
        })
    }

    fn ensure_registered(
        inventory: &VpnInventory,
        target: &VpnEntryName,
        stage: ActionStage,
    ) -> ActionResult<bool> {
        inventory.is_connected(target).ok_or_else(|| {
            ActionError::new(
                ActionErrorCode::InvalidParameters,
                stage,
                false,
                "action.temporary_vpn.not_registered",
            )
        })
    }

    fn observed_state(context: &ActionContext<'_>, inventory: &VpnInventory) -> DetectedState {
        DetectedState::Known {
            value: ObservedValue::TemporaryVpn(TemporaryVpnObservation {
                entries: inventory.entries.clone(),
            }),
            evidence: evidence(
                context,
                "RasEnumEntriesW and RasEnumConnectionsW state readback",
            ),
        }
    }

    fn selected_fingerprint(
        inventory: &VpnInventory,
        target: &VpnEntryName,
        stage: ActionStage,
    ) -> ActionResult<Fingerprint> {
        inventory.selected_fingerprint(target).ok_or_else(|| {
            ActionError::new(
                ActionErrorCode::StateUnknown,
                stage,
                false,
                "action.temporary_vpn.selected_state_missing",
            )
        })
    }

    fn intended_fingerprint(target: &VpnEntryName) -> Fingerprint {
        let connected = [1u8];
        Fingerprint::of_parts([target.as_str().as_bytes(), connected.as_slice()])
    }

    fn payload(envelope: &BackupEnvelope, stage: ActionStage) -> ActionResult<&TemporaryVpnBackup> {
        let BackupPayload::TemporaryVpn(payload) = &envelope.payload else {
            return Err(ActionError::recovery_required(
                stage,
                "action.temporary_vpn.backup_kind_mismatch",
            ));
        };
        if !payload.connection.is_valid() || !payload.intended_connected {
            return Err(ActionError::recovery_required(
                stage,
                "action.temporary_vpn.backup_contract_mismatch",
            ));
        }
        Ok(payload)
    }

    fn assert_parameter_matches_backup(
        target: &VpnEntryName,
        payload: &TemporaryVpnBackup,
        stage: ActionStage,
    ) -> ActionResult<()> {
        if target != &payload.connection {
            return Err(ActionError::recovery_required(
                stage,
                "action.temporary_vpn.parameter_backup_mismatch",
            ));
        }
        Ok(())
    }

    fn owned(item_id: Uuid, payload: &TemporaryVpnBackup) -> ActionResult<OwnedConnection> {
        let owned = OWNED_CONNECTIONS
            .lock()
            .get(&item_id)
            .copied()
            .ok_or_else(|| {
                ActionError::recovery_required(
                    ActionStage::Rollback,
                    "action.temporary_vpn.ownership_lost_after_restart",
                )
            })?;
        if owned.name_hash != payload.connection.fingerprint() {
            return Err(ActionError::recovery_required(
                ActionStage::Rollback,
                "action.temporary_vpn.ownership_mismatch",
            ));
        }
        Ok(owned)
    }

    fn cleanup_failed_apply(
        item_id: Uuid,
        target: &VpnEntryName,
        handle: VpnConnectionHandle,
    ) -> ActionResult<()> {
        disconnect_owned_vpn(target, handle).map_err(|error| {
            map_windows_error(
                ActionStage::Apply,
                "action.temporary_vpn.apply_compensation_failed",
                error,
            )
        })?;
        OWNED_CONNECTIONS.lock().remove(&item_id);
        Ok(())
    }
}

impl Action for TemporaryVpnAction {
    fn metadata(&self) -> &'static ActionMetadata {
        &METADATA
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Detect)?;
        let ActionParameters::SessionTemporaryVpn { .. } = parameters else {
            return Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                ActionStage::Detect,
                false,
                "action.parameters.id_mismatch",
            ));
        };
        let inventory = Self::read(ActionStage::Detect)?;
        Ok(Self::observed_state(context, &inventory))
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        let report = validate_base(&METADATA, context, parameters, true, ActionStage::Validate)?;
        let target = Self::target(parameters, ActionStage::Validate)?;
        let inventory = Self::read(ActionStage::Validate)?;
        Self::ensure_registered(&inventory, target, ActionStage::Validate)?;
        Ok(report)
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Backup)?;
        let target = Self::target(parameters, ActionStage::Backup)?;
        let inventory = Self::read(ActionStage::Backup)?;
        let original_connected = Self::ensure_registered(&inventory, target, ActionStage::Backup)?;
        Ok(BackupDraft {
            precondition_fingerprint: Self::selected_fingerprint(
                &inventory,
                target,
                ActionStage::Backup,
            )?,
            intended_fingerprint: Self::intended_fingerprint(target),
            payload: BackupPayload::TemporaryVpn(TemporaryVpnBackup {
                connection: target.clone(),
                original_connected,
                intended_connected: true,
            }),
        })
    }

    fn apply(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<AppliedEvidence> {
        validate_base(&METADATA, context, parameters, true, ActionStage::Apply)?;
        let target = Self::target(parameters, ActionStage::Apply)?;
        validate_backup_for_apply(&METADATA, context, envelope)?;
        let payload = Self::payload(envelope, ActionStage::Apply)?;
        Self::assert_parameter_matches_backup(target, payload, ActionStage::Apply)?;

        let before = Self::read(ActionStage::Apply)?;
        let current_connected = Self::ensure_registered(&before, target, ActionStage::Apply)?;
        if current_connected != payload.original_connected {
            return Err(ActionError::new(
                ActionErrorCode::ExternalConflict,
                ActionStage::Apply,
                false,
                "action.apply.external_change_detected",
            ));
        }
        if payload.original_connected {
            return Ok(AppliedEvidence {
                state: Self::observed_state(context, &before),
                applied_fingerprint: Self::selected_fingerprint(
                    &before,
                    target,
                    ActionStage::Apply,
                )?,
            });
        }

        let handle = connect_registered_vpn(target).map_err(|error| {
            if error.kind == WindowsErrorKind::ApiFailure {
                let detail = match error.os_code {
                    Some(os_code) => format!("{} (OS code {})", error.operation, os_code),
                    None => error.operation.to_owned(),
                };
                ActionError::new(
                    ActionErrorCode::GuidedRequired,
                    ActionStage::Apply,
                    false,
                    "action.temporary_vpn.windows_sign_in_required",
                )
                .with_safe_detail(detail)
            } else {
                map_windows_error(
                    ActionStage::Apply,
                    "action.temporary_vpn.connect_failed",
                    error,
                )
            }
        })?;
        let owned = OwnedConnection {
            name_hash: target.fingerprint(),
            handle,
        };
        let mut owners = OWNED_CONNECTIONS.lock();
        if owners.contains_key(&context.item_id) {
            drop(owners);
            disconnect_owned_vpn(target, handle).map_err(|error| {
                map_windows_error(
                    ActionStage::Apply,
                    "action.temporary_vpn.duplicate_owner_compensation_failed",
                    error,
                )
            })?;
            return Err(ActionError::recovery_required(
                ActionStage::Apply,
                "action.temporary_vpn.duplicate_runtime_owner",
            ));
        }
        owners.insert(context.item_id, owned);
        drop(owners);

        let applied = match Self::read(ActionStage::Apply) {
            Ok(inventory)
                if inventory.is_connected(target) == Some(true)
                    && inventory.selected_fingerprint(target)
                        == Some(Self::intended_fingerprint(target)) =>
            {
                inventory
            }
            Ok(_) => {
                Self::cleanup_failed_apply(context.item_id, target, handle)?;
                return Err(ActionError::recovery_required(
                    ActionStage::Apply,
                    "action.temporary_vpn.apply_readback_mismatch",
                ));
            }
            Err(error) => {
                Self::cleanup_failed_apply(context.item_id, target, handle)?;
                return Err(error);
            }
        };
        Ok(AppliedEvidence {
            state: Self::observed_state(context, &applied),
            applied_fingerprint: Self::selected_fingerprint(&applied, target, ActionStage::Apply)?,
        })
    }

    fn verify_applied(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyApplied)?;
        let target = Self::target(parameters, ActionStage::VerifyApplied)?;
        let payload = Self::payload(envelope, ActionStage::VerifyApplied)?;
        Self::assert_parameter_matches_backup(target, payload, ActionStage::VerifyApplied)?;
        if !payload.original_connected {
            let owned = OWNED_CONNECTIONS.lock().get(&context.item_id).copied();
            if owned.map(|value| value.name_hash) != Some(target.fingerprint()) {
                return Err(ActionError::recovery_required(
                    ActionStage::VerifyApplied,
                    "action.temporary_vpn.ownership_missing_after_apply",
                ));
            }
        }
        let current = Self::read(ActionStage::VerifyApplied)?;
        Ok(Verification {
            verified: current.is_connected(target) == Some(true),
            observed: Self::observed_state(context, &current),
        })
    }

    fn rollback(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence> {
        validate_base(&METADATA, context, parameters, false, ActionStage::Rollback)?;
        let target = Self::target(parameters, ActionStage::Rollback)?;
        validate_backup(&METADATA, context, envelope, ActionStage::Rollback)?;
        let payload = Self::payload(envelope, ActionStage::Rollback)?;
        Self::assert_parameter_matches_backup(target, payload, ActionStage::Rollback)?;

        let current = Self::read(ActionStage::Rollback)?;
        if current.is_connected(target) != Some(payload.intended_connected) {
            return Err(ActionError::new(
                ActionErrorCode::ExternalConflict,
                ActionStage::Rollback,
                false,
                "action.rollback.external_change_detected",
            ));
        }
        if payload.original_connected {
            return Ok(RollbackEvidence {
                state: Self::observed_state(context, &current),
                restored_fingerprint: Self::selected_fingerprint(
                    &current,
                    target,
                    ActionStage::Rollback,
                )?,
            });
        }

        let owned = Self::owned(context.item_id, payload)?;
        disconnect_owned_vpn(target, owned.handle).map_err(|error| {
            map_windows_error(
                ActionStage::Rollback,
                "action.temporary_vpn.disconnect_failed",
                error,
            )
        })?;
        OWNED_CONNECTIONS.lock().remove(&context.item_id);
        let restored = Self::read(ActionStage::Rollback)?;
        if restored.is_connected(target) != Some(false) {
            return Err(ActionError::recovery_required(
                ActionStage::Rollback,
                "action.temporary_vpn.rollback_readback_mismatch",
            ));
        }
        Ok(RollbackEvidence {
            state: Self::observed_state(context, &restored),
            restored_fingerprint: Self::selected_fingerprint(
                &restored,
                target,
                ActionStage::Rollback,
            )?,
        })
    }

    fn verify_rolled_back(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        envelope: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(&METADATA, context, envelope, ActionStage::VerifyRolledBack)?;
        let target = Self::target(parameters, ActionStage::VerifyRolledBack)?;
        let payload = Self::payload(envelope, ActionStage::VerifyRolledBack)?;
        Self::assert_parameter_matches_backup(target, payload, ActionStage::VerifyRolledBack)?;
        let current = Self::read(ActionStage::VerifyRolledBack)?;
        Ok(Verification {
            verified: current.is_connected(target) == Some(payload.original_connected)
                && (payload.original_connected
                    || !OWNED_CONNECTIONS.lock().contains_key(&context.item_id)),
            observed: Self::observed_state(context, &current),
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        Self::target(parameters, ActionStage::Validate)?;
        Ok(ChangeExplanation {
            action_id: METADATA.id,
            result: "選択した登録済みVPNを、この作業の間だけ接続します。開始前から接続中なら出番なしとして何も変えません。".to_owned(),
            method: "Windowsの公開RAS API（認証情報は受け取りません）".to_owned(),
            resources: vec!["利用者が選んだ登録済みVPN 1件".to_owned()],
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: METADATA.windows_update_impact.to_owned(),
            rollback_scope: "同じPCカスタム実行中に、自分が得た接続ハンドルの所有を確認できる場合だけ切断します。再起動後や途中で状態が変わった場合は自動切断しません。".to_owned(),
        })
    }

    fn troubleshooting(&self, _code: ActionErrorCode) -> &'static [TroubleshootingStep] {
        &[TroubleshootingStep {
            message_key: "action.temporary_vpn.open_windows_vpn_settings",
            opens_official_settings: true,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(value: &str) -> VpnEntryName {
        VpnEntryName::new(value.to_owned()).expect("valid test VPN entry")
    }

    fn parameters(connection: VpnEntryName) -> ActionParameters {
        ActionParameters::SessionTemporaryVpn { connection }
    }

    #[test]
    fn sensitive_names_are_redacted_from_parameter_and_backup_debug() {
        let private = "Private organization connection";
        let target = name(private);
        let parameters = parameters(target.clone());
        let backup = TemporaryVpnBackup {
            connection: target,
            original_connected: false,
            intended_connected: true,
        };
        assert!(!format!("{parameters:?}").contains(private));
        assert!(!format!("{backup:?}").contains(private));
    }

    #[test]
    fn backup_contract_requires_a_valid_connected_intention() {
        let draft = BackupDraft {
            precondition_fingerprint: Fingerprint::of_bytes(b"before"),
            intended_fingerprint: Fingerprint::of_bytes(b"after"),
            payload: BackupPayload::TemporaryVpn(TemporaryVpnBackup {
                connection: name("test"),
                original_connected: false,
                intended_connected: false,
            }),
        };
        let envelope = BackupEnvelope::from_draft(
            draft,
            Uuid::nil(),
            Uuid::nil(),
            METADATA.id,
            METADATA.action_version,
            0,
            26_200,
        );
        assert_eq!(
            TemporaryVpnAction::payload(&envelope, ActionStage::Rollback)
                .expect_err("disconnected intention must fail")
                .code,
            ActionErrorCode::RecoveryRequired
        );
    }

    #[test]
    fn connected_and_disconnected_states_have_distinct_fingerprints() {
        let target = name("test");
        let inventory = |connected| VpnInventory {
            entries: vec![crate::windows::VpnEntryState {
                name: target.clone(),
                connected,
            }],
        };
        assert_ne!(
            TemporaryVpnAction::selected_fingerprint(
                &inventory(false),
                &target,
                ActionStage::Detect,
            )
            .expect("disconnected fingerprint"),
            TemporaryVpnAction::selected_fingerprint(
                &inventory(true),
                &target,
                ActionStage::Detect,
            )
            .expect("connected fingerprint")
        );
    }

    #[cfg(windows)]
    struct RestoreVpnOnDrop {
        item_id: Uuid,
        connection: VpnEntryName,
        armed: bool,
    }

    #[cfg(windows)]
    impl Drop for RestoreVpnOnDrop {
        fn drop(&mut self) {
            if !self.armed {
                return;
            }
            let owned = OWNED_CONNECTIONS.lock().get(&self.item_id).copied();
            if let Some(owned) = owned {
                if disconnect_owned_vpn(&self.connection, owned.handle).is_ok() {
                    OWNED_CONNECTIONS.lock().remove(&self.item_id);
                }
            }
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "実機の登録済みVPNを、資格情報を受け取らず一時接続して元へ戻す"]
    fn real_machine_temporary_vpn_round_trip() {
        use crate::{compatibility::OsIdentity, windows::acquire_core_mutation_lock};

        let _mutation_lock = acquire_core_mutation_lock().expect("exclusive core mutation lock");
        let before = read_vpn_inventory().expect("read registered VPN inventory");
        println!(
            "EVIDENCE: temporary_vpn registered_count={} connected_count={}",
            before.entries.len(),
            before.connected_count()
        );
        if before.entries.is_empty() {
            println!(
                "EVIDENCE: temporary_vpn measured=false reason=no_registered_vpn no_change=true"
            );
            return;
        }
        if before.connected_count() > 0 {
            println!("EVIDENCE: temporary_vpn measured=false reason=already_connected no_op=true");
            return;
        }

        let target = before.entries[0].name.clone();
        let target_hash = target.fingerprint();
        let os = OsIdentity::load().expect("load real Windows identity");
        let transaction_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let context = ActionContext {
            os_identity: &os,
            transaction_id,
            item_id,
            observed_at_unix_ms: os.observed_at_unix_ms,
            is_elevated: false,
        };
        let parameters = parameters(target.clone());
        let draft = TEMPORARY_VPN_ACTION
            .create_backup(&context, &parameters)
            .expect("save exact original connection state");
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            METADATA.id,
            METADATA.action_version,
            os.observed_at_unix_ms,
            os.base_build,
        );
        let mut cleanup = RestoreVpnOnDrop {
            item_id,
            connection: target.clone(),
            armed: true,
        };

        let applied = match TEMPORARY_VPN_ACTION.apply(&context, &parameters, &envelope) {
            Ok(applied) => applied,
            Err(error)
                if matches!(
                    error.code,
                    ActionErrorCode::GuidedRequired | ActionErrorCode::AccessDenied
                ) =>
            {
                let after_failure = read_vpn_inventory().expect("read after guided failure");
                let unchanged = after_failure.is_connected(&target) == Some(false);
                println!(
                    "EVIDENCE: temporary_vpn measured=false reason=windows_sign_in_required guided=true no_change={unchanged}"
                );
                assert!(unchanged);
                cleanup.armed = false;
                return;
            }
            Err(error) => panic!("temporary VPN apply failed unexpectedly: {error}"),
        };
        envelope.record_applied(applied.applied_fingerprint);
        let independent_after = crate::windows::read_vpn_probe_in_child()
            .expect("independent connected-state readback");
        let connected_after = independent_after.connected_hashes.contains(&target_hash);
        println!(
            "EVIDENCE: temporary_vpn applied target_hash={} independent_connected={} registered_count={}",
            hex::encode(target_hash.0),
            connected_after,
            independent_after.registered_count
        );
        assert!(connected_after);
        assert!(
            TEMPORARY_VPN_ACTION
                .verify_applied(&context, &parameters, &envelope)
                .expect("verify temporary VPN applied")
                .verified
        );

        TEMPORARY_VPN_ACTION
            .rollback(&context, &parameters, &envelope)
            .expect("disconnect only the owned VPN connection");
        let independent_restored =
            crate::windows::read_vpn_probe_in_child().expect("independent restored-state readback");
        let disconnected_after = !independent_restored.connected_hashes.contains(&target_hash);
        println!(
            "EVIDENCE: temporary_vpn restored target_hash={} independent_disconnected={} registered_count={}",
            hex::encode(target_hash.0),
            disconnected_after,
            independent_restored.registered_count
        );
        assert!(disconnected_after);
        assert!(
            TEMPORARY_VPN_ACTION
                .verify_rolled_back(&context, &parameters, &envelope)
                .expect("verify original disconnected state")
                .verified
        );
        cleanup.armed = false;
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "実機で到達不能なテスト用VPNエントリを作成し失敗経路と削除後始末を検証"]
    fn real_machine_unreachable_vpn_failure_and_cleanup() {
        use crate::windows::{acquire_core_mutation_lock, TestVpnEntryGuard};

        let _mutation_lock = acquire_core_mutation_lock().expect("exclusive core mutation lock");
        let before_inventory = read_vpn_inventory().expect("read initial VPN inventory");
        let before_count = before_inventory.entries.len();

        let mut guard = TestVpnEntryGuard::create_unreachable("unreachable-fail")
            .expect("create test VPN entry with unreachable IP");

        let created_inventory = read_vpn_inventory().expect("read VPN inventory after creation");
        let created_count = created_inventory.entries.len();
        let appeared_in_list = created_inventory.contains(guard.name());

        assert_eq!(
            created_count,
            before_count + 1,
            "registered_count must increase by 1"
        );
        assert!(appeared_in_list, "test VPN entry must appear in inventory");

        let connect_result = connect_registered_vpn(guard.name());
        assert!(
            connect_result.is_err(),
            "unreachable IP connection without credentials must fail"
        );

        let independent_after_fail = crate::windows::read_vpn_probe_in_child()
            .expect("independent probe readback after failure");
        let guard_hash = guard.name().fingerprint();
        let handle_left = independent_after_fail
            .connected_hashes
            .contains(&guard_hash);
        assert!(
            !handle_left,
            "no connection handle must remain after failure"
        );

        guard
            .cleanup()
            .expect("delete test VPN entry via RasDeleteEntryW");

        let after_cleanup_inventory =
            read_vpn_inventory().expect("read VPN inventory after cleanup");
        let after_cleanup_count = after_cleanup_inventory.entries.len();
        let exists_after_cleanup = after_cleanup_inventory.contains(guard.name());

        assert!(
            !exists_after_cleanup,
            "test VPN entry must not exist after cleanup"
        );
        assert_eq!(
            after_cleanup_count, before_count,
            "registered_count must return to initial count"
        );

        println!(
            "EVIDENCE: vpn_unreachable_failure registered_before={} registered_with_test={} connect_failed={} active_handles_left=false deleted_cleanly={}",
            before_count,
            created_count,
            connect_result.is_err(),
            !exists_after_cleanup && after_cleanup_count == before_count
        );
    }
}
