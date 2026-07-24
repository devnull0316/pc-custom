//! Typed Action contracts and the compile-time Action registry.

mod id;
mod metadata;
mod parameters;
mod registry;
mod state;
mod traits;
mod types;

pub use id::ActionId;
pub use metadata::{
    ActionKind, ActionMetadata, ActionRiskLevel, MethodClass, WindowsReleaseFamily,
};
pub use parameters::{
    ActionParameters, ProcessBindingParameters, ProcessFileIdentity, ThemeColorMode,
};
pub use registry::{ActionRegistry, ACTION_REGISTRY};
pub use state::{DetectedState, ObservedProcess, ObservedValue, StateEvidence, ThemeObservation};
pub use traits::Action;
pub use types::{
    ActionContext, ActionError, ActionErrorCode, ActionResult, ActionStage, AppliedEvidence,
    ChangeExplanation, RollbackEvidence, TroubleshootingStep, ValidationReport, Verification,
};
