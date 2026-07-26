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
    ActionParameters, AppLaunchBundle, ExplorerLaunchTarget, PowerScheme, ProcessBindingParameters,
    ProcessFileIdentity, StartLayout, TaskbarAlignment, TaskbarGroupingMode,
    TaskbarMultiMonitorMode, TaskbarSearchMode, ThemeColorMode,
};
pub use registry::{ActionRegistry, ACTION_REGISTRY};
pub use state::{
    AdvancedColorObservation, DefaultRenderAudioObservation, DetectedState,
    GameReadinessObservation, KnownAppObservation, KnownAppState, KnownAppsObservation,
    ObservationWarning, ObservedProcess, ObservedValue, PowerToysInstallationObservation,
    PrimaryRefreshRateObservation, ReadinessComponent, StartupEntrySource, StartupEntryStatus,
    StartupInventoryEntry, StartupInventoryObservation, StateEvidence, SystemDriveSpaceObservation,
    TempFilesObservation, ThemeObservation, WindowColorPreset, WindowsUpdateStatusObservation,
};
pub use traits::Action;
pub use types::{
    ActionContext, ActionError, ActionErrorCode, ActionResult, ActionStage, AppliedEvidence,
    ChangeExplanation, RollbackEvidence, TroubleshootingStep, ValidationReport, Verification,
};
