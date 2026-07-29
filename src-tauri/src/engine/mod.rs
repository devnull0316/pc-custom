mod recovery;
#[cfg(all(test, windows))]
mod tests;
mod transaction;

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rand::{rngs::OsRng, RngCore};
use uuid::Uuid;

use crate::{
    action::{
        Action, ActionContext, ActionError, ActionErrorCode, ActionId, ActionKind,
        ActionParameters, ActionResult, ActionStage, DetectedState, ObservedValue,
        ReadinessComponent, RollbackEvidence, Verification, ACTION_REGISTRY,
    },
    backup::{
        classify_registry_backup, BackupEnvelope, BackupPayload, Fingerprint,
        RegistryClassification,
    },
    compatibility::{CompatibilityCatalog, CompatibilityMode, OsIdentity},
    error::{CoreError, CoreResult},
    game_profile::{CreateProfileRequest, ImportResult, ProfileStore, StoredProfile},
    health_report::{build_health_report, HealthInput, HealthProbe, HealthReport},
    journal::{ItemState, JournalDatabase, ReconcileResult, RecoveryClassification, TimelineItem},
    presentation::{
        action_presentation, default_parameters, listing_parameters, os_label,
        parse_action_request, preview_change, state_to_ui, ActionPresentation, BootstrapStatus,
        CommitResult, DetectionResponse, PreviewActionsRequest, PreviewResponse,
    },
    window_layout::{WindowLayoutInvocation, WindowLayoutStatus, WindowLayoutStore},
};

const PREVIEW_TTL_MS: u64 = 5 * 60 * 1_000;
const MAX_ACTIONS_PER_TRANSACTION: usize = 32;

#[derive(Clone)]
struct PreviewRecord {
    invocations: Vec<ActionParameters>,
    before: HashMap<ActionId, Fingerprint>,
    os_fingerprint: Fingerprint,
    expires_at_ms: u64,
}

#[derive(Clone)]
struct WorkItem {
    item_id: Uuid,
    parameters: ActionParameters,
    backup: BackupEnvelope,
}

pub struct PcCustomEngine {
    journal: Arc<JournalDatabase>,
    initial_identity: Option<OsIdentity>,
    profile_store: Option<Arc<ProfileStore>>,
    window_layout_store: Option<Arc<WindowLayoutStore>>,
    offscreen_window_rescue: crate::windows::OffscreenWindowRescueManager,
    previews: Mutex<HashMap<String, PreviewRecord>>,
    mutation_gate: Mutex<()>,
}

impl PcCustomEngine {
    pub fn new(
        journal: Arc<JournalDatabase>,
        initial_identity: Option<OsIdentity>,
    ) -> CoreResult<Self> {
        Self::new_with_runtime_stores(journal, initial_identity, None, None)
    }

    pub fn new_with_runtime_stores(
        journal: Arc<JournalDatabase>,
        initial_identity: Option<OsIdentity>,
        profile_store: Option<Arc<ProfileStore>>,
        window_layout_store: Option<Arc<WindowLayoutStore>>,
    ) -> CoreResult<Self> {
        ACTION_REGISTRY.validate().map_err(CoreError::from)?;
        let engine = Self {
            journal,
            initial_identity,
            profile_store,
            window_layout_store,
            offscreen_window_rescue: crate::windows::OffscreenWindowRescueManager::new(),
            previews: Mutex::new(HashMap::new()),
            mutation_gate: Mutex::new(()),
        };
        engine.reconcile_now()?;
        Ok(engine)
    }

    pub fn window_layout_parameters(&self) -> CoreResult<ActionParameters> {
        let store = self.window_layout_store.as_ref().ok_or_else(|| {
            CoreError::recovery_required(
                "ウィンドウ配置の保存領域を初期化できなかったため、操作を停止しました。",
            )
        })?;
        let snapshot_id = store.current_snapshot_id()?;
        let invocation = WindowLayoutInvocation {
            desired: store.snapshot(snapshot_id)?,
            excluded_game_file_identities: self
                .profile_store
                .as_ref()
                .ok_or_else(|| {
                    CoreError::recovery_required(
                        "登録ゲームを確認できなかったため、ウィンドウ操作を停止しました。",
                    )
                })?
                .registered_game_file_identities()?,
        };
        invocation.validate()?;
        Ok(ActionParameters::SetupWindowLayout { invocation })
    }

    pub fn window_layout_status(&self) -> CoreResult<WindowLayoutStatus> {
        self.window_layout_store
            .as_ref()
            .map(|store| store.status())
            .ok_or_else(|| {
                CoreError::recovery_required(
                    "ウィンドウ配置の保存領域を初期化できなかったため、操作を停止しました。",
                )
            })
    }

