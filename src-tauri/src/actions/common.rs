use crate::{
    action::{
        ActionContext, ActionError, ActionErrorCode, ActionMetadata, ActionParameters,
        ActionResult, ActionStage, DetectedState, StateEvidence, ValidationReport,
    },
    backup::{BackupEnvelope, Fingerprint, RegistryBackup},
    compatibility::CompatibilityCatalog,
    windows::{WindowsError, WindowsErrorKind},
};

pub const REG_DWORD_TYPE: u32 = 4;

pub fn validate_base(
    metadata: &'static ActionMetadata,
    context: &ActionContext<'_>,
    parameters: &ActionParameters,
    mutation: bool,
    stage: ActionStage,
) -> ActionResult<ValidationReport> {
    if parameters.action_id() != metadata.id {
        return Err(ActionError::new(
            ActionErrorCode::WrongParameters,
            stage,
            false,
            "action.parameters.id_mismatch",
        ));
    }
    if metadata.requiresAdmin && !context.is_elevated {
        return Err(ActionError::new(
            ActionErrorCode::AccessDenied,
            stage,
            false,
            "action.permissions.administrator_required",
        ));
    }
    if mutation {
        CompatibilityCatalog::ensure_mutation_allowed(context.os_identity, metadata, stage)?;
    } else {
        CompatibilityCatalog::ensure_detect_allowed(context.os_identity, metadata)?;
    }
    Ok(ValidationReport::valid(
        metadata.resource_keys.iter().copied(),
    ))
}

pub fn validate_backup(
    metadata: &'static ActionMetadata,
    _context: &ActionContext<'_>,
    backup: &BackupEnvelope,
    stage: ActionStage,
) -> ActionResult<()> {
    if !backup.verify_integrity()
        || backup.action_id != metadata.id
        || backup.action_version != metadata.action_version
        || !metadata
            .rollback_decoder_versions
            .contains(&backup.codec_version)
    {
        return Err(ActionError::recovery_required(
            stage,
            "action.backup.integrity_or_identity_mismatch",
        ));
    }
    Ok(())
}

pub fn validate_backup_for_apply(
    metadata: &'static ActionMetadata,
    context: &ActionContext<'_>,
    backup: &BackupEnvelope,
) -> ActionResult<()> {
    validate_backup(metadata, context, backup, ActionStage::Apply)?;
    if backup.transaction_id != context.transaction_id || backup.item_id != context.item_id {
        return Err(ActionError::recovery_required(
            ActionStage::Apply,
            "action.backup.not_prepared_for_current_item",
        ));
    }
    if backup.os_build != context.os_identity.base_build {
        return Err(ActionError::recovery_required(
            ActionStage::Apply,
            "action.backup.os_build_changed_since_preview",
        ));
    }
    if backup.applied_fingerprint.is_some() {
        return Err(ActionError::new(
            ActionErrorCode::BackupMismatch,
            ActionStage::Apply,
            false,
            "action.backup.already_applied",
        ));
    }
    Ok(())
}

/// New registry mutations require the containing key to have existed when the
/// backup was prepared. Windows has no atomic "delete this key only if it is
/// still empty" primitive, so creating a missing key cannot satisfy both exact
/// rollback and the rule that concurrent third-party values are never erased.
pub fn ensure_registry_key_preexisted(
    backup: &RegistryBackup,
    stage: ActionStage,
) -> ActionResult<()> {
    if !backup.original.key_existed {
        return Err(ActionError::recovery_required(
            stage,
            "action.registry.backup_key_was_absent",
        ));
    }
    Ok(())
}

pub fn evidence(context: &ActionContext<'_>, source: &'static str) -> StateEvidence {
    StateEvidence {
        source: source.to_owned(),
        observed_at_unix_ms: context.observed_at_unix_ms,
        os_build: context.os_identity.base_build,
    }
}

pub fn fingerprint_state(
    state: &DetectedState,
    stage: ActionStage,
) -> ActionResult<Fingerprint> {
    state.stable_fingerprint().map_err(|_| {
        ActionError::new(
            ActionErrorCode::InternalInvariant,
            stage,
            false,
            "action.state.canonical_serialization_failed",
        )
    })
}

pub fn dword_bytes(value: u32) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

pub fn decode_dword(value_type: Option<u32>, bytes: &[u8]) -> Option<u32> {
    if value_type != Some(REG_DWORD_TYPE) || bytes.len() != 4 {
        return None;
    }
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

pub fn map_windows_error(
    stage: ActionStage,
    message_key: &'static str,
    error: WindowsError,
) -> ActionError {
    let code = match error.kind {
        WindowsErrorKind::UnsupportedPlatform => ActionErrorCode::UnsupportedPlatform,
        WindowsErrorKind::AccessDenied => ActionErrorCode::AccessDenied,
        WindowsErrorKind::ResourceLimit => ActionErrorCode::ResourceLimit,
        WindowsErrorKind::InvalidData => ActionErrorCode::StateUnknown,
        WindowsErrorKind::ChannelClosed => ActionErrorCode::LeaseFailure,
        WindowsErrorKind::ApiFailure => ActionErrorCode::WindowsApiFailure,
    };
    let detail = match error.os_code {
        Some(os_code) => format!("{} (OS code {})", error.operation, os_code),
        None => error.operation.to_owned(),
    };
    ActionError::new(code, stage, false, message_key).with_safe_detail(detail)
}
