use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind, ActionMetadata,
        ActionParameters, ActionResult, ActionRiskLevel, ActionStage, AppliedEvidence,
        ChangeExplanation, DetectedState, MethodClass, ObservedValue, RollbackEvidence,
        TroubleshootingStep, ValidationReport, Verification, WindowsReleaseFamily,
    },
    backup::{BackupDraft, BackupEnvelope, BackupPayload, ObservationBackup},
    windows::{
        read_startup_inventory, read_system_drive_space, read_user_temp_inventory, WindowsResult,
    },
};

use super::common::{
    evidence, fingerprint_state, map_windows_error, validate_backup, validate_backup_for_apply,
    validate_base,
};

type DetectFn = fn() -> WindowsResult<ObservedValue>;
type ExpectedFn = fn(&ObservedValue) -> bool;

pub struct SystemObservationAction {
    metadata: &'static ActionMetadata,
    backup_source: &'static str,
    detect: DetectFn,
    expected: ExpectedFn,
    result: &'static str,
    method: &'static str,
}

static STARTUP_INVENTORY_METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::SetupStartupInventory,
    name: "スタートアップ項目を確認する",
    description: "HKCU/HKLMの固定RunキーとWindowsのユーザー/共通Startupフォルダーを、変更せず上限付きで一覧化します。",
    category: "setup",
    tags: &["startup", "inventory", "read-only"],
    supportedWindowsVersions: &[
        WindowsReleaseFamily::Windows11_24H2,
        WindowsReleaseFamily::Windows11_25H2,
        WindowsReleaseFamily::Windows11_26H1,
    ],
    minimumBuild: 22_631,
    maximumTestedBuild: 26_200,
    riskLevel: ActionRiskLevel::Safe,
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    conflicts: &[],
    dependencies: &[],
    action_version: 1,
    kind: ActionKind::Observation,
    parameter_schema: "{}",
    resource_keys: &[
        "registry:hkcu:startup-run:observation",
        "registry:hklm:startup-run:observation",
        "filesystem:known-startup-folders:observation",
    ],
    method_class: MethodClass::DocumentedRegistry,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/setupapi/run-and-runonce-registry-keys",
        "https://learn.microsoft.com/windows/win32/shell/knownfolderid",
        "https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-getdrivetypew",
        "https://learn.microsoft.com/windows/win32/fileio/reparse-points",
    ],
    compatibility_key: "setup.startup_inventory.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低。固定RunキーとKnown Folderの読み取りのみです。",
};

static FREE_SPACE_METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::StorageFreeSpaceCheck,
    name: "システムドライブの空き容量を確認する",
    description: "Windowsの公開APIでシステムドライブの総容量と利用可能容量を読み取るだけです。",
    category: "storage",
    tags: &["storage", "disk", "read-only"],
    supportedWindowsVersions: &[
        WindowsReleaseFamily::Windows11_24H2,
        WindowsReleaseFamily::Windows11_25H2,
        WindowsReleaseFamily::Windows11_26H1,
    ],
    minimumBuild: 22_631,
    maximumTestedBuild: 26_200,
    riskLevel: ActionRiskLevel::Safe,
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    conflicts: &[],
    dependencies: &[],
    action_version: 1,
    kind: ActionKind::Observation,
    parameter_schema: "{}",
    resource_keys: &["filesystem:system-volume-space:observation"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-getdiskfreespaceexw",
        "https://learn.microsoft.com/windows/win32/api/sysinfoapi/nf-sysinfoapi-getwindowsdirectoryw",
    ],
    compatibility_key: "storage.free_space_check.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低。公開ファイルシステムAPIの読み取りのみです。",
};

