//! Lossless, versioned backup primitives. Display JSON is never a restore source.

mod envelope;
mod fingerprint;
mod registry;

pub use envelope::{
    BackupDraft, BackupEnvelope, BackupPayload, CompositeBackup, ObservationBackup,
    PowerSchemeBackup, PowerSchemeGuid, ProcessWatchBackup, SleepLeaseBackup, BACKUP_CODEC_VERSION,
};
pub use fingerprint::Fingerprint;
pub use registry::{
    classify_registry_backup, prepare_registry_backup, read_registry_state,
    restore_registry_backup, verify_registry_backup_restored, RegistryBackup,
    RegistryClassification, RegistryHive, RegistryLocation, RegistryRestoreOutcome, RegistryTarget,
    RegistryValueState, RegistryView, MAX_REGISTRY_VALUE_BYTES,
};