    pub fn save_window_layout(
        &self,
        unregistered_games_closed: bool,
    ) -> CoreResult<WindowLayoutStatus> {
        if !unregistered_games_closed {
            return Err(CoreError::invalid_request(
                "未登録のウィンドウゲームを閉じたことを確認してから保存してください。",
            ));
        }
        let _mutation_guard = self.mutation_gate.lock();
        let _process_guard =
            crate::windows::acquire_core_mutation_lock().map_err(core_mutation_lock_error)?;
        if self.journal.recovery_count()? > 0 {
            return Err(CoreError::recovery_required(
                "未復元の変更があるため、現在の混合状態を配置として保存しません。",
            ));
        }
        let profile_store = self.profile_store.as_ref().ok_or_else(|| {
            CoreError::recovery_required(
                "登録ゲームを確認できなかったため、ウィンドウ操作を停止しました。",
            )
        })?;
        let window_layout_store = self.window_layout_store.as_ref().ok_or_else(|| {
            CoreError::recovery_required(
                "ウィンドウ配置の保存領域を初期化できなかったため、操作を停止しました。",
            )
        })?;
        let exclusions = profile_store.registered_game_file_identities()?;
        let snapshot = crate::windows::capture_window_layout(&exclusions)
            .map_err(window_layout_windows_error)?;
        if profile_store.registered_game_file_identities()? != exclusions {
            return Err(CoreError::new(
                "STALE_GAME_IDENTITY",
                "WINDOW_LAYOUT",
                true,
                "保存中に登録ゲームの実行ファイルが変わったため、配置を保存しませんでした。もう一度確認してください。",
            ));
        }
        window_layout_store.replace(snapshot)
    }

    pub fn scan_offscreen_windows(
        &self,
        anchor_window: isize,
    ) -> CoreResult<crate::windows::OffscreenWindowScan> {
        let exclusions = self
            .runtime_profile_store()?
            .registered_game_file_identities()?;
        self.offscreen_window_rescue
            .scan(&exclusions, anchor_window)
            .map_err(offscreen_window_rescue_error)
    }

    pub fn rescue_offscreen_window(
        &self,
        candidate_id: Uuid,
        anchor_window: isize,
    ) -> CoreResult<crate::windows::OffscreenWindowRescueOutcome> {
        let _mutation_guard = self.mutation_gate.lock();
        let _process_guard =
            crate::windows::acquire_core_mutation_lock().map_err(core_mutation_lock_error)?;
        self.ensure_offscreen_window_mutation_allowed()?;
        let exclusions = self
            .runtime_profile_store()?
            .registered_game_file_identities()?;
        self.offscreen_window_rescue
            .rescue(candidate_id, &exclusions, anchor_window)
            .map_err(offscreen_window_rescue_error)
    }

    pub fn rollback_offscreen_window(
        &self,
        undo_id: Uuid,
    ) -> CoreResult<crate::windows::OffscreenWindowRescueOutcome> {
        let _mutation_guard = self.mutation_gate.lock();
        let _process_guard =
            crate::windows::acquire_core_mutation_lock().map_err(core_mutation_lock_error)?;
        // Re-check that the running OS instance still matches startup before touching the HWND.
        // Rollback remains a separate one-window operation and never uses a saved layout.
        let _identity = self.identity_for_commit()?;
        let exclusions = self
            .runtime_profile_store()?
            .registered_game_file_identities()?;
        self.offscreen_window_rescue
            .rollback(undo_id, &exclusions)
            .map_err(offscreen_window_rescue_error)
    }

    fn ensure_offscreen_window_mutation_allowed(&self) -> CoreResult<()> {
        let _identity = self.identity_for_commit()?;
        if self.bootstrap_status()?.mode != "ready" {
            return Err(CoreError::recovery_required(
                "このWindows環境では、新しいウィンドウ移動を開始できません。",
            ));
        }
        Ok(())
    }

    pub fn create_profile(&self, request: CreateProfileRequest) -> CoreResult<StoredProfile> {
        let _mutation_guard = self.mutation_gate.lock();
        self.runtime_profile_store()?.create(request)
    }

    pub fn delete_profile(&self, id: &str) -> CoreResult<()> {
        let _mutation_guard = self.mutation_gate.lock();
        self.runtime_profile_store()?.delete(id)
    }

    pub fn import_profiles(&self, json: &str) -> CoreResult<ImportResult> {
        let _mutation_guard = self.mutation_gate.lock();
        self.runtime_profile_store()?.import_apply(json)
    }

    pub fn bootstrap_status(&self) -> CoreResult<BootstrapStatus> {
        let recovery_count = self.journal.recovery_count()?;
        let Some(identity) = &self.initial_identity else {
            return Ok(BootstrapStatus {
                mode: "recovery_required".to_owned(),
                os_label: "Windowsの情報を確認できません".to_owned(),
                build: None,
                message: "buildを一意に確認できないため、自動書き込みを停止しています。".to_owned(),
                recovery_count,
            });
        };
        let decision = CompatibilityCatalog::decision_for_identity(identity);
        let (mode, message) = if recovery_count > 0 {
            (
                "recovery_required",
                "未復元または競合中の項目があります。新しい変更は停止しています。",
            )
        } else {
            match decision.mode {
                CompatibilityMode::TestedMutable => {
                    ("ready", "変更前を保存し、適用後と復元後を検証します。")
                }
                CompatibilityMode::TestedDetectOnly => (
                    "read_only",
                    "実機承認前のbuildのため、状態確認だけを利用できます。",
                ),
                CompatibilityMode::Unsupported => {
                    ("unsupported", "このWindows releaseは変更対象外です。")
                }
                CompatibilityMode::UnknownBuild => (
                    "recovery_required",
                    "未知buildのため、自動書き込みと自動復元を停止しています。",
                ),
            }
        };
        Ok(BootstrapStatus {
            mode: mode.to_owned(),
            os_label: os_label(identity),
            build: Some(identity.base_build),
            message: message.to_owned(),
            recovery_count,
        })
    }