static TEMP_FILES_METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::StorageTempFilesCheck,
    name: "一時ファイルの使用量を確認する",
    description: "Windowsが返すユーザー一時フォルダーを、reparse pointを追跡せず件数・深さ・時間・合計量の上限内で集計します。削除はしません。",
    category: "storage",
    tags: &["storage", "temp", "read-only"],
    supportedWindowsVersions: &[
        WindowsReleaseFamily::Windows11_24H2,
        WindowsReleaseFamily::Windows11_25H2,
        WindowsReleaseFamily::Windows11_26H1,
    ],
    minimumBuild: 22_631,
    maximumTestedBuild: 26_200,
    riskLevel: ActionRiskLevel::Safe,
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    conflicts: &[],
    dependencies: &[],
    action_version: 1,
    kind: ActionKind::Observation,
    parameter_schema: "{}",
    resource_keys: &["filesystem:user-temp:bounded-observation"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-gettemppath2w",
        "https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-getdrivetypew",
        "https://learn.microsoft.com/windows/win32/fileio/reparse-points",
    ],
    compatibility_key: "storage.temp_files_check.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低。公開APIで得た一時フォルダーのbounded metadata走査のみです。",
};

fn detect_startup_inventory() -> WindowsResult<ObservedValue> {
    read_startup_inventory().map(ObservedValue::StartupInventory)
}

fn detect_free_space() -> WindowsResult<ObservedValue> {
    read_system_drive_space().map(ObservedValue::SystemDriveSpace)
}

fn detect_temp_files() -> WindowsResult<ObservedValue> {
    read_user_temp_inventory().map(ObservedValue::TempFiles)
}

fn is_startup_inventory(value: &ObservedValue) -> bool {
    matches!(value, ObservedValue::StartupInventory(_))
}

fn is_free_space(value: &ObservedValue) -> bool {
    matches!(value, ObservedValue::SystemDriveSpace(_))
}

fn is_temp_files(value: &ObservedValue) -> bool {
    matches!(value, ObservedValue::TempFiles(_))
}

pub static STARTUP_INVENTORY_ACTION: SystemObservationAction = SystemObservationAction {
    metadata: &STARTUP_INVENTORY_METADATA,
    backup_source: "fixed Run keys and Windows known startup folders",
    detect: detect_startup_inventory,
    expected: is_startup_inventory,
    result: "登録済みの固定RunキーとStartupフォルダーから項目名・場所・値の状態を一覧化します。コマンド本文は保持も実行もしません。",
    method: "固定HKCU/HKLM Runキー、SHGetKnownFolderPath、bounded directory enumeration（読み取り専用）",
};

pub static FREE_SPACE_CHECK_ACTION: SystemObservationAction = SystemObservationAction {
    metadata: &FREE_SPACE_METADATA,
    backup_source: "GetWindowsDirectoryW and GetDiskFreeSpaceExW",
    detect: detect_free_space,
    expected: is_free_space,
    result: "Windowsのシステムドライブについて、総容量・全体の空き・現在ユーザーが利用可能な空きを表示します。",
    method: "GetWindowsDirectoryW + GetDiskFreeSpaceExW（読み取り専用）",
};

pub static TEMP_FILES_CHECK_ACTION: SystemObservationAction = SystemObservationAction {
    metadata: &TEMP_FILES_METADATA,
    backup_source: "GetTempPath2W and bounded no-follow metadata walk",
    detect: detect_temp_files,
    expected: is_temp_files,
    result: "ユーザー一時フォルダーのファイル数と合計サイズを上限付きで集計します。ファイルの内容確認や削除は行いません。",
    method: "GetTempPath2W + reparse point非追跡のbounded metadata走査（読み取り専用）",
};

