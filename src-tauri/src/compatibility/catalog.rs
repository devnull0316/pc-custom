use serde::Serialize;

use crate::action::{ActionError, ActionErrorCode, ActionKind, ActionMetadata, ActionStage};

use super::{Architecture, OsIdentity};

const WINDOWS_11_24H2: u32 = 26_100;
const WINDOWS_11_25H2: u32 = 26_200;
const WINDOWS_11_26H1: u32 = 28_000;
const WINDOWS_11_23H2: u32 = 22_631;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityMode {
    TestedMutable,
    TestedDetectOnly,
    Unsupported,
    UnknownBuild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CompatibilityDecision {
    pub mode: CompatibilityMode,
    pub rollback_across_unknown_build: bool,
    pub evidence_id: &'static str,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CompatibilityCatalog;

impl CompatibilityCatalog {
    pub const fn decision_for_build(base_build: u32) -> CompatibilityDecision {
        let (mode, evidence_id) = match base_build {
            WINDOWS_11_24H2 => (
                CompatibilityMode::TestedMutable,
                "totonoe.win11.24h2.task2-smoke-required",
            ),
            WINDOWS_11_25H2 => (
                CompatibilityMode::TestedMutable,
                "totonoe.win11.25h2.task2-smoke-required",
            ),
            WINDOWS_11_26H1 => (
                CompatibilityMode::TestedDetectOnly,
                "totonoe.win11.26h1.detect-only",
            ),
            WINDOWS_11_23H2 => (
                CompatibilityMode::Unsupported,
                "totonoe.win11.23h2.home-pro-out-of-scope",
            ),
            _ => (CompatibilityMode::UnknownBuild, "totonoe.unknown-build"),
        };
        CompatibilityDecision {
            mode,
            rollback_across_unknown_build: false,
            evidence_id,
        }
    }

    pub fn decision_for_identity(os_identity: &OsIdentity) -> CompatibilityDecision {
        if os_identity.major != 10 || os_identity.product_type != 1 {
            return CompatibilityDecision {
                mode: CompatibilityMode::Unsupported,
                rollback_across_unknown_build: false,
                evidence_id: "totonoe.non-client-windows",
            };
        }
        if !matches!(
            os_identity.architecture,
            Architecture::X64 | Architecture::Arm64
        ) {
            return CompatibilityDecision {
                mode: CompatibilityMode::Unsupported,
                rollback_across_unknown_build: false,
                evidence_id: "totonoe.unsupported-architecture",
            };
        }
        // Professional/Professional N and the four consumer Core (Home) SKUs.
        // Enterprise/Education stay read-only outside the Task 2 test matrix.
        if !matches!(
            os_identity.operating_system_sku,
            48 | 49 | 98 | 99 | 100 | 101
        ) {
            return CompatibilityDecision {
                mode: CompatibilityMode::TestedDetectOnly,
                rollback_across_unknown_build: false,
                evidence_id: "totonoe.edition-outside-home-pro-matrix",
            };
        }
        Self::decision_for_build(os_identity.base_build)
    }

    pub fn evaluate(os_identity: &OsIdentity, metadata: &ActionMetadata) -> CompatibilityDecision {
        let mut decision = Self::decision_for_identity(os_identity);
        if matches!(decision.mode, CompatibilityMode::TestedMutable)
            && (os_identity.base_build < metadata.minimumBuild
                || os_identity.base_build > metadata.maximumTestedBuild)
        {
            decision.mode = CompatibilityMode::TestedDetectOnly;
            decision.evidence_id = "totonoe.action-build-outside-tested-range";
        }
        decision
    }

    pub fn ensure_detect_allowed(
        os_identity: &OsIdentity,
        metadata: &ActionMetadata,
    ) -> Result<CompatibilityDecision, ActionError> {
        let decision = Self::evaluate(os_identity, metadata);
        if decision.mode == CompatibilityMode::Unsupported {
            return Err(ActionError::new(
                ActionErrorCode::CompatibilityBlocked,
                ActionStage::Detect,
                false,
                "action.compatibility.unsupported",
            ));
        }
        Ok(decision)
    }

    pub fn ensure_mutation_allowed(
        os_identity: &OsIdentity,
        metadata: &ActionMetadata,
        stage: ActionStage,
    ) -> Result<CompatibilityDecision, ActionError> {
        if matches!(metadata.kind, ActionKind::Observation | ActionKind::Guided) {
            return Self::ensure_detect_allowed(os_identity, metadata);
        }
        let decision = Self::evaluate(os_identity, metadata);
        match decision.mode {
            CompatibilityMode::TestedMutable => Ok(decision),
            CompatibilityMode::UnknownBuild => Err(ActionError::new(
                ActionErrorCode::RecoveryRequired,
                stage,
                false,
                "action.compatibility.unknown_build_recovery_required",
            )),
            CompatibilityMode::TestedDetectOnly => Err(ActionError::new(
                ActionErrorCode::CompatibilityBlocked,
                stage,
                false,
                "action.compatibility.detect_only",
            )),
            CompatibilityMode::Unsupported => Err(ActionError::new(
                ActionErrorCode::CompatibilityBlocked,
                stage,
                false,
                "action.compatibility.unsupported",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_build_is_never_mutable() {
        let decision = CompatibilityCatalog::decision_for_build(99_999);
        assert_eq!(decision.mode, CompatibilityMode::UnknownBuild);
        assert!(!decision.rollback_across_unknown_build);
    }

    #[test]
    fn current_known_builds_have_explicit_modes() {
        assert_eq!(
            CompatibilityCatalog::decision_for_build(WINDOWS_11_24H2).mode,
            CompatibilityMode::TestedMutable
        );
        assert_eq!(
            CompatibilityCatalog::decision_for_build(WINDOWS_11_26H1).mode,
            CompatibilityMode::TestedDetectOnly
        );
    }
}
