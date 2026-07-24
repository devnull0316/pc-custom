mod recovery;
mod transaction;
#[cfg(all(test, windows))]
mod tests;


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
        ActionParameters, ActionStage, DetectedState, Verification, ACTION_REGISTRY,
    },
    backup::{BackupEnvelope, Fingerprint},
    compatibility::{CompatibilityCatalog, CompatibilityMode, OsIdentity},
    error::{CoreError, CoreResult},
    journal::{JournalDatabase, ReconcileResult, RecoveryClassification, TimelineItem},
    presentation::{
        action_presentation, default_parameters, listing_parameters, os_label,
        parse_action_request, preview_change, state_to_ui, ActionPresentation, BootstrapStatus,
        CommitResult, DetectionResponse, PreviewActionsRequest, PreviewResponse,
    },
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

pub struct TotonoeEngine {
    journal: Arc<JournalDatabase>,
    initial_identity: Option<OsIdentity>,
    previews: Mutex<HashMap<String, PreviewRecord>>,
    mutation_gate: Mutex<()>,
}

impl TotonoeEngine {
    pub fn new(
        journal: Arc<JournalDatabase>,
        initial_identity: Option<OsIdentity>,
    ) -> CoreResult<Self> {
        ACTION_REGISTRY.validate().map_err(CoreError::from)?;
        let engine = Self {
            journal,
            initial_identity,
            previews: Mutex::new(HashMap::new()),
            mutation_gate: Mutex::new(()),
        };
        engine.reconcile_now()?;
        Ok(engine)
    }

    pub fn bootstrap_status(&self) -> CoreResult<BootstrapStatus> {
        let recovery_count = self.journal.recovery_count()?;
        let Some(identity) = &self.initial_identity else {
            return Ok(BootstrapStatus {
                mode: "recovery_required".to_owned(),
                os_label: "Windowsの情報を確認できません".to_owned(),
                build: None,
                message: "buildを一意に確認できないため、自動書き込みを停止しています。"
                    .to_owned(),
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
                CompatibilityMode::TestedMutable => (
                    "ready",
                    "変更前を保存し、適用後と復元後を検証します。",
                ),
                CompatibilityMode::TestedDetectOnly => (
                    "read_only",
                    "実機承認前のbuildのため、状態確認だけを利用できます。",
                ),
                CompatibilityMode::Unsupported => (
                    "unsupported",
                    "このWindows releaseは変更対象外です。",
                ),
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
                        action.detect_current_state(&context, &parameters).unwrap_or_else(|error| {
                            DetectedState::Error {
                                code: error.code.as_code().to_owned(),
                                reason: "状態を安全に確認できませんでした。".to_owned(),
                            }
                        })
                    })
                });
                action_presentation(metadata, compatibility, current_state)
            })
            .collect()
    }

    pub fn detect_action(&self, action_id: ActionId) -> CoreResult<DetectionResponse> {
        let identity = self.identity_for_read()?;
        let action = registered_action(action_id)?;
        let parameters = default_parameters(action_id).ok_or_else(|| {
            CoreError::invalid_request("ゲーム監視には確認済みの実行ファイルbindingが必要です。")
        })?;
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
        let mut before = HashMap::with_capacity(invocations.len());
        let mut changes = Vec::with_capacity(invocations.len());
        let mut warnings = Vec::new();
        for parameters in &invocations {
            let action = registered_action(parameters.action_id())?;
            let context = action_context(&identity, Uuid::nil(), Uuid::new_v4());
            action.validate(&context, parameters).map_err(CoreError::from)?;
            let state = action
                .detect_current_state(&context, parameters)
                .map_err(CoreError::from)?;
            let explanation = action
                .explain_changes(parameters)
                .map_err(CoreError::from)?;
            if !action.metadata().auto_apply_eligible {
                warnings.push(format!(
                    "{} は実機スモーク完了まで無人適用には使いません。",
                    action.metadata().name
                ));
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

    pub fn list_timeline(&self, limit: u32) -> CoreResult<Vec<TimelineItem>> {
        let mut timeline = self.journal.list_timeline(limit)?;
        for item in &mut timeline {
            if let Ok(action_id) = item.action_id.parse::<ActionId>() {
                if let Some(action) = ACTION_REGISTRY.get(action_id) {
                    item.title = action.metadata().name.to_owned();
                    item.summary = action.metadata().description.to_owned();
                    if action.metadata().kind == ActionKind::Observation {
                        item.rollback_available = false;
                    }
                }
            }
        }
        Ok(timeline)
    }

    fn identity_for_read(&self) -> CoreResult<OsIdentity> {
        self.initial_identity.clone().ok_or_else(|| {
            CoreError::recovery_required("Windows buildを確認できないため、状態を推測しません。")
        })
    }

    fn identity_for_commit(&self) -> CoreResult<OsIdentity> {
        #[cfg(test)]
        {
            return self.identity_for_read();
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
        return Err(CoreError::invalid_request("同じActionを重複指定できません。"));
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
) -> RecoveryClassification {
    match action.verify_rolled_back(context, parameters, backup) {
        Ok(Verification { verified: true, .. }) => return RecoveryClassification::Original,
        Ok(_) => {}
        Err(_) => return RecoveryClassification::Unknown,
    }
    match action.verify_applied(context, parameters, backup) {
        Ok(Verification { verified: true, .. }) => RecoveryClassification::Applied,
        Ok(Verification { observed, .. }) => match observed {
            DetectedState::Known { .. }
            | DetectedState::NeedsRestart { .. }
            | DetectedState::Conflict { .. } => RecoveryClassification::Third,
            _ => RecoveryClassification::Unknown,
        },
        Err(_) => RecoveryClassification::Unknown,
    }
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
        "別のTotonoe処理が進行中か、安全な排他を取得できませんでした。変更していません。",
    )
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

fn action_context(
    identity: &OsIdentity,
    transaction_id: Uuid,
    item_id: Uuid,
) -> ActionContext<'_> {
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
        .map_err(|_| CoreError::new(
            "SERIALIZATION_FAILURE",
            "VALIDATE",
            false,
            "状態のfingerprintを作成できませんでした。",
        ))
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