/// 現在のアクセントカラーを読み取るだけのAction。公開APIのみで、Windowsを変更しない。
static ACCENT_COLOR_CHECK_METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::AppearanceAccentColorCheck,
    name: "いまのアクセントカラーを確認する",
    description: "Windowsが現在使っている色を、公開APIで読み取るだけです。色の変更は行いません。",
    category: "appearance",
    tags: &["appearance", "accent", "read-only"],
    supportedWindowsVersions: &[
        WindowsReleaseFamily::Windows11_24H2,
        WindowsReleaseFamily::Windows11_25H2,
        WindowsReleaseFamily::Windows11_26H1,
    ],
    minimumBuild: 22_631,
    maximumTestedBuild: 26_200,
    riskLevel: ActionRiskLevel::Safe,
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    conflicts: &[],
    dependencies: &[],
    action_version: 1,
    kind: ActionKind::Observation,
    parameter_schema: "{}",
    resource_keys: &["dwm:colorization-color:observation"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/dwmapi/nf-dwmapi-dwmgetcolorizationcolor",
    ],
    compatibility_key: "appearance.accent_color_check.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "低。公開APIでの読み取りのみです。",
};

fn detect_accent_color() -> WindowsResult<ObservedValue> {
    let accent = crate::windows::system_accent_color()?;
    Ok(ObservedValue::AccentColor {
        hex: format!("#{:02X}{:02X}{:02X}", accent.red, accent.green, accent.blue),
        opaque_blend: accent.opaque_blend,
    })
}

fn expects_accent_color(value: &ObservedValue) -> bool {
    matches!(value, ObservedValue::AccentColor { .. })
}

pub static ACCENT_COLOR_CHECK_ACTION: SystemObservationAction = SystemObservationAction {
    metadata: &ACCENT_COLOR_CHECK_METADATA,
    backup_source: "appearance.accent_color_check",
    detect: detect_accent_color,
    expected: expects_accent_color,
    result: "いまWindowsが使っている色を表示します（変更しません）。",
    method: "公開APIのDwmGetColorizationColor",
};

static WINDOWS_UPDATE_STATUS_METADATA: ActionMetadata = ActionMetadata {
    id: ActionId::SetupWindowsUpdateStatus,
    name: "Windows Updateの確認状況を見る",
    description: "Windows Update Agentの公開COM APIで、最後に更新を確認できた日時と再起動保留だけを読み取ります。Updateの設定・サービス・動作は変更しません。",
    category: "setup",
    tags: &["windows-update", "status", "read-only"],
    supportedWindowsVersions: &[
        WindowsReleaseFamily::Windows11_24H2,
        WindowsReleaseFamily::Windows11_25H2,
        WindowsReleaseFamily::Windows11_26H1,
    ],
    minimumBuild: 22_631,
    maximumTestedBuild: 26_200,
    riskLevel: ActionRiskLevel::Safe,
    requiresAdmin: false,
    requiresRestart: false,
    requiresExplorerRestart: false,
    conflicts: &[],
    dependencies: &[],
    action_version: 1,
    kind: ActionKind::Observation,
    parameter_schema: "{}",
    resource_keys: &["windows-update:wua-status:observation"],
    method_class: MethodClass::PublicApi,
    evidence_urls: &[
        "https://learn.microsoft.com/windows/win32/api/wuapi/nn-wuapi-iautomaticupdatesresults",
        "https://learn.microsoft.com/windows/win32/api/wuapi/nn-wuapi-isysteminformation",
    ],
    compatibility_key: "setup.windows_update_status.v1",
    backup_codec_version: 1,
    rollback_decoder_versions: &[1],
    auto_apply_eligible: false,
    windows_update_impact: "なし。Windows Update Agentの読み取り専用プロパティだけを確認します。",
};

fn detect_windows_update_status() -> WindowsResult<ObservedValue> {
    crate::windows::read_windows_update_status().map(ObservedValue::WindowsUpdateStatus)
}

fn is_windows_update_status(value: &ObservedValue) -> bool {
    matches!(value, ObservedValue::WindowsUpdateStatus(_))
}

