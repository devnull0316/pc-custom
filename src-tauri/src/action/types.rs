use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{backup::Fingerprint, compatibility::OsIdentity};

use super::{ActionId, DetectedState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionStage {
    Detect,
    Validate,
    Backup,
    Apply,
    VerifyApplied,
    Rollback,
    VerifyRolledBack,
    Recovery,
}

impl ActionStage {
    pub const fn as_code(self) -> &'static str {
        match self {
            Self::Detect => "DETECT",
            Self::Validate => "VALIDATE",
            Self::Backup => "BACKUP",
            Self::Apply => "APPLY",
            Self::VerifyApplied => "VERIFY_APPLIED",
            Self::Rollback => "ROLLBACK",
            Self::VerifyRolledBack => "VERIFY_ROLLED_BACK",
            Self::Recovery => "RECOVERY",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionErrorCode {
    UnsupportedPlatform,
    UnknownBuild,
    CompatibilityBlocked,
    WrongParameters,
    InvalidParameters,
    BackupRequired,
    BackupMismatch,
    ExternalConflict,
    DisplayTopologyChanged,
    SavedDisplayTopologyMissing,
    GuidedRequired,
    WindowsApiFailure,
    AccessDenied,
    ResourceLimit,
    StateUnknown,
    LeaseFailure,
    InternalInvariant,
    RecoveryRequired,
}

impl ActionErrorCode {
    pub const fn as_code(self) -> &'static str {
        match self {
            Self::UnsupportedPlatform => "UNSUPPORTED_PLATFORM",
            Self::UnknownBuild => "UNKNOWN_BUILD",
            Self::CompatibilityBlocked => "COMPATIBILITY_BLOCKED",
            Self::WrongParameters => "WRONG_PARAMETERS",
            Self::InvalidParameters => "INVALID_PARAMETERS",
            Self::BackupRequired => "BACKUP_REQUIRED",
            Self::BackupMismatch => "BACKUP_MISMATCH",
            Self::ExternalConflict => "EXTERNAL_CONFLICT",
            Self::DisplayTopologyChanged => "DISPLAY_TOPOLOGY_CHANGED",
            Self::SavedDisplayTopologyMissing => "SAVED_DISPLAY_TOPOLOGY_MISSING",
            Self::GuidedRequired => "GUIDED_REQUIRED",
            Self::WindowsApiFailure => "WINDOWS_API_FAILURE",
            Self::AccessDenied => "ACCESS_DENIED",
            Self::ResourceLimit => "RESOURCE_LIMIT",
            Self::StateUnknown => "STATE_UNKNOWN",
            Self::LeaseFailure => "LEASE_FAILURE",
            Self::InternalInvariant => "INTERNAL_INVARIANT",
            Self::RecoveryRequired => "RECOVERY_REQUIRED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionError {
    pub code: ActionErrorCode,
    pub stage: ActionStage,
    pub retryable: bool,
    pub user_message_key: String,
    pub diagnostic_id: Uuid,
    /// Safe, bounded diagnostic text. Paths, command lines and raw OS output are forbidden.
    pub safe_detail: Option<String>,
}

impl ActionError {
    pub fn new(
        code: ActionErrorCode,
        stage: ActionStage,
        retryable: bool,
        user_message_key: impl Into<String>,
    ) -> Self {
        Self {
            code,
            stage,
            retryable,
            user_message_key: user_message_key.into(),
            diagnostic_id: Uuid::new_v4(),
            safe_detail: None,
        }
    }

    pub fn with_safe_detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        self.safe_detail = Some(detail.chars().take(256).collect());
        self
    }

    pub fn recovery_required(stage: ActionStage, message_key: &'static str) -> Self {
        Self::new(ActionErrorCode::RecoveryRequired, stage, false, message_key)
    }
}

impl fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} ({}/{}, diagnostic {})",
            self.user_message_key,
            self.stage.as_code(),
            self.code.as_code(),
            self.diagnostic_id
        )
    }
}

impl Error for ActionError {}

pub type ActionResult<T> = Result<T, ActionError>;

#[derive(Debug, Clone, Copy)]
pub struct ActionContext<'a> {
    pub os_identity: &'a OsIdentity,
    pub transaction_id: Uuid,
    pub item_id: Uuid,
    pub observed_at_unix_ms: u64,
    pub is_elevated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub valid: bool,
    pub warnings: Vec<String>,
    pub resource_keys: Vec<String>,
}

impl ValidationReport {
    pub fn valid(resource_keys: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            valid: true,
            warnings: Vec::new(),
            resource_keys: resource_keys.into_iter().map(str::to_owned).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedEvidence {
    pub state: DetectedState,
    pub applied_fingerprint: Fingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackEvidence {
    pub state: DetectedState,
    pub restored_fingerprint: Fingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    pub verified: bool,
    pub observed: DetectedState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeExplanation {
    pub action_id: ActionId,
    pub result: String,
    pub method: String,
    pub resources: Vec<String>,
    pub requires_admin: bool,
    pub requires_restart: bool,
    pub windows_update_impact: String,
    pub rollback_scope: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TroubleshootingStep {
    pub message_key: &'static str,
    pub opens_official_settings: bool,
}