    pub fn list_actions(&self) -> Vec<ActionPresentation> {
        ACTION_REGISTRY
            .iter()
            .map(|action| {
                let metadata = action.metadata();
                let compatibility = self
                    .initial_identity
                    .as_ref()
                    .map(|identity| CompatibilityCatalog::evaluate(identity, metadata))
                    .unwrap_or_else(|| CompatibilityCatalog::decision_for_build(0));
                let current_state = self.initial_identity.as_ref().and_then(|identity| {
                    listing_parameters(metadata.id).map(|parameters| {
                        let context = action_context(identity, Uuid::nil(), Uuid::nil());
                        action
                            .detect_current_state(&context, &parameters)
                            .unwrap_or_else(|error| DetectedState::Error {
                                code: error.code.as_code().to_owned(),
                                reason: "状態を安全に確認できませんでした。".to_owned(),
                            })
                    })
                });
                action_presentation(metadata, compatibility, current_state)
            })
            .collect()
    }

    pub fn detect_action(&self, action_id: ActionId) -> CoreResult<DetectionResponse> {
        let parameters = default_parameters(action_id).ok_or_else(|| {
            CoreError::invalid_request("ゲーム監視には確認済みの実行ファイルbindingが必要です。")
        })?;
        self.detect_parameters(parameters)
    }

    pub fn detect_parameters(&self, parameters: ActionParameters) -> CoreResult<DetectionResponse> {
        let identity = self.identity_for_read()?;
        let action_id = parameters.action_id();
        let action = registered_action(action_id)?;
        CompatibilityCatalog::ensure_detect_allowed(&identity, action.metadata())
            .map_err(CoreError::from)?;
        let context = action_context(&identity, Uuid::nil(), Uuid::nil());
        let state = action
            .detect_current_state(&context, &parameters)
            .map_err(CoreError::from)?;
        Ok(DetectionResponse {
            action_id: action_id.as_str().to_owned(),
            state: state_to_ui(action.metadata(), state),
        })
    }

    pub fn preview(&self, request: PreviewActionsRequest) -> CoreResult<PreviewResponse> {
        if request.actions.is_empty() || request.actions.len() > MAX_ACTIONS_PER_TRANSACTION {
            return Err(CoreError::invalid_request(
                "1回の適用には1件以上32件以下のActionを指定してください。",
            ));
        }
        if self.journal.recovery_count()? > 0 {
            return Err(CoreError::recovery_required(
                "未復元項目を解決するまで、新しい適用は開始できません。",
            ));
        }
        let identity = self.identity_for_read()?;
        let invocations = order_and_validate_requests(
            request
                .actions
                .into_iter()
                .map(parse_action_request)
                .collect::<CoreResult<Vec<_>>>()?,
        )?;
        self.ensure_layout_invocations_current(&invocations)?;
        let mut before = HashMap::with_capacity(invocations.len());
        let mut changes = Vec::with_capacity(invocations.len());
        let mut warnings = Vec::new();
        for parameters in &invocations {
            let action = registered_action(parameters.action_id())?;
            let context = action_context(&identity, Uuid::nil(), Uuid::new_v4());
            action
                .validate(&context, parameters)
                .map_err(CoreError::from)?;
            let state = action
                .detect_current_state(&context, parameters)
                .map_err(CoreError::from)?;
            let explanation = action
                .explain_changes(parameters)
                .map_err(CoreError::from)?;
            if !action.metadata().auto_apply_eligible {
                warnings.push(if action.metadata().id == ActionId::SetupWindowLayout {
                    "ウィンドウ配置の復元は、登録ゲームへの操作を避けるため明示操作だけで実行します。"
                        .to_owned()
                } else {
                    format!(
                        "{} は実機スモーク完了まで無人適用には使いません。",
                        action.metadata().name
                    )
                });
            }
            before.insert(parameters.action_id(), state_fingerprint(&state)?);
            changes.push(preview_change(action.metadata(), &state, &explanation));
        }

        let token = random_token()?;
        let now = now_ms();
        let expires_at = now.saturating_add(PREVIEW_TTL_MS);
        let mut previews = self.previews.lock();
        previews.retain(|_, record| record.expires_at_ms >= now);
        previews.insert(
            token.clone(),
            PreviewRecord {
                invocations,
                before,
                os_fingerprint: os_identity_fingerprint(&identity)?,
                expires_at_ms: expires_at,
            },
        );
        Ok(PreviewResponse {
            preview_token: token,
            expires_at: format_timestamp(expires_at),
            os_build: identity.base_build,
            changes,
            warnings,
        })
    }

    /// Resolve backend-owned inputs before entering the ordinary preview path.
    ///
    /// The UI and persisted manual modes may select the registered window-layout
    /// Action, but they can never supply titles, HWNDs, executable identities, or
    /// a replacement snapshot. Those values come only from the private runtime
    /// stores and the resulting request still follows preview -> commit -> journal.
    pub fn preview_with_runtime_parameters(
        &self,
        mut request: PreviewActionsRequest,
    ) -> CoreResult<PreviewResponse> {
        for action in &mut request.actions {
            if action.action_id != ActionId::SetupWindowLayout.as_str() {
                continue;
            }
            if !action.parameters.is_empty() {
                return Err(CoreError::invalid_request(
                    "ウィンドウ配置の復元内容は保存済みデータからだけ作成できます。",
                ));
            }
            let ActionParameters::SetupWindowLayout { invocation } =
                self.window_layout_parameters()?
            else {
                unreachable!("window layout helper returns the matching variant");
            };
            action.parameters.insert(
                "invocation".to_owned(),
                serde_json::to_value(invocation).map_err(|_| CoreError::storage())?,
            );
        }
        self.preview(request)
    }

