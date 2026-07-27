use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    action::{ActionId, ActionParameters},
    backup::BackupEnvelope,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransactionState {
    Planned,
    Preflighting,
    Prepared,
    Applying,
    Applied,
    Succeeded,
    RollingBack,
    RolledBack,
    RollbackFailed,
    RecoveryRequired,
}

impl TransactionState {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Planned => "PLANNED",
            Self::Preflighting => "PREFLIGHTING",
            Self::Prepared => "PREPARED",
            Self::Applying => "APPLYING",
            Self::Applied => "APPLIED",
            Self::Succeeded => "SUCCEEDED",
            Self::RollingBack => "ROLLING_BACK",
            Self::RolledBack => "ROLLED_BACK",
            Self::RollbackFailed => "ROLLBACK_FAILED",
            Self::RecoveryRequired => "RECOVERY_REQUIRED",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "PLANNED" => Self::Planned,
            "PREFLIGHTING" => Self::Preflighting,
            "PREPARED" => Self::Prepared,
            "APPLYING" => Self::Applying,
            "APPLIED" => Self::Applied,
            "SUCCEEDED" => Self::Succeeded,
            "ROLLING_BACK" => Self::RollingBack,
            "ROLLED_BACK" => Self::RolledBack,
            "ROLLBACK_FAILED" => Self::RollbackFailed,
            "RECOVERY_REQUIRED" => Self::RecoveryRequired,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ItemState {
    Prepared,
    Applying,
    Applied,
    ApplyFailed,
    RollingBack,
    RolledBack,
    RollbackFailed,
    RecoveryRequired,
}

impl ItemState {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Prepared => "PREPARED",
            Self::Applying => "APPLYING",
            Self::Applied => "APPLIED",
            Self::ApplyFailed => "APPLY_FAILED",
            Self::RollingBack => "ROLLING_BACK",
            Self::RolledBack => "ROLLED_BACK",
            Self::RollbackFailed => "ROLLBACK_FAILED",
            Self::RecoveryRequired => "RECOVERY_REQUIRED",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        Some(match value {
            "PREPARED" => Self::Prepared,
            "APPLYING" => Self::Applying,
            "APPLIED" => Self::Applied,
            "APPLY_FAILED" => Self::ApplyFailed,
            "ROLLING_BACK" => Self::RollingBack,
            "ROLLED_BACK" => Self::RolledBack,
            "ROLLBACK_FAILED" => Self::RollbackFailed,
            "RECOVERY_REQUIRED" => Self::RecoveryRequired,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryClassification {
    Original,
    Applied,
    Third,
    Unknown,
}

impl RecoveryClassification {
    pub const fn as_db(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Applied => "applied",
            Self::Third => "third",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PreparedItem {
    pub item_id: Uuid,
    pub ordinal: u32,
    pub action_id: ActionId,
    pub action_version: u32,
    pub parameters: ActionParameters,
    pub resource_keys: Vec<String>,
    pub backup: BackupEnvelope,
}

#[derive(Debug, Clone)]
pub struct PersistedItem {
    pub item_id: Uuid,
    pub transaction_id: Uuid,
    pub ordinal: u32,
    pub apply_order: Option<u32>,
    pub action_id: ActionId,
    pub action_version: u32,
    pub parameters: ActionParameters,
    pub state: ItemState,
    pub backup: BackupEnvelope,
    pub error_code: Option<String>,
    pub diagnostic_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct RecoveryTransaction {
    pub transaction_id: Uuid,
    pub state: TransactionState,
    pub os_fingerprint: String,
    pub items: Vec<PersistedItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineStage {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineItem {
    pub item_id: Uuid,
    pub transaction_id: Uuid,
    pub action_id: String,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub started_at: String,
    pub before: String,
    pub after: String,
    pub rollback_available: bool,
    pub retry_available: bool,
    pub stages: Vec<TimelineStage>,
    pub diagnostic_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileResult {
    pub status: String,
    pub recovered_count: u32,
    pub remaining_count: u32,
    pub message: String,
}
