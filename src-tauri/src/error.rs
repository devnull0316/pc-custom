use std::{error::Error, fmt};

use serde::Serialize;
use uuid::Uuid;

use crate::action::{ActionError, ActionErrorCode};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreError {
    pub code: String,
    pub stage: String,
    pub retryable: bool,
    pub user_message: String,
    pub diagnostic_id: Uuid,
}

impl CoreError {
    pub fn new(
        code: impl Into<String>,
        stage: impl Into<String>,
        retryable: bool,
        user_message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            stage: stage.into(),
            retryable,
            user_message: user_message.into(),
            diagnostic_id: Uuid::new_v4(),
        }
    }

    pub fn storage() -> Self {
        Self::new(
            "STORAGE_FAILURE",
            "JOURNAL",
            false,
            "変更記録を安全に保存できませんでした。設定は変更していません。",
        )
    }

    pub fn recovery_required(message: impl Into<String>) -> Self {
        Self::new(
            "RECOVERY_REQUIRED",
            "RECOVERY",
            false,
            message,
        )
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new("INVALID_REQUEST", "VALIDATE", false, message)
    }
}

impl From<ActionError> for CoreError {
    fn from(error: ActionError) -> Self {
        let user_message = match error.code {
            ActionErrorCode::UnknownBuild | ActionErrorCode::RecoveryRequired => {
                "未検証のWindowsビルドです。自動書き込みを止め、復旧確認が必要です。"
            }
            ActionErrorCode::CompatibilityBlocked => {
                "このWindows環境では、この変更は読み取り専用です。"
            }
            ActionErrorCode::WrongParameters | ActionErrorCode::InvalidParameters => {
                "指定内容を安全に検証できませんでした。"
            }
            ActionErrorCode::ExternalConflict | ActionErrorCode::BackupMismatch => {
                "適用後に別の変更を検出したため、自動では上書きしません。"
            }
            ActionErrorCode::AccessDenied => "Windowsからこの操作が拒否されました。",
            ActionErrorCode::StateUnknown => {
                "現在の状態を確認できないため、変更を停止しました。"
            }
            _ => "Windows操作を完了できませんでした。変更履歴を確認してください。",
        };
        Self {
            code: error.code.as_code().to_owned(),
            stage: error.stage.as_code().to_owned(),
            retryable: error.retryable,
            user_message: user_message.to_owned(),
            diagnostic_id: error.diagnostic_id,
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} ({}/{}, diagnostic {})",
            self.user_message, self.stage, self.code, self.diagnostic_id
        )
    }
}

impl Error for CoreError {}

pub type CoreResult<T> = Result<T, CoreError>;
