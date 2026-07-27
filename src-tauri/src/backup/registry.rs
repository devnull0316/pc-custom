use serde::{Deserialize, Serialize};

use crate::windows::{
    delete_value, read_value_state, write_raw_value, RawRegistryValue, WindowsError,
};

use super::Fingerprint;

pub const MAX_REGISTRY_VALUE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryHive {
    CurrentUser,
    LocalMachine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryView {
    Registry32,
    Registry64,
}

#[derive(Debug, Clone, Copy)]
pub struct RegistryTarget {
    pub hive: RegistryHive,
    pub subkey: &'static str,
    pub value_name: &'static str,
    pub view: RegistryView,
}

impl RegistryTarget {
    pub const fn current_user_64(subkey: &'static str, value_name: &'static str) -> Self {
        Self {
            hive: RegistryHive::CurrentUser,
            subkey,
            value_name,
            view: RegistryView::Registry64,
        }
    }

    pub fn location(self) -> RegistryLocation {
        RegistryLocation {
            hive: self.hive,
            canonical_subkey: self.subkey.to_owned(),
            value_name: self.value_name.to_owned(),
            view: self.view,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryLocation {
    pub hive: RegistryHive,
    pub canonical_subkey: String,
    pub value_name: String,
    pub view: RegistryView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryValueState {
    pub key_existed: bool,
    pub value_existed: bool,
    pub value_type: Option<u32>,
    pub raw_bytes: Vec<u8>,
}

impl RegistryValueState {
    pub fn fingerprint(&self, location: &RegistryLocation) -> Fingerprint {
        let hive = [match location.hive {
            RegistryHive::CurrentUser => 1,
            RegistryHive::LocalMachine => 2,
        }];
        let view = [match location.view {
            RegistryView::Registry32 => 32,
            RegistryView::Registry64 => 64,
        }];
        let key_existed = [u8::from(self.key_existed)];
        let value_existed = [u8::from(self.value_existed)];
        let value_type = self.value_type.unwrap_or(0).to_le_bytes();
        Fingerprint::of_parts([
            hive.as_slice(),
            view.as_slice(),
            location.canonical_subkey.as_bytes(),
            location.value_name.as_bytes(),
            key_existed.as_slice(),
            value_existed.as_slice(),
            value_type.as_slice(),
            self.raw_bytes.as_slice(),
        ])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryBackup {
    pub location: RegistryLocation,
    pub original: RegistryValueState,
    pub intended_type: u32,
    pub intended_raw: Vec<u8>,
    pub applied_type: u32,
    pub applied_raw: Vec<u8>,
    pub action_version: u32,
    pub windows_build: u32,
}

impl RegistryBackup {
    pub fn intended_state(&self) -> RegistryValueState {
        RegistryValueState {
            key_existed: true,
            value_existed: true,
            value_type: Some(self.intended_type),
            raw_bytes: self.intended_raw.clone(),
        }
    }

    pub fn applied_state(&self) -> RegistryValueState {
        RegistryValueState {
            key_existed: true,
            value_existed: true,
            value_type: Some(self.applied_type),
            raw_bytes: self.applied_raw.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryClassification {
    Original,
    Applied,
    Third,
    /// A legacy rollback removed the value, but the key did not originally
    /// exist. The key is retained because safely comparing and deleting it is
    /// not atomic.
    CreatedKeyWithoutValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryRestoreOutcome {
    Restored,
    AlreadyOriginal,
    /// The target value was safely removed, but an empty key was retained.
    ///
    /// This is only reachable while recovering a backup written by an older
    /// PCカスタム version. Deleting an apparently-empty registry key is not an
    /// atomic compare-and-delete operation and could erase values written by
    /// another process after the emptiness check.
    RestoredValueKeyRetained,
    ExternalConflict,
}

pub fn read_registry_state(
    location: &RegistryLocation,
) -> Result<RegistryValueState, WindowsError> {
    let observed = read_value_state(location, MAX_REGISTRY_VALUE_BYTES)?;
    let (value_existed, value_type, raw_bytes) = match observed.value {
        Some(RawRegistryValue { value_type, bytes }) => (true, Some(value_type), bytes),
        None => (false, None, Vec::new()),
    };
    Ok(RegistryValueState {
        key_existed: observed.key_existed,
        value_existed,
        value_type,
        raw_bytes,
    })
}

pub fn prepare_registry_backup(
    target: RegistryTarget,
    intended_type: u32,
    intended_raw: Vec<u8>,
    action_version: u32,
    windows_build: u32,
) -> Result<RegistryBackup, WindowsError> {
    if intended_raw.len() > MAX_REGISTRY_VALUE_BYTES {
        return Err(WindowsError::new(
            crate::windows::WindowsErrorKind::ResourceLimit,
            "prepare registry backup",
            None,
        ));
    }
    let location = target.location();
    let original = read_registry_state(&location)?;
    if !original.key_existed {
        return Err(WindowsError::new(
            crate::windows::WindowsErrorKind::InvalidData,
            "require pre-existing registry key for reversible mutation",
            None,
        ));
    }
    Ok(RegistryBackup {
        location,
        original,
        intended_type,
        applied_type: intended_type,
        applied_raw: intended_raw.clone(),
        intended_raw,
        action_version,
        windows_build,
    })
}

pub fn classify_registry_backup(
    backup: &RegistryBackup,
) -> Result<RegistryClassification, WindowsError> {
    let current = read_registry_state(&backup.location)?;
    if current == backup.original {
        Ok(RegistryClassification::Original)
    } else if current == backup.applied_state() {
        Ok(RegistryClassification::Applied)
    } else if !backup.original.key_existed && current.key_existed && !current.value_existed {
        Ok(RegistryClassification::CreatedKeyWithoutValue)
    } else {
        Ok(RegistryClassification::Third)
    }
}

pub fn restore_registry_backup(
    backup: &RegistryBackup,
) -> Result<RegistryRestoreOutcome, WindowsError> {
    match classify_registry_backup(backup)? {
        RegistryClassification::Original => Ok(RegistryRestoreOutcome::AlreadyOriginal),
        RegistryClassification::Third => Ok(RegistryRestoreOutcome::ExternalConflict),
        RegistryClassification::CreatedKeyWithoutValue => {
            Ok(RegistryRestoreOutcome::RestoredValueKeyRetained)
        }
        RegistryClassification::Applied => {
            if backup.original.value_existed {
                let original_type = backup.original.value_type.ok_or_else(|| {
                    WindowsError::new(
                        crate::windows::WindowsErrorKind::InvalidData,
                        "restore registry value type",
                        None,
                    )
                })?;
                write_raw_value(&backup.location, original_type, &backup.original.raw_bytes)?;
            } else {
                delete_value(&backup.location)?;
            }
            if !backup.original.key_existed {
                return Ok(RegistryRestoreOutcome::RestoredValueKeyRetained);
            }
            Ok(RegistryRestoreOutcome::Restored)
        }
    }
}

pub fn verify_registry_backup_restored(backup: &RegistryBackup) -> Result<bool, WindowsError> {
    Ok(read_registry_state(&backup.location)? == backup.original)
}

#[cfg(all(test, windows))]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::windows::{delete_key_if_empty, delete_value, write_raw_value};

    fn unique_target(value_name: &'static str) -> RegistryTarget {
        let key: &'static str = Box::leak(
            format!(r"Software\PCカスタム\IntegrationTests\{}", Uuid::new_v4()).into_boxed_str(),
        );
        RegistryTarget::current_user_64(key, value_name)
    }

    fn legacy_missing_key_backup(target: RegistryTarget, intended: u32) -> RegistryBackup {
        RegistryBackup {
            location: target.location(),
            original: RegistryValueState {
                key_existed: false,
                value_existed: false,
                value_type: None,
                raw_bytes: Vec::new(),
            },
            intended_type: winreg::enums::REG_DWORD as u32,
            intended_raw: intended.to_le_bytes().to_vec(),
            applied_type: winreg::enums::REG_DWORD as u32,
            applied_raw: intended.to_le_bytes().to_vec(),
            action_version: 1,
            windows_build: 26_100,
        }
    }

    #[test]
    fn missing_key_is_rejected_before_backup() {
        let target = unique_target("RoundTripValue");
        let location = target.location();
        let error = prepare_registry_backup(
            target,
            winreg::enums::REG_DWORD as u32,
            1u32.to_le_bytes().to_vec(),
            1,
            26_100,
        )
        .expect_err("a missing key must not produce a mutable backup");
        assert_eq!(error.kind, crate::windows::WindowsErrorKind::InvalidData);
        let after = read_registry_state(&location).expect("read missing key after rejection");
        assert!(!after.key_existed);
        assert!(!after.value_existed);
    }

    #[test]
    fn legacy_missing_key_backup_removes_only_target_and_preserves_sibling() {
        let target = unique_target("LegacyTarget");
        let backup = legacy_missing_key_backup(target, 1);
        let mut sibling = backup.location.clone();
        sibling.value_name = "SiblingValue".to_owned();
        write_raw_value(&backup.location, backup.intended_type, &backup.intended_raw)
            .expect("seed legacy applied value");
        let sibling_raw = vec![0xde, 0xad, 0xbe, 0xef];
        write_raw_value(&sibling, winreg::enums::REG_BINARY as u32, &sibling_raw)
            .expect("seed third-party sibling value");
        assert_eq!(
            classify_registry_backup(&backup).expect("classify legacy applied state"),
            RegistryClassification::Applied
        );
        assert_eq!(
            restore_registry_backup(&backup).expect("safely restore legacy backup"),
            RegistryRestoreOutcome::RestoredValueKeyRetained
        );
        let target_after = read_registry_state(&backup.location).expect("read restored target");
        assert!(target_after.key_existed);
        assert!(!target_after.value_existed);
        let sibling_after = read_registry_state(&sibling).expect("read preserved sibling");
        assert_eq!(
            sibling_after.value_type,
            Some(winreg::enums::REG_BINARY as u32)
        );
        assert_eq!(sibling_after.raw_bytes, sibling_raw);
        assert!(!verify_registry_backup_restored(&backup).expect("verify retained key is partial"));

        delete_value(&sibling).expect("remove isolated sibling value");
        delete_key_if_empty(&backup.location).expect("remove isolated legacy test key");
    }
    #[test]
    fn existing_type_and_raw_bytes_round_trip_losslessly() {
        let target = unique_target("RawValue");
        let location = target.location();
        let original = vec![0x00, 0xff, 0x10, 0x00, 0x80];
        write_raw_value(&location, winreg::enums::REG_BINARY as u32, &original)
            .expect("seed isolated test value");

        let backup = prepare_registry_backup(
            target,
            winreg::enums::REG_DWORD as u32,
            0u32.to_le_bytes().to_vec(),
            1,
            26_100,
        )
        .expect("prepare isolated lossless backup");
        write_raw_value(&backup.location, backup.intended_type, &backup.intended_raw)
            .expect("apply isolated test value");
        restore_registry_backup(&backup).expect("restore isolated test value");
        let restored = read_registry_state(&backup.location).unwrap();
        assert_eq!(restored.value_type, Some(winreg::enums::REG_BINARY as u32));
        assert_eq!(restored.raw_bytes, original);

        delete_value(&backup.location).expect("remove isolated test value");
        delete_key_if_empty(&backup.location).expect("remove isolated test key");
    }

    #[test]
    fn absent_value_round_trips_without_deleting_preexisting_key() {
        let target = unique_target("AbsentValue");
        let location = target.location();
        write_raw_value(
            &location,
            winreg::enums::REG_DWORD as u32,
            &7u32.to_le_bytes(),
        )
        .expect("create isolated test key");
        delete_value(&location).expect("leave isolated key empty");

        let backup = prepare_registry_backup(
            target,
            winreg::enums::REG_DWORD as u32,
            1u32.to_le_bytes().to_vec(),
            1,
            26_100,
        )
        .expect("prepare backup for absent value");
        assert!(backup.original.key_existed);
        assert!(!backup.original.value_existed);

        write_raw_value(&backup.location, backup.intended_type, &backup.intended_raw)
            .expect("apply isolated value");
        assert_eq!(
            restore_registry_backup(&backup).expect("restore absent value"),
            RegistryRestoreOutcome::Restored
        );
        let restored =
            read_registry_state(&backup.location).expect("read restored absent value state");
        assert!(restored.key_existed);
        assert!(!restored.value_existed);

        delete_key_if_empty(&backup.location).expect("remove isolated test key");
    }

    #[test]
    fn third_state_is_reported_and_never_overwritten() {
        let target = unique_target("ExternalChange");
        let location = target.location();
        write_raw_value(
            &location,
            winreg::enums::REG_DWORD as u32,
            &0u32.to_le_bytes(),
        )
        .expect("create isolated pre-existing key");
        delete_value(&location).expect("leave isolated key without target value");
        let backup = prepare_registry_backup(
            target,
            winreg::enums::REG_DWORD as u32,
            1u32.to_le_bytes().to_vec(),
            1,
            26_100,
        )
        .expect("prepare isolated backup");
        write_raw_value(&backup.location, backup.intended_type, &backup.intended_raw)
            .expect("apply isolated intended value");

        let external_bytes = vec![0xde, 0xad, 0xbe, 0xef];
        write_raw_value(
            &backup.location,
            winreg::enums::REG_BINARY as u32,
            &external_bytes,
        )
        .expect("simulate external writer");
        assert_eq!(
            restore_registry_backup(&backup).expect("classify third state"),
            RegistryRestoreOutcome::ExternalConflict
        );
        let after = read_registry_state(&backup.location).expect("read value after conflict");
        assert_eq!(after.value_type, Some(winreg::enums::REG_BINARY as u32));
        assert_eq!(after.raw_bytes, external_bytes);

        delete_value(&backup.location).expect("remove isolated external value");
        delete_key_if_empty(&backup.location).expect("remove isolated test key");
    }
}