    pub fn list_timeline(&self, limit: u32) -> CoreResult<Vec<TimelineItem>> {
        let mut timeline = self.journal.list_timeline(limit)?;
        for item in &mut timeline {
            if let Ok(action_id) = item.action_id.parse::<ActionId>() {
                if let Some(action) = ACTION_REGISTRY.get(action_id) {
                    item.title = action.metadata().name.to_owned();
                    item.summary = action.metadata().description.to_owned();
                    if matches!(
                        action.metadata().kind,
                        ActionKind::Observation | ActionKind::OneWay
                    ) {
                        item.rollback_available = false;
                    }
                }
            }
        }
        Ok(timeline)
    }

    /// 適用した設定が今も残っているかを照合する。**read-only。**
    ///
    /// 基準は「このアプリで適用して、まだ戻していないもの」。
    /// 推測で「本来こうあるべき」を作らない。基準が無ければ無いと言う。
    pub fn build_health_report(&self) -> CoreResult<HealthReport> {
        let (applied, unreadable_backups) = self.journal.applied_backups()?;
        let mut inputs: Vec<HealthInput> = applied
            .into_iter()
            .map(|record| {
                let name = record
                    .action_id
                    .parse::<ActionId>()
                    .ok()
                    .and_then(|id| ACTION_REGISTRY.get(id))
                    .map(|action| action.metadata().name.to_owned())
                    .unwrap_or_else(|| "名前の分からない項目".to_owned());
                let (probe, uncomparable_reason) = probe_backup(&record.backup.payload);
                HealthInput {
                    action_id: record.action_id,
                    name,
                    applied_at: format_local_timestamp(record.applied_at_unix_ms),
                    probe,
                    uncomparable_reason,
                }
            })
            .collect();

        // 記録そのものが読めなかったものを、件数から消さない。
        // 消すと「全部確認できた」ように見える。
        for index in 0..unreadable_backups {
            inputs.push(HealthInput {
                action_id: format!("__unreadable_{index}"),
                name: "記録を読めなかった変更".to_owned(),
                applied_at: "不明".to_owned(),
                probe: HealthProbe::Uncomparable,
                uncomparable_reason: Some(
                    "この変更の控えを読み取れませんでした。今の状態と見比べられません。".to_owned(),
                ),
            });
        }

        // 更新の確認日時は参考として添えるだけ。取れなければ何も出さない。
        // 取れないことを「更新していない」と読み替えない。
        let last_checked = match crate::windows::read_windows_update_status() {
            Ok(status) => match status.last_checked_local {
                ReadinessComponent::Known { value } => Some(value),
                _ => None,
            },
            Err(_) => None,
        };
        Ok(build_health_report(inputs, last_checked.as_deref()))
    }

    fn identity_for_read(&self) -> CoreResult<OsIdentity> {
        self.initial_identity.clone().ok_or_else(|| {
            CoreError::recovery_required("Windows buildを確認できないため、状態を推測しません。")
        })
    }

    fn runtime_profile_store(&self) -> CoreResult<&ProfileStore> {
        self.profile_store.as_deref().ok_or_else(|| {
            CoreError::recovery_required(
                "プロファイル保存領域を初期化できなかったため、操作を停止しました。",
            )
        })
    }

    fn identity_for_commit(&self) -> CoreResult<OsIdentity> {
        #[cfg(test)]
        {
            self.identity_for_read()
        }
        #[cfg(not(test))]
        {
            let current = OsIdentity::load().map_err(|_| {
                CoreError::recovery_required("buildを再確認できないため、書き込みを停止しました。")
            })?;
            if let Some(initial) = &self.initial_identity {
                if os_identity_fingerprint(initial)? != os_identity_fingerprint(&current)? {
                    return Err(CoreError::recovery_required(
                        "起動後にWindows識別情報が変わりました。再起動してください。",
                    ));
                }
            }
            Ok(current)
        }
    }

    fn ensure_layout_invocations_current(
        &self,
        invocations: &[ActionParameters],
    ) -> CoreResult<()> {
        for parameters in invocations {
            let ActionParameters::SetupWindowLayout { invocation } = parameters else {
                continue;
            };
            if let Some(store) = &self.window_layout_store {
                let current_snapshot_id = store
                    .current_snapshot_id()
                    .map_err(|_| stale_window_layout_preview())?;
                if current_snapshot_id != invocation.desired.snapshot_id {
                    return Err(stale_window_layout_preview());
                }
            }
            if let Some(store) = &self.profile_store {
                let current = store.registered_game_file_identities()?;
                if current != invocation.excluded_game_file_identities {
                    return Err(stale_window_layout_preview());
                }
            }
        }
        Ok(())
    }

    fn recovery_parameters(&self, parameters: &ActionParameters) -> CoreResult<ActionParameters> {
        let ActionParameters::SetupWindowLayout { invocation } = parameters else {
            return Ok(parameters.clone());
        };
        let Some(profile_store) = &self.profile_store else {
            return Ok(parameters.clone());
        };
        let mut effective = invocation.clone();
        effective
            .excluded_game_file_identities
            .extend(profile_store.registered_game_file_identities()?);
        effective
            .excluded_game_file_identities
            .sort_by(|left, right| {
                left.volume_serial_number
                    .cmp(&right.volume_serial_number)
                    .then_with(|| left.file_id.cmp(&right.file_id))
            });
        effective.excluded_game_file_identities.dedup();
        effective.validate()?;
        Ok(ActionParameters::SetupWindowLayout {
            invocation: effective,
        })
    }
}

