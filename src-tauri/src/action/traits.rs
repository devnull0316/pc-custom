use crate::backup::{BackupDraft, BackupEnvelope};

use super::{
    ActionContext, ActionErrorCode, ActionMetadata, ActionParameters, ActionResult,
    AppliedEvidence, ChangeExplanation, DetectedState, RollbackEvidence, TroubleshootingStep,
    ValidationReport, Verification,
};

/// A first-party, compile-time registered unit of detection, mutation and exact restoration.
pub trait Action: Sync {
    fn metadata(&self) -> &'static ActionMetadata;

    fn detect_current_state(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<DetectedState>;

    /// Must not mutate Windows state.
    fn validate(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<ValidationReport>;

    /// Produces a lossless draft. The coordinator must durably commit it before `apply`.
    fn create_backup(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
    ) -> ActionResult<BackupDraft>;

    fn apply(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<AppliedEvidence>;

    fn verify_applied(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<Verification>;

    fn rollback(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<RollbackEvidence>;

    fn verify_rolled_back(
        &self,
        context: &ActionContext<'_>,
        parameters: &ActionParameters,
        backup: &BackupEnvelope,
    ) -> ActionResult<Verification>;

    fn explain_changes(
        &self,
        parameters: &ActionParameters,
    ) -> ActionResult<ChangeExplanation>;

    fn troubleshooting(&self, code: ActionErrorCode) -> &'static [TroubleshootingStep];
}
