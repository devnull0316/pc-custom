//! ゲームプロファイル定義の永続化。
//!
//! プロファイル**定義**(名前・対象EXE・適用Action)は設定データなので、安全側の journal
//! (SQLite, 実行時状態の正)とは分離し、ユーザーデータ配下の JSON に原子的に保存する。
//! 実行時の適用/復元・lease は [`super::ProfileSupervisor`] が journal を正として扱う。
//!
//! 対象EXEは登録時に `registered_file_identity` で canonical 化し、ローカル固定ボリューム上の
//! 通常ファイルであることと file identity を確認する(名前追従・UNC・reparse を拒否)。

use std::{
    collections::HashSet,
    io::Read,
    path::{Path, PathBuf},
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    action::ProcessFileIdentity,
    error::{CoreError, CoreResult},
};

const PROFILES_FILE_VERSION: u32 = 1;
const MAX_PROFILES_FILE_BYTES: u64 = 1024 * 1024;
const MAX_PROFILES: usize = 200;
const MAX_ACTIONS_PER_PROFILE: usize = 32;
const MAX_EXECUTABLE_PATH_CHARS: usize = 32_767;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredProfileAction {
    pub action_id: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredProfile {
    pub id: String,
    pub name: String,
    /// None は実行ファイルに紐付かない手動モード。Some はcanonical化済み絶対パス。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_serial_number: Option<u64>,
    /// 16 バイト file id の16進表現。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id_hex: Option<String>,
    pub conflict_policy: String,
    pub automation_enabled: bool,
    pub actions: Vec<StoredProfileAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_run: Option<ManualRunRecord>,
}

impl StoredProfile {
    pub fn is_manual(&self) -> bool {
        self.executable_path.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualRunRecord {
    pub transaction_id: String,
    pub reversible_item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfilesFile {
    version: u32,
    profiles: Vec<StoredProfile>,
}

/// UI から受け取るプロファイル作成要求。executable_path は生の入力で、
/// store が canonical 化・検証してから保存する。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateProfileRequest {
    pub name: String,
    #[serde(default)]
    pub executable_path: Option<String>,
    #[serde(default)]
    pub conflict_policy: Option<String>,
    #[serde(default)]
    pub actions: Vec<StoredProfileAction>,
}

/// インポート前プレビュー: この機で実行ファイルが解決できるか等を提示する。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewItem {
    pub name: String,
    pub executable_path: String,
    pub action_count: usize,
    pub resolvable: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSkip {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub imported: Vec<String>,
    pub skipped: Vec<ImportSkip>,
}

#[derive(Debug)]
pub struct ProfileStore {
    path: PathBuf,
    profiles: Mutex<Vec<StoredProfile>>,
}

impl ProfileStore {
    /// 既存ファイルがあれば読み込み、無ければ空で開く。壊れたファイルは読み込み拒否。
    pub fn open(path: PathBuf) -> CoreResult<Self> {
        let profiles = match std::fs::File::open(&path) {
            Ok(file) => {
                // metadata確認だけでは確認後の追記を防げないため、読み取り自体を
                // 上限+1 byteに制限する。巨大/増大中ファイルを無制限に確保しない。
                let mut bytes = Vec::new();
                file.take(MAX_PROFILES_FILE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| CoreError::storage())?;
                if bytes.len() as u64 > MAX_PROFILES_FILE_BYTES {
                    return Err(CoreError::new(
                        "PROFILES_FILE_TOO_LARGE",
                        "BOOTSTRAP",
                        false,
                        "プロファイル定義ファイルが安全な読込上限を超えています。",
                    ));
                }
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
                validate_loaded_profiles(&parsed.profiles)?;
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

    /// Returns every identity that must be treated as a registered game for
    /// window operations.
    ///
    /// The stored identity protects a profile across an executable replacement,
    /// while the identity re-read from the current executable path protects the
    /// replacement itself. An unreadable or malformed binding fails closed.
    pub fn registered_game_file_identities(&self) -> CoreResult<Vec<ProcessFileIdentity>> {
        let profiles = self.profiles.lock().clone();
        let mut unique = HashSet::with_capacity(profiles.len().saturating_mul(2));
        for profile in profiles {
            if profile.is_manual() {
                continue;
            }
            let (Some(path), Some(volume_serial_number), Some(file_id_hex)) = (
                profile.executable_path.as_deref(),
                profile.volume_serial_number,
                profile.file_id_hex.as_deref(),
            ) else {
                return Err(game_identity_unavailable());
            };
            let bytes = hex::decode(file_id_hex).map_err(|_| game_identity_unavailable())?;
            let file_id: [u8; 16] = bytes.try_into().map_err(|_| game_identity_unavailable())?;
            unique.insert(ProcessFileIdentity {
                volume_serial_number,
                file_id,
            });
            unique.insert(current_registered_file_identity(path)?);
        }
        let mut identities = unique.into_iter().collect::<Vec<_>>();
        identities.sort_by(|left, right| {
            left.volume_serial_number
                .cmp(&right.volume_serial_number)
                .then_with(|| left.file_id.cmp(&right.file_id))
        });
        Ok(identities)
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
        let manual = request.executable_path.is_none();
        if manual {
            validate_manual_actions(&request.actions)?;
        } else {
            validate_automation_actions(&request.actions)?;
        }
        let conflict_policy = match request.conflict_policy.as_deref() {
            None | Some("abort_profile") => "abort_profile".to_owned(),
            Some("skip_conflicting") => "skip_conflicting".to_owned(),
            Some(_) => return Err(CoreError::invalid_request("競合方針の値が不正です。")),
        };

        let (executable_path, volume_serial_number, file_id_hex) = match request.executable_path {
            Some(path) => {
                let (canonical_path, volume_serial_number, file_id) = resolve_binding(&path)?;
                (
                    Some(canonical_path),
                    Some(volume_serial_number),
                    Some(hex::encode(file_id)),
                )
            }
            None => (None, None, None),
        };

        let profile = StoredProfile {
            id: Uuid::new_v4().to_string(),
            name: name.to_owned(),
            executable_path,
            volume_serial_number,
            file_id_hex,
            conflict_policy,
            automation_enabled: false,
            actions: request.actions,
            active_run: None,
        };

        let mut guard = self.profiles.lock();
        if guard.len() >= MAX_PROFILES {
            return Err(CoreError::invalid_request(
                "登録できるプロファイル数の上限に達しました。",
            ));
        }
        let mut next = guard.clone();
        next.push(profile.clone());
        Self::persist(&self.path, &next)?;
        *guard = next;
        Ok(profile)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> CoreResult<()> {
        let mut guard = self.profiles.lock();
        let index = guard
            .iter()
            .position(|profile| profile.id == id)
            .ok_or_else(|| CoreError::invalid_request("対象のプロファイルがありません。"))?;
        if enabled {
            if guard[index].is_manual() {
                return Err(CoreError::invalid_request(
                    "手動モードは自動適用を有効にできません。",
                ));
            }
            // 旧版で保存済みの定義や、将来の移行処理が混在しても、無人適用を
            // 許可していないActionを有効化の境界で必ず止める。
            validate_automation_actions(&guard[index].actions)?;
        }

        // copy-on-write: 永続化候補だけを変更し、原子的保存が成功してから
        // in-memory状態を置き換える。write/rename失敗時は旧状態を維持する。
        let mut next = guard.clone();
        next[index].automation_enabled = enabled;
        Self::persist(&self.path, &next)?;
        *guard = next;
        Ok(())
    }

    /// 現在のプロファイル定義を data-only の JSON バックアップとして書き出す。
    /// 任意コード・スクリプト・レジファイルは含まない(StoredProfile のデータのみ)。
    pub fn export_json(&self) -> CoreResult<String> {
        let file = ProfilesFile {
            version: PROFILES_FILE_VERSION,
            profiles: self.profiles.lock().clone(),
        };
        serde_json::to_string_pretty(&file).map_err(|_| CoreError::storage())
    }

    fn parse_import(json: &str) -> CoreResult<Vec<StoredProfile>> {
        let parsed: ProfilesFile = serde_json::from_str(json).map_err(|_| {
            CoreError::invalid_request("バックアップJSONを読めません。形式を確認してください。")
        })?;
        if parsed.version != PROFILES_FILE_VERSION {
            return Err(CoreError::invalid_request("バックアップの版が対応外です。"));
        }
        if parsed.profiles.len() > MAX_PROFILES {
            return Err(CoreError::invalid_request(
                "バックアップのプロファイル数が多すぎます。",
            ));
        }
        Ok(parsed.profiles)
    }

    /// インポート適用前に「この機で何が起きるか」を提示する(BRIEF: 実際に行う変更を一覧表示)。
    pub fn import_preview(&self, json: &str) -> CoreResult<Vec<ImportPreviewItem>> {
        let profiles = Self::parse_import(json)?;
        let mut items = Vec::with_capacity(profiles.len());
        for profile in profiles {
            // この機で実行ファイルが解決できるか(cross-PCは identity が変わる)。read-only。
            let (resolvable, note) = match profile.executable_path.as_deref() {
                None => (
                    true,
                    format!(
                        "手動モードとして{}件の準備を取り込みます",
                        profile.actions.len()
                    ),
                ),
                Some(path) => match resolve_binding(path) {
                    Ok(_) => (
                        true,
                        format!("{}件の準備を取り込みます", profile.actions.len()),
                    ),
                    Err(error) => (false, error.user_message),
                },
            };
            items.push(ImportPreviewItem {
                name: profile.name,
                executable_path: profile.executable_path.unwrap_or_default(),
                action_count: profile.actions.len(),
                resolvable,
                note,
            });
        }
        Ok(items)
    }

    /// バックアップから取り込む。各プロファイルはこの機で実行ファイルを再検証してから追加する。
    /// 解決できないものはスキップし理由を返す(黙って壊さない)。
    pub fn import_apply(&self, json: &str) -> CoreResult<ImportResult> {
        let profiles = Self::parse_import(json)?;
        let mut imported = Vec::new();
        let mut skipped = Vec::new();
        for profile in profiles {
            match self.create(CreateProfileRequest {
                name: profile.name.clone(),
                executable_path: profile.executable_path.clone(),
                conflict_policy: Some(profile.conflict_policy.clone()),
                actions: profile.actions.clone(),
            }) {
                Ok(created) => imported.push(created.name),
                Err(error) => skipped.push(ImportSkip {
                    name: profile.name,
                    reason: error.user_message,
                }),
            }
        }
        Ok(ImportResult { imported, skipped })
    }

    pub fn get(&self, id: &str) -> CoreResult<StoredProfile> {
        self.profiles
            .lock()
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
            .ok_or_else(|| CoreError::invalid_request("対象のプロファイルがありません。"))
    }

    pub fn set_active_run(&self, id: &str, run: ManualRunRecord) -> CoreResult<()> {
        let mut guard = self.profiles.lock();
        let index = guard
            .iter()
            .position(|profile| profile.id == id)
            .ok_or_else(|| CoreError::invalid_request("対象のプロファイルがありません。"))?;
        if !guard[index].is_manual() || guard[index].active_run.is_some() {
            return Err(CoreError::invalid_request(
                "この手動モードは現在実行できません。",
            ));
        }
        let mut next = guard.clone();
        next[index].active_run = Some(run);
        Self::persist(&self.path, &next)?;
        *guard = next;
        Ok(())
    }

    pub fn update_active_run_items(
        &self,
        id: &str,
        transaction_id: &str,
        item_ids: Vec<String>,
    ) -> CoreResult<()> {
        let mut guard = self.profiles.lock();
        let index = guard
            .iter()
            .position(|profile| profile.id == id)
            .ok_or_else(|| CoreError::invalid_request("対象のプロファイルがありません。"))?;
        let Some(active) = guard[index].active_run.as_ref() else {
            return Err(CoreError::invalid_request(
                "この手動モードは実行中ではありません。",
            ));
        };
        if active.transaction_id != transaction_id {
            return Err(CoreError::recovery_required(
                "手動モードの復元記録が一致しません。",
            ));
        }
        let mut next = guard.clone();
        next[index].active_run = (!item_ids.is_empty()).then(|| ManualRunRecord {
            transaction_id: transaction_id.to_owned(),
            reversible_item_ids: item_ids,
        });
        Self::persist(&self.path, &next)?;
        *guard = next;
        Ok(())
    }
    pub fn clear_active_run(&self, id: &str) -> CoreResult<()> {
        let mut guard = self.profiles.lock();
        let index = guard
            .iter()
            .position(|profile| profile.id == id)
            .ok_or_else(|| CoreError::invalid_request("対象のプロファイルがありません。"))?;
        let mut next = guard.clone();
        next[index].active_run = None;
        Self::persist(&self.path, &next)?;
        *guard = next;
        Ok(())
    }
    pub fn delete(&self, id: &str) -> CoreResult<()> {
        let mut guard = self.profiles.lock();
        if !guard.iter().any(|profile| profile.id == id) {
            return Err(CoreError::invalid_request(
                "対象のプロファイルがありません。",
            ));
        }
        if guard
            .iter()
            .any(|profile| profile.id == id && profile.active_run.is_some())
        {
            return Err(CoreError::invalid_request(
                "実行中の手動モードは、先に実行した分を復元してください。",
            ));
        }
        let mut next = guard.clone();
        next.retain(|profile| profile.id != id);
        Self::persist(&self.path, &next)?;
        *guard = next;
        Ok(())
    }

    /// 一時ファイルへ書いてから rename する原子的保存(Rust std は Windows で置換 rename)。
    fn persist(path: &PathBuf, profiles: &[StoredProfile]) -> CoreResult<()> {
        let file = ProfilesFile {
            version: PROFILES_FILE_VERSION,
            profiles: profiles.to_vec(),
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|_| CoreError::storage())?;
        if bytes.len() as u64 > MAX_PROFILES_FILE_BYTES {
            return Err(CoreError::invalid_request(
                "プロファイル定義の合計サイズが保存上限を超えています。",
            ));
        }
        let mut temp = path.clone();
        temp.set_extension("json.tmp");
        std::fs::write(&temp, &bytes).map_err(|_| CoreError::storage())?;
        std::fs::rename(&temp, path).map_err(|_| CoreError::storage())?;
        Ok(())
    }
}

/// 永続ファイルは将来の移行データや外部編集を含み得るため、起動時にも
/// 作成APIと同じ上限・型・自動適用境界を再検証する。
fn validate_loaded_profiles(profiles: &[StoredProfile]) -> CoreResult<()> {
    if profiles.len() > MAX_PROFILES {
        return Err(profiles_file_corrupt());
    }

    let mut ids = HashSet::with_capacity(profiles.len());
    for profile in profiles {
        if Uuid::parse_str(&profile.id).is_err() || !ids.insert(profile.id.clone()) {
            return Err(profiles_file_corrupt());
        }
        if profile.name.trim() != profile.name
            || profile.name.is_empty()
            || profile.name.chars().count() > 120
        {
            return Err(profiles_file_corrupt());
        }
        if profile.actions.len() > MAX_ACTIONS_PER_PROFILE {
            return Err(profiles_file_corrupt());
        }
        if !matches!(
            profile.conflict_policy.as_str(),
            "abort_profile" | "skip_conflicting"
        ) {
            return Err(profiles_file_corrupt());
        }
        match (
            profile.executable_path.as_deref(),
            profile.volume_serial_number,
            profile.file_id_hex.as_deref(),
        ) {
            (Some(path), Some(_), Some(file_id_hex)) => {
                if path.is_empty()
                    || path.chars().count() > MAX_EXECUTABLE_PATH_CHARS
                    || !Path::new(path).is_absolute()
                    || !path.to_ascii_lowercase().ends_with(".exe")
                    || hex::decode(file_id_hex)
                        .ok()
                        .filter(|bytes| bytes.len() == 16)
                        .is_none()
                    || profile.active_run.is_some()
                {
                    return Err(profiles_file_corrupt());
                }
                validate_automation_actions(&profile.actions)
                    .map_err(|_| profiles_file_corrupt())?;
            }
            (None, None, None) => {
                if profile.automation_enabled {
                    return Err(profiles_file_corrupt());
                }
                validate_manual_actions(&profile.actions).map_err(|_| profiles_file_corrupt())?;
                if let Some(run) = &profile.active_run {
                    if Uuid::parse_str(&run.transaction_id).is_err()
                        || run.reversible_item_ids.len() > MAX_ACTIONS_PER_PROFILE
                        || run
                            .reversible_item_ids
                            .iter()
                            .any(|id| Uuid::parse_str(id).is_err())
                    {
                        return Err(profiles_file_corrupt());
                    }
                }
            }
            _ => return Err(profiles_file_corrupt()),
        }
    }
    Ok(())
}

fn profiles_file_corrupt() -> CoreError {
    CoreError::new(
        "PROFILES_FILE_CORRUPT",
        "BOOTSTRAP",
        false,
        "プロファイル定義ファイルを安全に検証できません。破損または未対応の内容です。",
    )
}

fn game_identity_unavailable() -> CoreError {
    CoreError::recovery_required(
        "登録ゲームの本人性を再確認できないため、ウィンドウ操作を停止しました。",
    )
}

/// プロファイルはプロセス検知を起点に無人適用されるため、登録済みであることに加えて
/// Action側が明示的に自動適用を許可していることを保存・有効化の境界で確認する。
fn validate_automation_actions(actions: &[StoredProfileAction]) -> CoreResult<()> {
    for stored in actions {
        let parameters = parse_stored_profile_action(stored)?;
        let action_id = parameters.action_id();
        let action = crate::action::ACTION_REGISTRY
            .get(action_id)
            .ok_or_else(|| CoreError::invalid_request("登録済みActionを解決できませんでした。"))?;
        if !action.metadata().auto_apply_eligible
            || matches!(
                action.metadata().kind,
                crate::action::ActionKind::Observation | crate::action::ActionKind::Guided
            )
        {
            return Err(CoreError::invalid_request(
                "このActionは自動適用が許可されていないため、ゲームプロファイルには登録できません。",
            ));
        }
    }
    Ok(())
}

fn validate_manual_actions(actions: &[StoredProfileAction]) -> CoreResult<()> {
    if actions.is_empty() {
        return Err(CoreError::invalid_request(
            "手動モードには1件以上のActionを選んでください。",
        ));
    }
    for stored in actions {
        let parameters = parse_stored_profile_action(stored)?;
        if parameters.action_id() == crate::action::ActionId::SetupWindowLayout {
            return Err(CoreError::invalid_request(
                "ウィンドウ配置の復元は、配置画面で明示保存した内容だけを実行できます。",
            ));
        }
        let action = crate::action::ACTION_REGISTRY
            .get(parameters.action_id())
            .ok_or_else(|| CoreError::invalid_request("登録済みActionを解決できませんでした。"))?;
        if !matches!(
            action.metadata().kind,
            crate::action::ActionKind::Persistent
                | crate::action::ActionKind::Session
                | crate::action::ActionKind::OneWay
        ) {
            return Err(CoreError::invalid_request(
                "読み取り専用または案内専用Actionは手動モードの実行対象にできません。",
            ));
        }
    }
    Ok(())
}
pub(crate) fn parse_stored_profile_action(
    stored: &StoredProfileAction,
) -> CoreResult<crate::action::ActionParameters> {
    let action_id = stored
        .action_id
        .parse::<crate::action::ActionId>()
        .map_err(|_| CoreError::invalid_request("登録されていないActionは登録できません。"))?;
    let parameters =
        stored.parameters.as_object().cloned().ok_or_else(|| {
            CoreError::invalid_request("Actionの設定値はobjectで指定してください。")
        })?;
    let parsed =
        crate::presentation::parse_action_request(crate::presentation::PreviewActionRequest {
            action_id: stored.action_id.clone(),
            parameters,
        })?;
    if parsed.action_id() != action_id {
        return Err(CoreError::invalid_request(
            "Action IDと設定値の組み合わせが一致しません。",
        ));
    }
    Ok(parsed)
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

fn current_registered_file_identity(path: &str) -> CoreResult<ProcessFileIdentity> {
    crate::windows::registered_file_identity(path)
        .map(|(_, identity)| identity)
        .map_err(|_| game_identity_unavailable())
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

    fn block_persist_temp(store: &ProfileStore) {
        let mut blocking_temp = store.path.clone();
        blocking_temp.set_extension("json.tmp");
        std::fs::create_dir(&blocking_temp).expect("block temp-file write with directory");
    }

    fn eligible_request(name: &str) -> CreateProfileRequest {
        CreateProfileRequest {
            name: name.to_owned(),
            executable_path: Some(notepad()),
            conflict_policy: None,
            actions: vec![StoredProfileAction {
                action_id: "theme.color_mode".to_owned(),
                parameters: serde_json::json!({ "mode": "dark" }),
            }],
        }
    }

    fn persisted_profile(id: Uuid) -> StoredProfile {
        StoredProfile {
            id: id.to_string(),
            name: "保存済みテスト".to_owned(),
            executable_path: Some(notepad()),
            volume_serial_number: Some(1),
            file_id_hex: Some("000102030405060708090a0b0c0d0e0f".to_owned()),
            conflict_policy: "abort_profile".to_owned(),
            automation_enabled: false,
            actions: vec![StoredProfileAction {
                action_id: "theme.color_mode".to_owned(),
                parameters: serde_json::json!({ "mode": "dark" }),
            }],
            active_run: None,
        }
    }

    #[test]
    fn create_list_enable_delete_round_trips_on_disk() {
        let (store, _dir) = temp_store();
        let created = store
            .create(CreateProfileRequest {
                name: "  テストゲーム  ".to_owned(),
                executable_path: Some(notepad()),
                conflict_policy: None,
                actions: vec![StoredProfileAction {
                    action_id: "theme.color_mode".to_owned(),
                    parameters: serde_json::json!({ "mode": "dark" }),
                }],
            })
            .expect("create profile");
        assert_eq!(created.name, "テストゲーム"); // trim 済み
        assert!(!created.automation_enabled); // 既定は無効
        assert!(created
            .executable_path
            .as_ref()
            .expect("game binding")
            .to_lowercase()
            .ends_with("notepad.exe"));
        assert_eq!(
            created.file_id_hex.as_ref().expect("game file id").len(),
            32
        );

        // 別インスタンスで開き直しても永続化されている。
        let reopened = ProfileStore::open(store.path.clone()).expect("reopen");
        assert_eq!(reopened.list().len(), 1);

        store.set_enabled(&created.id, true).expect("enable");
        assert!(ProfileStore::open(store.path.clone()).unwrap().list()[0].automation_enabled);

        store.delete(&created.id).expect("delete");
        assert!(store.list().is_empty());
        assert!(ProfileStore::open(store.path.clone())
            .unwrap()
            .list()
            .is_empty());
    }

    #[test]
    fn game_exclusions_include_stored_and_fresh_executable_identities() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("profiles.json");
        let profile = persisted_profile(Uuid::new_v4());
        let stored = ProcessFileIdentity {
            volume_serial_number: profile.volume_serial_number.expect("stored volume"),
            file_id: hex::decode(profile.file_id_hex.as_deref().expect("stored file id"))
                .expect("hex")
                .try_into()
                .expect("16-byte file id"),
        };
        ProfileStore::persist(&path, &[profile]).expect("write test profile");
        let store = ProfileStore::open(path).expect("open test profile");

        let identities = store
            .registered_game_file_identities()
            .expect("resolve stored and current identities");
        let (_, current) =
            crate::windows::registered_file_identity(&notepad()).expect("current notepad identity");
        assert!(identities.contains(&stored));
        assert!(identities.contains(&current));
    }

    #[test]
    fn unreadable_registered_game_identity_fails_closed() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("profiles.json");
        let mut profile = persisted_profile(Uuid::new_v4());
        profile.executable_path = Some(
            directory
                .path()
                .join("missing-game.exe")
                .to_string_lossy()
                .into_owned(),
        );
        ProfileStore::persist(&path, &[profile]).expect("write test profile");
        let store = ProfileStore::open(path).expect("open test profile");

        let error = store
            .registered_game_file_identities()
            .expect_err("missing executable must stop window operations");
        assert_eq!(error.code, "RECOVERY_REQUIRED");
    }

    #[test]
    fn unknown_action_id_is_rejected() {
        let (store, _dir) = temp_store();
        let result = store.create(CreateProfileRequest {
            name: "x".to_owned(),
            executable_path: Some(notepad()),
            conflict_policy: None,
            actions: vec![StoredProfileAction {
                action_id: "totally.unknown".to_owned(),
                parameters: serde_json::Value::Null,
            }],
        });
        assert!(result.is_err());
    }

    #[test]
    fn manual_only_action_is_rejected_at_create_boundary() {
        let (store, _dir) = temp_store();
        let result = store.create(CreateProfileRequest {
            name: "x".to_owned(),
            executable_path: Some(notepad()),
            conflict_policy: None,
            actions: vec![StoredProfileAction {
                action_id: "taskbar.search_mode".to_owned(),
                parameters: serde_json::json!({ "mode": "hidden" }),
            }],
        });
        let error = result.expect_err("manual-only Action must be rejected");
        assert_eq!(error.code, "INVALID_REQUEST");
        assert!(error.user_message.contains("自動適用"));
        assert!(store.list().is_empty());
    }

    #[test]
    fn observation_action_is_rejected_at_create_boundary() {
        let (store, _dir) = temp_store();
        let result = store.create(CreateProfileRequest {
            name: "x".to_owned(),
            executable_path: Some(notepad()),
            conflict_policy: None,
            actions: vec![StoredProfileAction {
                action_id: "power.active_scheme_check".to_owned(),
                parameters: serde_json::json!({}),
            }],
        });
        let error = result.expect_err("observation Action must be rejected");
        assert_eq!(error.code, "INVALID_REQUEST");
        assert!(error.user_message.contains("自動適用"));
        assert!(store.list().is_empty());
    }

    #[test]
    fn malformed_action_parameters_are_rejected_before_profile_storage() {
        let (store, _dir) = temp_store();
        let result = store.create(CreateProfileRequest {
            name: "x".to_owned(),
            executable_path: Some(notepad()),
            conflict_policy: None,
            actions: vec![StoredProfileAction {
                action_id: "theme.color_mode".to_owned(),
                parameters: serde_json::json!({ "mode": "neon", "extra": true }),
            }],
        });
        let error = result.expect_err("malformed parameters must be rejected");
        assert_eq!(error.code, "INVALID_REQUEST");
        assert!(store.list().is_empty());
    }

    #[test]
    fn manual_only_action_is_rejected_when_legacy_profile_is_enabled() {
        let (store, _dir) = temp_store();
        let created = store
            .create(CreateProfileRequest {
                name: "x".to_owned(),
                executable_path: Some(notepad()),
                conflict_policy: None,
                actions: vec![StoredProfileAction {
                    action_id: "theme.color_mode".to_owned(),
                    parameters: serde_json::json!({ "mode": "dark" }),
                }],
            })
            .expect("create eligible profile");

        // 旧版ファイルから読み込まれた状態を再現する。外部I/Oのmockは使わず、
        // 実際のProfileStore有効化境界を通す。
        store.profiles.lock()[0].actions = vec![StoredProfileAction {
            action_id: "taskbar.search_mode".to_owned(),
            parameters: serde_json::json!({ "mode": "hidden" }),
        }];

        let error = store
            .set_enabled(&created.id, true)
            .expect_err("manual-only Action must not be enabled");
        assert!(error.user_message.contains("自動適用"));
        assert!(!store.list()[0].automation_enabled);
    }

    #[test]
    fn nonexistent_executable_is_rejected() {
        let (store, _dir) = temp_store();
        let result = store.create(CreateProfileRequest {
            name: "x".to_owned(),
            executable_path: Some(r"C:\definitely\not\here\ghost.exe".to_owned()),
            conflict_policy: None,
            actions: vec![],
        });
        assert!(result.is_err());
    }

    #[test]
    fn export_then_import_revalidates_and_round_trips() {
        let (store, _dir) = temp_store();
        store
            .create(CreateProfileRequest {
                name: "ゲームA".to_owned(),
                executable_path: Some(notepad()),
                conflict_policy: None,
                actions: vec![],
            })
            .expect("create source profile");
        let json = store.export_json().expect("export");
        assert!(json.contains("ゲームA"));

        // 別ストアへインポート → この機で再検証されて取り込まれる。
        let (other, _dir2) = temp_store();
        let preview = other.import_preview(&json).expect("preview");
        assert_eq!(preview.len(), 1);
        assert!(preview[0].resolvable, "存在するexeは解決可能");
        let result = other.import_apply(&json).expect("apply");
        assert_eq!(result.imported.len(), 1);
        assert!(result.skipped.is_empty());
        assert_eq!(other.list().len(), 1);
        // 取り込み後も既定は自動適用オフ。
        assert!(!other.list()[0].automation_enabled);
    }

    #[test]
    fn import_skips_profiles_whose_executable_is_missing_here() {
        // 別PC由来で、この機に実行ファイルが無いプロファイルはスキップ理由つきで返す。
        let json = format!(
            r#"{{"version":{PROFILES_FILE_VERSION},"profiles":[{{"id":"00000000-0000-0000-0000-000000000001","name":"どこにもないゲーム","executablePath":"C:\\nope\\ghost.exe","volumeSerialNumber":1,"fileIdHex":"{}","conflictPolicy":"abort_profile","automationEnabled":true,"actions":[]}}]}}"#,
            "0".repeat(32)
        );
        let (store, _dir) = temp_store();
        let preview = store.import_preview(&json).expect("preview");
        assert_eq!(preview.len(), 1);
        assert!(!preview[0].resolvable);
        let result = store.import_apply(&json).expect("apply");
        assert!(result.imported.is_empty());
        assert_eq!(result.skipped.len(), 1);
        assert!(store.list().is_empty());
    }

    #[test]
    fn set_enabled_persist_failure_keeps_memory_and_disk_unchanged() {
        let (store, _dir) = temp_store();
        let created = store
            .create(eligible_request("copy-on-write"))
            .expect("create disabled profile");
        assert!(!store.list()[0].automation_enabled);

        // persistが書く実tempパスをディレクトリにして、実filesystemのwrite失敗を
        // 安全なTempDir内で再現する。I/O mockや差し替えは使わない。
        block_persist_temp(&store);

        let error = store
            .set_enabled(&created.id, true)
            .expect_err("persist must fail");
        assert_eq!(error.code, "STORAGE_FAILURE");
        assert!(!store.list()[0].automation_enabled);

        // disk上の原本も有効化前のまま。
        let reopened = ProfileStore::open(store.path.clone()).expect("reopen original file");
        assert!(!reopened.list()[0].automation_enabled);
    }

    #[test]
    fn create_persist_failure_keeps_memory_and_disk_unchanged() {
        let (store, _dir) = temp_store();
        block_persist_temp(&store);

        let error = store
            .create(eligible_request("create-failure"))
            .expect_err("persist must fail");
        assert_eq!(error.code, "STORAGE_FAILURE");
        assert!(store.list().is_empty());
        assert!(!store.path.exists());

        let reopened = ProfileStore::open(store.path.clone()).expect("reopen absent original");
        assert!(reopened.list().is_empty());
    }

    #[test]
    fn delete_persist_failure_keeps_memory_and_disk_unchanged() {
        let (store, _dir) = temp_store();
        let created = store
            .create(eligible_request("delete-failure"))
            .expect("create profile");
        block_persist_temp(&store);

        let error = store.delete(&created.id).expect_err("persist must fail");
        assert_eq!(error.code, "STORAGE_FAILURE");
        assert_eq!(store.list(), vec![created.clone()]);

        let reopened = ProfileStore::open(store.path.clone()).expect("reopen original file");
        assert_eq!(reopened.list(), vec![created]);
    }

    #[test]
    fn open_rejects_profile_file_over_the_bounded_read_limit() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("profiles.json");
        std::fs::write(&path, vec![b' '; MAX_PROFILES_FILE_BYTES as usize + 1])
            .expect("write oversized profile file");

        let error = ProfileStore::open(path).expect_err("oversized file must fail closed");
        assert_eq!(error.code, "PROFILES_FILE_TOO_LARGE");
    }

    #[test]
    fn open_rejects_unknown_fields_and_duplicate_profile_ids() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("profiles.json");
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "version": PROFILES_FILE_VERSION,
                "profiles": [],
                "unexpected": true
            }))
            .expect("serialize unknown-field file"),
        )
        .expect("write unknown-field file");
        assert_eq!(
            ProfileStore::open(path.clone())
                .expect_err("unknown fields must fail closed")
                .code,
            "PROFILES_FILE_CORRUPT"
        );

        let id = Uuid::from_u128(7);
        let duplicated = ProfilesFile {
            version: PROFILES_FILE_VERSION,
            profiles: vec![persisted_profile(id), persisted_profile(id)],
        };
        std::fs::write(
            &path,
            serde_json::to_vec(&duplicated).expect("serialize duplicates"),
        )
        .expect("write duplicates");
        assert_eq!(
            ProfileStore::open(path)
                .expect_err("duplicate IDs must fail closed")
                .code,
            "PROFILES_FILE_CORRUPT"
        );
    }

    #[test]
    fn open_revalidates_action_schema_and_automation_eligibility() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("profiles.json");
        let mut profile = persisted_profile(Uuid::from_u128(8));
        profile.actions[0].parameters = serde_json::json!({ "mode": "neon" });
        std::fs::write(
            &path,
            serde_json::to_vec(&ProfilesFile {
                version: PROFILES_FILE_VERSION,
                profiles: vec![profile],
            })
            .expect("serialize malformed action"),
        )
        .expect("write malformed action");
        assert_eq!(
            ProfileStore::open(path.clone())
                .expect_err("malformed action parameters must fail closed")
                .code,
            "PROFILES_FILE_CORRUPT"
        );

        let mut profile = persisted_profile(Uuid::from_u128(9));
        profile.actions[0] = StoredProfileAction {
            action_id: "taskbar.search_mode".to_owned(),
            parameters: serde_json::json!({ "mode": "hidden" }),
        };
        std::fs::write(
            &path,
            serde_json::to_vec(&ProfilesFile {
                version: PROFILES_FILE_VERSION,
                profiles: vec![profile],
            })
            .expect("serialize manual-only action"),
        )
        .expect("write manual-only action");
        assert_eq!(
            ProfileStore::open(path)
                .expect_err("manual-only action must fail closed at open")
                .code,
            "PROFILES_FILE_CORRUPT"
        );
    }
    #[test]
    fn legacy_game_json_without_manual_fields_remains_readable() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("profiles.json");
        let json = serde_json::json!({
            "version": 1,
            "profiles": [{
                "id": Uuid::from_u128(100).to_string(),
                "name": "旧ゲーム",
                "executablePath": notepad(),
                "volumeSerialNumber": 1,
                "fileIdHex": "000102030405060708090a0b0c0d0e0f",
                "conflictPolicy": "abort_profile",
                "automationEnabled": false,
                "actions": [{"actionId": "theme.color_mode", "parameters": {"mode": "dark"}}]
            }]
        });
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        let loaded = ProfileStore::open(path)
            .expect("legacy version 1 game profile")
            .list();
        assert_eq!(loaded.len(), 1);
        assert!(!loaded[0].is_manual());
        assert!(loaded[0].active_run.is_none());
    }

    #[test]
    fn empty_manual_profile_is_rejected() {
        let (store, _dir) = temp_store();
        let error = store
            .create(CreateProfileRequest {
                name: "empty".to_owned(),
                executable_path: None,
                conflict_policy: None,
                actions: Vec::new(),
            })
            .expect_err("manual mode must contain an Action");
        assert_eq!(error.code, "INVALID_REQUEST");
    }

    #[test]
    fn manual_profile_has_no_executable_and_can_never_enable_automation() {
        let (store, _dir) = temp_store();
        let profile = store
            .create(CreateProfileRequest {
                name: "勉強".to_owned(),
                executable_path: None,
                conflict_policy: None,
                actions: vec![StoredProfileAction {
                    action_id: "setup.launch_apps".to_owned(),
                    parameters: serde_json::json!({"bundle": "study"}),
                }],
            })
            .expect("create manual mode with one-way action");
        assert!(profile.is_manual());
        assert_eq!(profile.volume_serial_number, None);
        assert_eq!(profile.file_id_hex, None);
        let error = store
            .set_enabled(&profile.id, true)
            .expect_err("manual mode must stay manual");
        assert_eq!(error.code, "INVALID_REQUEST");

        let exported = store.export_json().unwrap();
        assert!(!exported.contains("executablePath"));
        assert!(!exported.contains("volumeSerialNumber"));
        assert!(!exported.contains("fileIdHex"));
    }

    #[test]
    fn one_way_launch_action_is_rejected_for_automatic_game_profile() {
        let (store, _dir) = temp_store();
        let error = store
            .create(CreateProfileRequest {
                name: "ゲーム".to_owned(),
                executable_path: Some(notepad()),
                conflict_policy: None,
                actions: vec![StoredProfileAction {
                    action_id: "setup.launch_apps".to_owned(),
                    parameters: serde_json::json!({"bundle": "study"}),
                }],
            })
            .expect_err("one-way app launch must not enter automatic profile");
        assert_eq!(error.code, "INVALID_REQUEST");
    }
}
