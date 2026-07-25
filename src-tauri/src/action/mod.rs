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
    ActionParameters, ExplorerLaunchTarget, PowerScheme, ProcessBindingParameters,
    ProcessFileIdentity, StartLayout, TaskbarAlignment, TaskbarGroupingMode,
    TaskbarMultiMonitorMode, TaskbarSearchMode, ThemeColorMode,
};
pub use registry::{ActionRegistry, ACTION_REGISTRY};
pub use state::{WindowColorPreset, 
    AdvancedColorObservation, DefaultRenderAudioObservation, DetectedState,
    GameReadinessObservation, ObservationWarning, ObservedProcess, ObservedValue,
    PrimaryRefreshRateObservation, ReadinessComponent, StartupEntrySource,
    StartupEntryStatus, StartupInventoryEntry, StartupInventoryObservation,
    StateEvidence, SystemDriveSpaceObservation, TempFilesObservation,
    ThemeObservation,
};
pub use traits::Action;
pub use types::{
    ActionContext, ActionError, ActionErrorCode, ActionResult, ActionStage,
    AppliedEvidence, ChangeExplanation, RollbackEvidence, TroubleshootingStep,
    ValidationReport, Verification,
};
