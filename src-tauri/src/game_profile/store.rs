//! ゲームプロファイル定義の永続化。
//!
//! プロファイル**定義**(名前・対象EXE・適用Action)は設定データなので、安全側の journal
//! (SQLite, 実行時状態の正)とは分離し、ユーザーデータ配下の JSON に原子的に保存する。
//! 実行時の適用/復元・lease は [`super::ProfileSupervisor`] が journal を正として扱う。
//!
//! 対象EXEは登録時に `registered_file_identity` で canonical 化し、ローカル固定ボリューム上の
//! 通常ファイルであることと file identity を確認する(名前追従・UNC・reparse を拒否)。

use std::path::PathBuf;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{CoreError, CoreResult};

const PROFILES_FILE_VERSION: u32 = 1;
const MAX_PROFILES: usize = 200;
const MAX_ACTIONS_PER_PROFILE: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredProfileAction {
    pub action_id: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredProfile {
    pub id: String,
    pub name: String,
    /// canonical 化済みの絶対パス。
    pub executable_path: String,
    pub volume_serial_number: u64,
    /// 16 バイト file id の16進表現。
    pub file_id_hex: String,
    pub conflict_policy: String,
    pub automation_enabled: bool,
    pub actions: Vec<StoredProfileAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfilesFile {
    version: u32,
    profiles: Vec<StoredProfile>,
}

/// UI から受け取るプロファイル作成要求。executable_path は生の入力で、
/// store が canonical 化・検証してから保存する。
#[derive(Debug, Clone, Deserialize)]
pub struct CreateProfileRequest {
    pub name: String,
    pub executable_path: String,
    #[serde(default)]
    pub conflict_policy: Option<String>,
    #[serde(default)]
    pub actions: Vec<StoredProfileAction>,
}

pub struct ProfileStore {
    path: PathBuf,
    profiles: Mutex<Vec<StoredProfile>>,
}

impl ProfileStore {
    /// 既存ファイルがあれば読み込み、無ければ空で開く。壊れたファイルは読み込み拒否。
    pub fn open(path: PathBuf) -> CoreResult<Self> {
        let profiles = match std::fs::read(&path) {
            Ok(bytes) => {
                let parsed: ProfilesFile = serde_json::from_slice(&bytes).map_err(|_| {
                    CoreError::new(
                        "PROFILES_FILE_CORRUPT",
                        "BOOTSTRAP",
                        false,
                        "プロファイル定義ファイルを読めません。破損の可能性があります。",
                    )
                })?;
                if parsed.version != PROFILES_FILE_VERSION {
                    return Err(CoreError::new(
                        "PROFILES_FILE_VERSION",
                        "BOOTSTRAP",
                        false,
                        "プロファイル定義ファイルの版が対応外です。",
                    ));
                }
                parsed.profiles
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(_) => return Err(CoreError::storage()),
        };
        Ok(Self {
            path,
            profiles: Mutex::new(profiles),
        })
    }

    pub fn list(&self) -> Vec<StoredProfile> {
        self.profiles.lock().clone()
    }

    pub fn create(&self, request: CreateProfileRequest) -> CoreResult<StoredProfile> {
        let name = request.name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(CoreError::invalid_request(
                "プロファイル名は1〜120文字で入力してください。",
            ));
        }
        if request.actions.len() > MAX_ACTIONS_PER_PROFILE {
            return Err(CoreError::invalid_request(
                "1プロファイルのActionが多すぎます。",
            ));
        }
        // 登録済みActionだけを許可し、未知IDを弾く(任意ID実行への迂回防止)。
        for action in &request.actions {
            if action.action_id.parse::<crate::action::ActionId>().is_err() {
                return Err(CoreError::invalid_request(
                    "登録されていないActionは登録できません。",
                ));
            }
        }
        let conflict_policy = match request.conflict_policy.as_deref() {
            None | Some("abort_profile") => "abort_profile".to_owned(),
            Some("skip_conflicting") => "skip_conflicting".to_owned(),
            Some(_) => {
                return Err(CoreError::invalid_request("競合方針の値が不正です。"))
            }
        };

        // 対象EXEを canonical 化し、ローカル固定ボリューム/通常ファイル/非reparse を検証する。
        let (canonical_path, volume_serial_number, file_id) =
            resolve_binding(&request.executable_path)?;

        let profile = StoredProfile {
            id: Uuid::new_v4().to_string(),
            name: name.to_owned(),
            executable_path: canonical_path,
            volume_serial_number,
            file_id_hex: hex::encode(file_id),
            conflict_policy,
            automation_enabled: false, // 明示的に有効化するまで自動適用しない。
            actions: request.actions,
        };

        let mut guard = self.profiles.lock();
        if guard.len() >= MAX_PROFILES {
            return Err(CoreError::invalid_request(
                "登録できるプロファイル数の上限に達しました。",
            ));
        }
        guard.push(profile.clone());
        Self::persist(&self.path, &guard)?;
        Ok(profile)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> CoreResult<()> {
        let mut guard = self.profiles.lock();
        let profile = guard
            .iter_mut()
            .find(|profile| profile.id == id)
            .ok_or_else(|| CoreError::invalid_request("対象のプロファイルがありません。"))?;
        profile.automation_enabled = enabled;
        Self::persist(&self.path, &guard)
    }

    pub fn delete(&self, id: &str) -> CoreResult<()> {
        let mut guard = self.profiles.lock();
        let before = guard.len();
        guard.retain(|profile| profile.id != id);
        if guard.len() == before {
            return Err(CoreError::invalid_request("対象のプロファイルがありません。"));
        }
        Self::persist(&self.path, &guard)
    }

    /// 一時ファイルへ書いてから rename する原子的保存(Rust std は Windows で置換 rename)。
    fn persist(path: &PathBuf, profiles: &[StoredProfile]) -> CoreResult<()> {
        let file = ProfilesFile {
            version: PROFILES_FILE_VERSION,
            profiles: profiles.to_vec(),
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|_| CoreError::storage())?;
        let mut temp = path.clone();
        temp.set_extension("json.tmp");
        std::fs::write(&temp, &bytes).map_err(|_| CoreError::storage())?;
        std::fs::rename(&temp, path).map_err(|_| CoreError::storage())?;
        Ok(())
    }
}

#[cfg(windows)]
fn resolve_binding(path: &str) -> CoreResult<(String, u64, [u8; 16])> {
    let (canonical, identity) = crate::windows::registered_file_identity(path).map_err(|_| {
        CoreError::invalid_request(
            "対象の実行ファイルを確認できません。ローカルドライブ上の実在する .exe を選んでください。",
        )
    })?;
    Ok((canonical, identity.volume_serial_number, identity.file_id))
}

#[cfg(not(windows))]
fn resolve_binding(_path: &str) -> CoreResult<(String, u64, [u8; 16])> {
    Err(CoreError::new(
        "UNSUPPORTED_PLATFORM",
        "VALIDATE",
        false,
        "TotonoeはWindows 11専用です。",
    ))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn temp_store() -> (ProfileStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("profiles.json");
        (ProfileStore::open(path).expect("open empty store"), dir)
    }

    fn notepad() -> String {
        // ローカル固定ボリューム上に必ず存在する検証用の実 EXE。
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
        format!(r"{root}\System32\notepad.exe")
    }

    #[test]
    fn create_list_enable_delete_round_trips_on_disk() {
        let (store, _dir) = temp_store();
        let created = store
            .create(CreateProfileRequest {
                name: "  テストゲーム  ".to_owned(),
                executable_path: notepad(),
                conflict_policy: None,
                actions: vec![StoredProfileAction {
                    action_id: "theme.color_mode".to_owned(),
                    parameters: serde_json::json!({ "mode": "dark" }),
                }],
            })
            .expect("create profile");
        assert_eq!(created.name, "テストゲーム"); // trim 済み
        assert!(!created.automation_enabled); // 既定は無効
        assert!(created.executable_path.to_lowercase().ends_with("notepad.exe"));
        assert_eq!(created.file_id_hex.len(), 32);

        // 別インスタンスで開き直しても永続化されている。
        let reopened = ProfileStore::open(store.path.clone()).expect("reopen");
        assert_eq!(reopened.list().len(), 1);

        store.set_enabled(&created.id, true).expect("enable");
        assert!(ProfileStore::open(store.path.clone()).unwrap().list()[0].automation_enabled);

        store.delete(&created.id).expect("delete");
        assert!(store.list().is_empty());
        assert!(ProfileStore::open(store.path.clone()).unwrap().list().is_empty());
    }

    #[test]
    fn unknown_action_id_is_rejected() {
        let (store, _dir) = temp_store();
        let result = store.create(CreateProfileRequest {
            name: "x".to_owned(),
            executable_path: notepad(),
            conflict_policy: None,
            actions: vec![StoredProfileAction {
                action_id: "totally.unknown".to_owned(),
                parameters: serde_json::Value::Null,
            }],
        });
        assert!(result.is_err());
    }

    #[test]
    fn nonexistent_executable_is_rejected() {
        let (store, _dir) = temp_store();
        let result = store.create(CreateProfileRequest {
            name: "x".to_owned(),
            executable_path: r"C:\definitely\not\here\ghost.exe".to_owned(),
            conflict_policy: None,
            actions: vec![],
        });
        assert!(result.is_err());
    }
}