fn stale_window_layout_preview() -> CoreError {
    CoreError::new(
        "STALE_PREVIEW",
        "VALIDATE",
        false,
        "保存済みの配置または登録ゲームが変わりました。差分を確認し直してください。",
    )
}

fn registered_action(action_id: ActionId) -> CoreResult<&'static dyn Action> {
    ACTION_REGISTRY.get(action_id).ok_or_else(|| {
        CoreError::new(
            "ACTION_REGISTRY_INVALID",
            "VALIDATE",
            false,
            "登録済みActionを解決できないため、変更を停止しました。",
        )
    })
}

fn order_and_validate_requests(
    mut invocations: Vec<ActionParameters>,
) -> CoreResult<Vec<ActionParameters>> {
    let ids = invocations
        .iter()
        .map(ActionParameters::action_id)
        .collect::<BTreeSet<_>>();
    if ids.len() != invocations.len() {
        return Err(CoreError::invalid_request(
            "同じActionを重複指定できません。",
        ));
    }
    for invocation in &invocations {
        let action = registered_action(invocation.action_id())?;
        for dependency in action.metadata().dependencies {
            if !ids.contains(dependency) {
                return Err(CoreError::invalid_request(format!(
                    "{} の依存Actionが含まれていません。",
                    action.metadata().name
                )));
            }
        }
        if action
            .metadata()
            .conflicts
            .iter()
            .any(|conflict| ids.contains(conflict))
        {
            return Err(CoreError::invalid_request(format!(
                "{} と同時適用できないActionがあります。",
                action.metadata().name
            )));
        }
    }
    invocations.sort_by_key(ActionParameters::action_id);
    Ok(invocations)
}

fn classify_action(
    action: &'static dyn Action,
    context: &ActionContext<'_>,
    parameters: &ActionParameters,
    backup: &BackupEnvelope,
    item_state: ItemState,
) -> RecoveryClassification {
    match action.verify_rolled_back(context, parameters, backup) {
        Ok(Verification { verified: true, .. }) => return RecoveryClassification::Original,
        Ok(_) => {}
        Err(_) => return RecoveryClassification::Unknown,
    }
    match action.verify_applied(context, parameters, backup) {
        Ok(Verification { verified: true, .. }) => RecoveryClassification::Applied,
        Ok(Verification { observed, .. }) => {
            if parameters.action_id() == ActionId::SetupWindowLayout {
                return match crate::actions::classify_recoverable_window_layout(parameters, backup)
                {
                    Ok(
                        crate::windows::WindowLayoutTransactionState::Desired
                        | crate::windows::WindowLayoutTransactionState::MixedOwned
                        | crate::windows::WindowLayoutTransactionState::AppliedWithExternal,
                    ) => RecoveryClassification::Applied,
                    Ok(
                        crate::windows::WindowLayoutTransactionState::Original
                        | crate::windows::WindowLayoutTransactionState::OriginalWithExternal,
                    ) => RecoveryClassification::Original,
                    Ok(crate::windows::WindowLayoutTransactionState::Third) => {
                        RecoveryClassification::Third
                    }
                    Err(_) => RecoveryClassification::Unknown,
                };
            }
            if parameters.action_id() == ActionId::InputShiftInterruptionGuard {
                return match crate::actions::classify_recoverable_shift_guard(context, backup) {
                    Ok(state) => classify_shift_guard_recovery_state(
                        state,
                        item_state,
                        backup.applied_fingerprint.is_some(),
                    ),
                    Err(_) => RecoveryClassification::Unknown,
                };
            }
            match observed {
                DetectedState::Known { .. }
                | DetectedState::NeedsRestart { .. }
                | DetectedState::Conflict { .. } => RecoveryClassification::Third,
                _ => RecoveryClassification::Unknown,
            }
        }
        Err(_) => RecoveryClassification::Unknown,
    }
}

fn classify_shift_guard_recovery_state(
    state: crate::actions::ShiftGuardTransactionState,
    item_state: ItemState,
    applied_fingerprint_recorded: bool,
) -> RecoveryClassification {
    match state {
        crate::actions::ShiftGuardTransactionState::Original => RecoveryClassification::Original,
        crate::actions::ShiftGuardTransactionState::Desired => RecoveryClassification::Applied,
        crate::actions::ShiftGuardTransactionState::MixedOwned
            if item_state == ItemState::RollingBack
                || (!applied_fingerprint_recorded
                    && matches!(item_state, ItemState::Applying | ItemState::ApplyFailed)) =>
        {
            RecoveryClassification::Applied
        }
        crate::actions::ShiftGuardTransactionState::MixedOwned
        | crate::actions::ShiftGuardTransactionState::Third => RecoveryClassification::Third,
    }
}

fn rollback_action_from_persisted_state(
    action: &'static dyn Action,
    context: &ActionContext<'_>,
    parameters: &ActionParameters,
    backup: &BackupEnvelope,
    item_state: ItemState,
) -> ActionResult<RollbackEvidence> {
    if parameters.action_id() == ActionId::InputShiftInterruptionGuard
        && item_state == ItemState::RollingBack
    {
        crate::actions::rollback_recoverable_shift_guard(context, parameters, backup)
    } else {
        action.rollback(context, parameters, backup)
    }
}

