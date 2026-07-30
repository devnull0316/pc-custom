use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::action::{ActionId, PowerModeChoice, PowerScheme, ProcessBindingParameters};
use crate::window_layout::WindowLayoutBackup;

use super::{Fingerprint, RegistryBackup};

pub const BACKUP_CODEC_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SleepLeaseBackup {
    pub owner_id: Uuid,
    pub keep_display_on: bool,
}

/// Complete user settings read through the documented keyboard accessibility API.
///
/// The size fields are retained deliberately: exact rollback restores every field
/// returned by Windows rather than synthesizing defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyboardAccessibilitySettings {
    pub sticky_size: u32,
    pub sticky_flags: u32,
    pub filter_size: u32,
    pub filter_flags: u32,
    pub filter_wait_ms: u32,
    pub filter_delay_ms: u32,
    pub filter_repeat_ms: u32,
    pub filter_bounce_ms: u32,
}

impl KeyboardAccessibilitySettings {
    pub fn fingerprint(self) -> Fingerprint {
        let mut bytes = Vec::with_capacity(8 * std::mem::size_of::<u32>());
        for value in [
            self.sticky_size,
            self.sticky_flags,
            self.filter_size,
            self.filter_flags,
            self.filter_wait_ms,
            self.filter_delay_ms,
            self.filter_repeat_ms,
            self.filter_bounce_ms,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        Fingerprint::of_bytes(&bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShiftInterruptionGuardBackup {
    pub original: KeyboardAccessibilitySettings,
    pub intended: KeyboardAccessibilitySettings,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerSchemeGuid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

impl PowerSchemeGuid {
    pub const BALANCED: Self = Self {
        data1: 0x381b4222,
        data2: 0xf694,
        data3: 0x41f0,
        data4: [0x96, 0x85, 0xff, 0x5b, 0xb2, 0x60, 0xdf, 0x2e],
    };
    pub const POWER_SAVER: Self = Self {
        data1: 0xa1841308,
        data2: 0x3541,
        data3: 0x4fab,
        data4: [0xbc, 0x81, 0xf7, 0x15, 0x56, 0xf2, 0x0b, 0x4a],
    };
    pub const HIGH_PERFORMANCE: Self = Self {
        data1: 0x8c5e7fda,
        data2: 0xe8bf,
        data3: 0x4a96,
        data4: [0x9a, 0x85, 0xa6, 0xe2, 0x3a, 0x8c, 0x63, 0x5c],
    };

    pub const fn for_scheme(scheme: PowerScheme) -> Self {
        match scheme {
            PowerScheme::Balanced => Self::BALANCED,
            PowerScheme::PowerSaver => Self::POWER_SAVER,
            PowerScheme::HighPerformance => Self::HIGH_PERFORMANCE,
        }
    }

    pub fn canonical_string(self) -> String {
        format!(
            "{{{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
            self.data1,
            self.data2,
            self.data3,
            self.data4[0],
            self.data4[1],
            self.data4[2],
            self.data4[3],
            self.data4[4],
            self.data4[5],
            self.data4[6],
            self.data4[7],
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerSchemeBackup {
    pub original_guid: PowerSchemeGuid,
    pub intended_scheme: PowerScheme,
    pub intended_guid: PowerSchemeGuid,
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
    PowerScheme(PowerSchemeBackup),
    WindowLayout(WindowLayoutBackup),
    ShiftInterruptionGuard(ShiftInterruptionGuardBackup),
    PowerMode(PowerModeBackup),
    PointerFeel(PointerFeelBackup),
    CommsMicMute(CommsMicMuteBackup),
    DefaultPrinter(DefaultPrinterBackup),
    HighContrast(HighContrastBackup),
}

/// ポインターの動き方の変更前状態。
///
/// 加速だけを変えるが、**保存と復元は4つの値すべてを対象にする。**
/// 速度だけ別の何かに動かされていたら、それは第三者の変更として扱いたい。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointerFeelBackup {
    pub original: crate::windows::PointerFeel,
    pub intended: crate::windows::PointerFeel,
}

/// The exact default communications capture endpoint changed by this Action.
///
/// Rollback resolves `device_id` directly and never asks Windows for the new
/// default endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommsMicMuteBackup {
    pub original: crate::windows::CommsMicMuteState,
    pub intended: crate::windows::CommsMicMuteState,
}

/// Exact printer names are private rollback data. Their `Debug` representation
/// is redacted by `PrinterName`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultPrinterBackup {
    pub original: crate::windows::PrinterName,
    pub intended: crate::windows::PrinterName,
}

/// `HIGHCONTRASTW` の開始前状態と、Windowsへ要求する適用状態。
///
/// 実際の適用値は Windows が scheme 名を正規化することがあるため、
/// `BackupEnvelope::applied_fingerprint` に読み直した状態を別途記録する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HighContrastBackup {
    pub original: crate::windows::HighContrastSnapshot,
    pub intended: crate::windows::HighContrastSnapshot,
}

/// 電源モードの変更前状態。
///
/// **3値の enum ではなく生のバイト列で持つ。** この PC が既に、こちらの知らない
/// overlay GUID を選んでいる可能性がある。知らない値を「バランス」に丸めて保存すると、
/// 戻したときに別の設定になる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerModeBackup {
    pub original_ac: [u8; 16],
    pub original_dc: [u8; 16],
    pub intended: PowerModeChoice,
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