pub static WINDOWS_UPDATE_STATUS_ACTION: SystemObservationAction = SystemObservationAction {
    metadata: &WINDOWS_UPDATE_STATUS_METADATA,
    backup_source: "Windows Update Agent read-only COM properties",
    detect: detect_windows_update_status,
    expected: is_windows_update_status,
    result: "最後に更新を確認できたローカル日時と、Windows Update Agentが返す再起動保留状態を表示します。取得できない項目は不明と表示します。",
    method: "IAutomaticUpdatesResults::LastSearchSuccessDate と ISystemInformation::RebootRequired（読み取り専用）",
};
impl Action for SystemObservationAction {
    fn metadata(&self) -> &'static ActionMetadata {
        self.metadata
    }

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState> {
        validate_base(
            self.metadata,
            context,
            parameters,
            false,
            ActionStage::Detect,
        )?;
        let value = (self.detect)().map_err(|error| {
            map_windows_error(
                ActionStage::Detect,
                "action.system_observation.detect_failed",
                error,
            )
        })?;
        Ok(DetectedState::Known {
            value,
            evidence: evidence(context, self.backup_source),
        })
    }

    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport> {
        validate_base(
            self.metadata,
            context,
            parameters,
            false,
            ActionStage::Validate,
        )
    }

    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft> {
        self.validate(context, parameters)?;
        let state = self.detect_current_state(context, parameters)?;
        let fingerprint = fingerprint_state(&state, ActionStage::Backup)?;
        Ok(BackupDraft {
            precondition_fingerprint: fingerprint,
            intended_fingerprint: fingerprint,
            payload: BackupPayload::Observation(ObservationBackup {
                source: self.backup_source.to_owned(),
            }),
        })
    }

    fn apply(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<AppliedEvidence> {
        self.validate(context, parameters)?;
        validate_backup_for_apply(self.metadata, context, backup)?;
        match &backup.payload {
            BackupPayload::Observation(observation) if observation.source == self.backup_source => {
            }
            _ => {
                return Err(ActionError::recovery_required(
                    ActionStage::Apply,
                    "action.system_observation.backup_kind_mismatch",
                ))
            }
        }
        let state = self.detect_current_state(context, parameters)?;
        Ok(AppliedEvidence {
            applied_fingerprint: fingerprint_state(&state, ActionStage::Apply)?,
            state,
        })
    }

    fn verify_applied(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(self.metadata, context, backup, ActionStage::VerifyApplied)?;
        let observed = self.detect_current_state(context, parameters)?;
        Ok(Verification {
            verified: observed.known_value().map(self.expected).unwrap_or(false),
            observed,
        })
    }

    fn rollback(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence> {
        self.validate(context, parameters)?;
        validate_backup(self.metadata, context, backup, ActionStage::Rollback)?;
        let state = self.detect_current_state(context, parameters)?;
        Ok(RollbackEvidence {
            restored_fingerprint: fingerprint_state(&state, ActionStage::Rollback)?,
            state,
        })
    }

    fn verify_rolled_back(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<Verification> {
        validate_backup(
            self.metadata,
            context,
            backup,
            ActionStage::VerifyRolledBack,
        )?;
        let observed = self.detect_current_state(context, parameters)?;
        Ok(Verification {
            verified: observed.known_value().map(self.expected).unwrap_or(false),
            observed,
        })
    }

    fn explain_changes(&self, parameters: &ActionParameters) -> ActionResult<ChangeExplanation> {
        if parameters.action_id() != self.metadata.id {
            return Err(ActionError::new(
                ActionErrorCode::WrongParameters,
                ActionStage::Validate,
                false,
                "action.parameters.id_mismatch",
            ));
        }
        Ok(ChangeExplanation {
            action_id: self.metadata.id,
            result: self.result.to_owned(),
            method: self.method.to_owned(),
            resources: self
                .metadata
                .resource_keys
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            requires_admin: false,
            requires_restart: false,
            windows_update_impact: self.metadata.windows_update_impact.to_owned(),
            rollback_scope: "変更がないためrollbackはno-opです。".to_owned(),
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
    use crate::{backup::BackupEnvelope, compatibility::OsIdentity};

    fn assert_read_only_round_trip(
        action: &'static SystemObservationAction,
        parameters: ActionParameters,
        expected: ExpectedFn,
    ) {
        let os = OsIdentity::from_test_build(26_100);
        let transaction_id = Uuid::new_v4();
        let item_id = Uuid::new_v4();
        let context = ActionContext {
            os_identity: &os,
            transaction_id,
            item_id,
            observed_at_unix_ms: 1,
            is_elevated: false,
        };
        let before = action
            .detect_current_state(&context, &parameters)
            .expect("detect before read-only round trip");
        assert!(before.known_value().map(expected).unwrap_or(false));
        let draft = action
            .create_backup(&context, &parameters)
            .expect("create observation backup");
        let mut envelope = BackupEnvelope::from_draft(
            draft,
            transaction_id,
            item_id,
            action.metadata.id,
            action.metadata.action_version,
            1,
            os.base_build,
        );
        let applied = action
            .apply(&context, &parameters, &envelope)
            .expect("apply read-only observation");
        assert!(applied.state.known_value().map(expected).unwrap_or(false));
        envelope.record_applied(applied.applied_fingerprint);
        let detected = action
            .detect_current_state(&context, &parameters)
            .expect("detect after read-only apply");
        assert!(detected.known_value().map(expected).unwrap_or(false));
        let rolled_back = action
            .rollback(&context, &parameters, &envelope)
            .expect("rollback read-only observation");
        assert!(rolled_back
            .state
            .known_value()
            .map(expected)
            .unwrap_or(false));
        let after = action
            .detect_current_state(&context, &parameters)
            .expect("detect after read-only rollback");
        assert!(after.known_value().map(expected).unwrap_or(false));
        assert!(
            action
                .verify_rolled_back(&context, &parameters, &envelope)
                .expect("verify read-only rollback")
                .verified
        );
    }

    #[test]
    fn startup_inventory_apply_detect_rollback_detect_is_read_only() {
        assert_read_only_round_trip(
            &STARTUP_INVENTORY_ACTION,
            ActionParameters::SetupStartupInventory {},
            is_startup_inventory,
        );
    }

    #[test]
    fn free_space_apply_detect_rollback_detect_is_read_only() {
        assert_read_only_round_trip(
            &FREE_SPACE_CHECK_ACTION,
            ActionParameters::StorageFreeSpaceCheck {},
            is_free_space,
        );
    }

    #[test]
    fn temp_files_apply_detect_rollback_detect_is_read_only() {
        assert_read_only_round_trip(
            &TEMP_FILES_CHECK_ACTION,
            ActionParameters::StorageTempFilesCheck {},
            is_temp_files,
        );
    }

    #[test]
    fn windows_update_status_apply_detect_rollback_detect_is_read_only() {
        assert_read_only_round_trip(
            &WINDOWS_UPDATE_STATUS_ACTION,
            ActionParameters::SetupWindowsUpdateStatus {},
            is_windows_update_status,
        );
    }
}

#[cfg(all(test, windows))]
mod accent_color_tests {
    use super::*;

    #[test]
    fn accent_color_reads_a_valid_hex_on_this_machine() {
        // 実機の公開APIから読み取れること。値は環境依存なので形式だけ検証する。
        let observed =
            detect_accent_color().expect("read accent color via DwmGetColorizationColor");
        let ObservedValue::AccentColor { hex, .. } = &observed else {
            panic!("accent color observation expected");
        };
        assert_eq!(hex.len(), 7, "#RRGGBB 形式");
        assert!(hex.starts_with('#'));
        assert!(
            hex[1..].chars().all(|c| c.is_ascii_hexdigit()),
            "16進数のみ: {hex}"
        );
        assert!(expects_accent_color(&observed));
    }
}