/// Once a layout item has crossed PREPARED, an immediate Original read does
/// not prove that a synchronous SetWindowPlacement call was never entered
/// before a crash. Recovery reasserts the original placement and verifies it
/// before terminalizing the journal item.
fn needs_original_rollback_fence(action_id: ActionId, state: ItemState) -> bool {
    action_id == ActionId::SetupWindowLayout && state != ItemState::Prepared
}

fn ensure_backup_mutation_allowed(
    identity: &OsIdentity,
    action: &'static dyn Action,
    backup: &BackupEnvelope,
) -> CoreResult<()> {
    if backup.os_build != identity.base_build && !backup.rollback_across_unknown_build {
        return Err(CoreError::recovery_required(
            "backup作成時とbase buildが異なるため、自動復元を停止しました。",
        ));
    }
    CompatibilityCatalog::ensure_mutation_allowed(
        identity,
        action.metadata(),
        ActionStage::Recovery,
    )
    .map_err(CoreError::from)?;
    Ok(())
}

fn core_mutation_lock_error(error: crate::windows::WindowsError) -> CoreError {
    let retryable = error.kind == crate::windows::WindowsErrorKind::ResourceLimit;
    CoreError::new(
        "MUTATION_LOCK_FAILURE",
        "LOCK_RESOURCES",
        retryable,
        "別のPCカスタム処理が進行中か、安全な排他を取得できませんでした。変更していません。",
    )
}

fn window_layout_windows_error(error: crate::windows::WindowsError) -> CoreError {
    let retryable = error.kind == crate::windows::WindowsErrorKind::ResourceLimit;
    CoreError::new(
        "WINDOW_LAYOUT_API_FAILURE",
        "WINDOW_LAYOUT",
        retryable,
        "ウィンドウ配置を安全に確認できませんでした。対象は変更していません。",
    )
}

fn offscreen_window_rescue_error(error: crate::windows::WindowsError) -> CoreError {
    match error.kind {
        crate::windows::WindowsErrorKind::ExternalConflict => CoreError::new(
            "OFFSCREEN_WINDOW_CHANGED",
            "WINDOW_RESCUE",
            true,
            "確認後に対象の状態が変わったため、ウィンドウを動かしませんでした。もう一度探してください。",
        ),
        crate::windows::WindowsErrorKind::AccessDenied => CoreError::new(
            "OFFSCREEN_WINDOW_UNAVAILABLE",
            "WINDOW_RESCUE",
            false,
            "このウィンドウは権限または応答状態のため、PCカスタムから動かせません。",
        ),
        crate::windows::WindowsErrorKind::RecoveryRequired => CoreError::recovery_required(
            "ウィンドウ移動後の状態を確認できませんでした。対象アプリの位置を確認してください。",
        ),
        crate::windows::WindowsErrorKind::ResourceLimit => CoreError::new(
            "OFFSCREEN_WINDOW_LIMIT",
            "WINDOW_RESCUE",
            true,
            "対象が多いため、安全な上限内で処理できませんでした。",
        ),
        _ => CoreError::new(
            "OFFSCREEN_WINDOW_API_FAILURE",
            "WINDOW_RESCUE",
            true,
            "ウィンドウの状態を安全に確認できませんでした。対象は成功として扱われていません。",
        ),
    }
}

fn state_fingerprint(state: &DetectedState) -> CoreResult<Fingerprint> {
    state.stable_fingerprint().map_err(|_| {
        CoreError::new(
            "SERIALIZATION_FAILURE",
            "VALIDATE",
            false,
            "状態のfingerprintを作成できませんでした。",
        )
    })
}

fn os_identity_fingerprint(identity: &OsIdentity) -> CoreResult<Fingerprint> {
    fingerprint_of(&(
        identity.major,
        identity.minor,
        identity.base_build,
        identity.revision,
        identity.operating_system_sku,
        identity.product_type,
        identity.architecture,
        identity.source,
    ))
}

fn action_context(identity: &OsIdentity, transaction_id: Uuid, item_id: Uuid) -> ActionContext<'_> {
    ActionContext {
        os_identity: identity,
        transaction_id,
        item_id,
        observed_at_unix_ms: now_ms(),
        is_elevated: false,
    }
}

fn fingerprint_of<T: serde::Serialize>(value: &T) -> CoreResult<Fingerprint> {
    serde_json::to_vec(value)
        .map(|bytes| Fingerprint::of_bytes(&bytes))
        .map_err(|_| {
            CoreError::new(
                "SERIALIZATION_FAILURE",
                "VALIDATE",
                false,
                "状態のfingerprintを作成できませんでした。",
            )
        })
}

