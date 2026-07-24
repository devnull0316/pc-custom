use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::action::{ActionId, ProcessBindingParameters};

use super::{Fingerprint, RegistryBackup};

pub const BACKUP_CODEC_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SleepLeaseBackup {
    pub owner_id: Uuid,
    pub keep_display_on: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationBackup {
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessWatchBackup {
    pub binding: ProcessBindingParameters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositeBackup {
    /// Apply order. Restoration always traverses this vector in reverse.
    pub registry_entries: Vec<RegistryBackup>,
    pub all_or_stop: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "primitive", content = "payload", rename_all = "snake_case")]
pub enum BackupPayload {
    Registry(RegistryBackup),
    Composite(CompositeBackup),
    SleepLease(SleepLeaseBackup),
    Observation(ObservationBackup),
    ProcessWatch(ProcessWatchBackup),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupDraft {
    pub precondition_fingerprint: Fingerprint,
    pub intended_fingerprint: Fingerprint,
    pub payload: BackupPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupEnvelope {
    pub backup_id: Uuid,
    pub transaction_id: Uuid,
    pub item_id: Uuid,
    pub action_id: ActionId,
    pub action_version: u32,
    pub codec_version: u32,
    pub created_at_unix_ms: u64,
    pub os_build: u32,
    pub precondition_fingerprint: Fingerprint,
    pub intended_fingerprint: Fingerprint,
    pub applied_fingerprint: Option<Fingerprint>,
    pub rollback_across_unknown_build: bool,
    pub payload: BackupPayload,
    pub integrity_hash: Fingerprint,
}

impl BackupEnvelope {
    pub fn from_draft(
        draft: BackupDraft,
        transaction_id: Uuid,
        item_id: Uuid,
        action_id: ActionId,
        action_version: u32,
        created_at_unix_ms: u64,
        os_build: u32,
    ) -> Self {
        let mut envelope = Self {
            backup_id: Uuid::new_v4(),
            transaction_id,
            item_id,
            action_id,
            action_version,
            codec_version: BACKUP_CODEC_VERSION,
            created_at_unix_ms,
            os_build,
            precondition_fingerprint: draft.precondition_fingerprint,
            intended_fingerprint: draft.intended_fingerprint,
            applied_fingerprint: None,
            rollback_across_unknown_build: false,
            payload: draft.payload,
            integrity_hash: Fingerprint::of_bytes(&[]),
        };
        envelope.integrity_hash = envelope.calculate_integrity_hash();
        envelope
    }

    pub fn record_applied(&mut self, fingerprint: Fingerprint) {
        self.applied_fingerprint = Some(fingerprint);
        self.integrity_hash = self.calculate_integrity_hash();
    }

    pub fn verify_integrity(&self) -> bool {
        self.integrity_hash == self.calculate_integrity_hash()
    }

    fn calculate_integrity_hash(&self) -> Fingerprint {
        let payload = serde_json::to_vec(&self.payload)
            .expect("typed backup payload serialization is infallible");
        let applied_present = [u8::from(self.applied_fingerprint.is_some())];
        let applied = self
            .applied_fingerprint
            .map(|value| value.0)
            .unwrap_or([0; 32]);
        let rollback_across_unknown = [u8::from(self.rollback_across_unknown_build)];
        Fingerprint::of_parts([
            self.backup_id.as_bytes().as_slice(),
            self.transaction_id.as_bytes().as_slice(),
            self.item_id.as_bytes().as_slice(),
            self.action_id.as_str().as_bytes(),
            &self.action_version.to_le_bytes(),
            &self.codec_version.to_le_bytes(),
            &self.created_at_unix_ms.to_le_bytes(),
            &self.os_build.to_le_bytes(),
            &self.precondition_fingerprint.0,
            &self.intended_fingerprint.0,
            applied_present.as_slice(),
            &applied,
            rollback_across_unknown.as_slice(),
            payload.as_slice(),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_envelope() -> BackupEnvelope {
        let draft = BackupDraft {
            precondition_fingerprint: Fingerprint::of_bytes(b"before"),
            intended_fingerprint: Fingerprint::of_bytes(b"after"),
            payload: BackupPayload::Observation(ObservationBackup {
                source: "test observation".to_owned(),
            }),
        };
        BackupEnvelope::from_draft(
            draft,
            Uuid::new_v4(),
            Uuid::new_v4(),
            ActionId::PowerActiveSchemeCheck,
            1,
            123,
            26_100,
        )
    }

    #[test]
    fn integrity_covers_timestamp_and_unknown_build_policy() {
        let envelope = sample_envelope();
        assert!(envelope.verify_integrity());

        let mut changed_timestamp = envelope.clone();
        changed_timestamp.created_at_unix_ms += 1;
        assert!(!changed_timestamp.verify_integrity());

        let mut changed_policy = envelope;
        changed_policy.rollback_across_unknown_build = true;
        assert!(!changed_policy.verify_integrity());
    }

    #[test]
    fn integrity_distinguishes_missing_and_zero_applied_fingerprint() {
        let envelope = sample_envelope();
        let mut tampered = envelope.clone();
        tampered.applied_fingerprint = Some(Fingerprint([0; 32]));
        assert!(!tampered.verify_integrity());

        let mut recorded = envelope;
        recorded.record_applied(Fingerprint([0; 32]));
        assert!(recorded.verify_integrity());
    }
}