fn random_token() -> CoreResult<String> {
    let mut bytes = [0u8; 32];
    OsRng.try_fill_bytes(&mut bytes).map_err(|_| {
        CoreError::new(
            "RANDOM_SOURCE_FAILURE",
            "VALIDATE",
            false,
            "安全なプレビューtokenを作成できませんでした。",
        )
    })?;
    Ok(hex::encode(bytes))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn format_timestamp(unix_ms: u64) -> String {
    let value = i64::try_from(unix_ms).unwrap_or(i64::MAX);
    DateTime::<Utc>::from_timestamp_millis(value)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        .to_rfc3339()
}

/// バックアップに残した実際のバイト列と、いまのレジストリ値を突き合わせる。
///
/// 判定を混ぜない: 1つでも読めなければ「確認できません」に落とす。
/// 読めた分だけで「基準どおり」と言うほうが、黙って見落とすより悪い。
fn probe_backup(payload: &BackupPayload) -> (HealthProbe, Option<String>) {
    let entries = match payload {
        BackupPayload::Registry(backup) => vec![backup],
        BackupPayload::Composite(composite) => composite.registry_entries.iter().collect(),
        // 電源モードとポインター設定は、控えに元の値と適用値の両方が入っていて、
        // 現在値も公開 API で読み直せる。レジストリではないが照合できる。
        BackupPayload::PowerMode(backup) => return probe_power_mode(backup),
        BackupPayload::PointerFeel(backup) => return probe_pointer_feel(backup),
        BackupPayload::CommsMicMute(backup) => return probe_comms_mic_mute(backup),
        // それ以外（セッション設定やウィンドウ配置）は、
        // 「適用したときの値」を今の環境から読み直す手段がない。無いものを在るとは言わない。
        _ => Vec::new(),
    };
    if entries.is_empty() {
        return (
            HealthProbe::Uncomparable,
            Some("この項目は、あとから見比べるしくみがありません。".to_owned()),
        );
    }

    let mut counts = ProbeCounts::default();
    for entry in entries {
        match classify_registry_backup(entry) {
            Ok(RegistryClassification::Applied) => counts.holds += 1,
            // 値が消えている場合も「適用前に戻った」側。作られた空のキーは値ではない。
            Ok(RegistryClassification::Original)
            | Ok(RegistryClassification::CreatedKeyWithoutValue) => counts.previous += 1,
            Ok(RegistryClassification::Third) => counts.neither += 1,
            Err(_) => {
                return (
                    HealthProbe::Uncomparable,
                    Some("いまの状態を読み取れませんでした。".to_owned()),
                )
            }
        }
    }
    (combine_probe(counts), None)
}

fn probe_comms_mic_mute(
    backup: &crate::backup::CommsMicMuteBackup,
) -> (HealthProbe, Option<String>) {
    let current = match crate::windows::read_comms_mic_mute_by_id(&backup.original.device_id) {
        Ok(current) => current,
        Err(_) => {
            return (
                HealthProbe::Uncomparable,
                Some("保存した通話マイクの状態を読み取れませんでした。".to_owned()),
            )
        }
    };
    if current == backup.intended {
        (HealthProbe::HoldsApplied, None)
    } else if current == backup.original {
        (HealthProbe::BackToPrevious, None)
    } else {
        (
            HealthProbe::Neither,
            Some("保存後に通話マイクのmute設定が別のものから変更されています。".to_owned()),
        )
    }
}

/// 電源モードを照合する。控えの生バイト列と、いまの値を突き合わせる。
fn probe_power_mode(backup: &crate::backup::PowerModeBackup) -> (HealthProbe, Option<String>) {
    let (Ok(ac), Ok(dc)) = (
        crate::windows::read_ac_mode(),
        crate::windows::read_dc_mode(),
    ) else {
        return (
            HealthProbe::Uncomparable,
            Some("いまの状態を読み取れませんでした。".to_owned()),
        );
    };
    let bytes = |reading: crate::windows::PowerModeReading| match reading {
        crate::windows::PowerModeReading::Known(mode) => Some(mode.bytes()),
        crate::windows::PowerModeReading::Unrecognised(raw) => Some(raw),
        crate::windows::PowerModeReading::Unavailable => None,
    };
    let (Some(current_ac), Some(current_dc)) = (bytes(ac), bytes(dc)) else {
        return (
            HealthProbe::Uncomparable,
            Some("この環境ではこの項目を確認できません。".to_owned()),
        );
    };
    let intended = match backup.intended {
        crate::action::PowerModeChoice::BestEfficiency => crate::windows::PowerMode::BestEfficiency,
        crate::action::PowerModeChoice::Balanced => crate::windows::PowerMode::Balanced,
        crate::action::PowerModeChoice::BestPerformance => {
            crate::windows::PowerMode::BestPerformance
        }
    }
    .bytes();
    let mut counts = ProbeCounts::default();
    for (current, original) in [
        (current_ac, backup.original_ac),
        (current_dc, backup.original_dc),
    ] {
        if current == intended {
            counts.holds += 1;
        } else if current == original {
            counts.previous += 1;
        } else {
            counts.neither += 1;
        }
    }
    (combine_probe(counts), None)
}

/// ポインター設定を照合する。
fn probe_pointer_feel(backup: &crate::backup::PointerFeelBackup) -> (HealthProbe, Option<String>) {
    let Ok(current) = crate::windows::read_pointer_feel() else {
        return (
            HealthProbe::Uncomparable,
            Some("いまの状態を読み取れませんでした。".to_owned()),
        );
    };
    if current == backup.intended {
        (HealthProbe::HoldsApplied, None)
    } else if current == backup.original {
        (HealthProbe::BackToPrevious, None)
    } else {
        (HealthProbe::Neither, None)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ProbeCounts {
    holds: usize,
    previous: usize,
    neither: usize,
}

/// 1つの Action が複数のレジストリ値にまたがるとき、全体をどう言うか。
///
/// 一部だけ戻っている状態を「適用したときのまま」とは呼べないので、
/// 混ざっていたら「どちらとも違う」に倒す。**都合のよい側へ丸めない。**
fn combine_probe(counts: ProbeCounts) -> HealthProbe {
    if counts.neither > 0 || (counts.holds > 0 && counts.previous > 0) {
        HealthProbe::Neither
    } else if counts.previous > 0 {
        HealthProbe::BackToPrevious
    } else {
        HealthProbe::HoldsApplied
    }
}

/// 実機の記録に対して照合を1回走らせ、数を出す。
///
/// 単体テストは合成ロジックしか見ていない。SQL が本当に行を返すか、
/// バックアップのバイト列が本当に現在のレジストリと突き合わせられるかは、
/// **実際の記録に当てないと分からない。** 読むだけなので何も変更しない。
#[cfg(all(test, windows))]
mod real_journal_probe {
    use super::*;
    use std::path::PathBuf;

    #[test]
    #[ignore = "実機の記録が要る"]
    fn reports_health_probes_when_a_real_journal_baseline_exists() {
        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            println!("EVIDENCE: health_report measured=false reason=LOCALAPPDATAなし");
            return;
        };
        let path = PathBuf::from(local)
            .join("PCCustom")
            .join("data")
            .join("pc-custom.db");
        if !path.exists() {
            println!("EVIDENCE: health_report measured=false reason=記録がまだ無い");
            return;
        }
        // 実物を開かない。`JournalDatabase::open` は読み書きで開くので、
        // 利用者の変更記録に検証の都合で触れる余地を残さない。複製を作って、そちらを開く。
        let copy = std::env::temp_dir().join("pc-custom-health-probe-copy.db");
        let _ = std::fs::remove_file(&copy);
        std::fs::copy(&path, &copy).expect("copy journal");
        let journal = JournalDatabase::open(&copy).expect("open journal copy");
        let (applied, unreadable) = journal.applied_backups().expect("applied backups");
        println!(
            "EVIDENCE: health_report measured={} baselines={} unreadable_records={}",
            !applied.is_empty(),
            applied.len(),
            unreadable
        );
        // 0件は「基準が無い」であって「確認した結果ゼロ」ではない。区別して出す。
        if applied.is_empty() {
            println!(
                "EVIDENCE: health_report measured=false reason=基準が1件も無く照合経路は未実行"
            );
        }
        for record in &applied {
            let (probe, reason) = probe_backup(&record.backup.payload);
            println!(
                "EVIDENCE: health_report action={} probe={:?} reason={:?}",
                record.action_id, probe, reason
            );
            // 「違う」が正しいのかを、判定関数を通さずに1つずつ読み直して確かめる。
            let entries: Vec<&crate::backup::RegistryBackup> = match &record.backup.payload {
                BackupPayload::Registry(backup) => vec![backup],
                BackupPayload::Composite(composite) => composite.registry_entries.iter().collect(),
                _ => Vec::new(),
            };
            for entry in entries {
                let current = crate::backup::read_registry_state(&entry.location);
                println!(
                    "EVIDENCE: health_report   entry class={:?} current={:?}",
                    classify_registry_backup(entry),
                    current.map(|state| (state.value_existed, state.raw_bytes))
                );
            }
        }
    }
}

#[cfg(test)]
mod probe_combination_tests {
    use super::*;

    fn counts(holds: usize, previous: usize, neither: usize) -> ProbeCounts {
        ProbeCounts {
            holds,
            previous,
            neither,
        }
    }

    #[test]
    fn every_value_still_applied_reads_as_applied() {
        assert_eq!(combine_probe(counts(3, 0, 0)), HealthProbe::HoldsApplied);
    }

    #[test]
    fn every_value_back_to_the_previous_one_reads_as_reverted() {
        assert_eq!(combine_probe(counts(0, 2, 0)), HealthProbe::BackToPrevious);
    }

    #[test]
    fn a_partial_revert_is_not_reported_as_healthy() {
        // 2つのうち1つだけ戻っている。ここを「そのまま」と言うと見落としになる。
        assert_eq!(combine_probe(counts(1, 1, 0)), HealthProbe::Neither);
    }

    #[test]
    fn a_third_value_anywhere_wins_over_the_rest() {
        assert_eq!(combine_probe(counts(5, 0, 1)), HealthProbe::Neither);
        assert_eq!(combine_probe(counts(0, 5, 1)), HealthProbe::Neither);
    }
}

/// 画面に出す時刻。秒までは要らない。
fn format_local_timestamp(unix_ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(unix_ms)
        .map(|value| value.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "時刻不明".to_owned())
}

#[cfg(test)]
mod original_rollback_fence_tests {
    use super::*;

    #[test]
    fn prepared_layout_has_no_dispatch_but_every_later_state_requires_a_fence() {
        assert!(!needs_original_rollback_fence(
            ActionId::SetupWindowLayout,
            ItemState::Prepared
        ));
        for state in [
            ItemState::Applying,
            ItemState::Applied,
            ItemState::ApplyFailed,
            ItemState::RollingBack,
            ItemState::RollbackFailed,
            ItemState::RecoveryRequired,
        ] {
            assert!(needs_original_rollback_fence(
                ActionId::SetupWindowLayout,
                state
            ));
        }
        assert!(!needs_original_rollback_fence(
            ActionId::PowerActiveSchemeCheck,
            ItemState::Applying
        ));
    }
}
