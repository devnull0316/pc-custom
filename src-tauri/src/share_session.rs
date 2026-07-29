//! A short-lived screen-sharing preparation session composed only from existing Actions.

use std::{io::Read, path::PathBuf, sync::Arc};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    action::ActionId,
    engine::PcCustomEngine,
    error::{CoreError, CoreResult},
    presentation::{PreviewActionRequest, PreviewActionsRequest},
};

const FILE_VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 16 * 1024;
const SESSION_ACTION_COUNT: usize = 2;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSessionState {
    pub active: bool,
    pub reversible_item_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSessionResult {
    pub status: String,
    pub message: String,
    pub details: Vec<String>,
    pub state: ShareSessionState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShareSessionRunRecord {
    transaction_id: String,
    rollback_item_ids: Vec<String>,
    sleep_item_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ShareSessionFile {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_run: Option<ShareSessionRunRecord>,
}

pub struct ShareSessionStore {
    path: PathBuf,
    state: Mutex<ShareSessionFile>,
    operation: Mutex<()>,
}

impl ShareSessionStore {
    pub fn open(path: PathBuf) -> CoreResult<Self> {
        let state = match std::fs::File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_FILE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| CoreError::storage())?;
                if bytes.len() as u64 > MAX_FILE_BYTES {
                    return Err(file_error(
                        "SHARE_SESSION_FILE_TOO_LARGE",
                        "画面共有セッションの記録が読込上限を超えています。",
                    ));
                }
                let parsed: ShareSessionFile = serde_json::from_slice(&bytes).map_err(|_| {
                    file_error(
                        "SHARE_SESSION_FILE_CORRUPT",
                        "画面共有セッションの記録を読み取れません。",
                    )
                })?;
                validate_file(&parsed)?;
                parsed
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => ShareSessionFile {
                version: FILE_VERSION,
                active_run: None,
            },
            Err(_) => return Err(CoreError::storage()),
        };
        Ok(Self {
            path,
            state: Mutex::new(state),
            operation: Mutex::new(()),
        })
    }

    pub fn state(&self) -> ShareSessionState {
        let state = self.state.lock();
        ShareSessionState {
            active: state.active_run.is_some(),
            reversible_item_count: state
                .active_run
                .as_ref()
                .map_or(0, |run| run.rollback_item_ids.len()),
        }
    }

    fn active_run(&self) -> Option<ShareSessionRunRecord> {
        self.state.lock().active_run.clone()
    }

    fn replace_active_run(&self, active_run: Option<ShareSessionRunRecord>) -> CoreResult<()> {
        let next = ShareSessionFile {
            version: FILE_VERSION,
            active_run,
        };
        let bytes = serde_json::to_vec_pretty(&next).map_err(|_| CoreError::storage())?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(CoreError::storage());
        }
        crate::settings_file::replace(&self.path, &bytes)?;
        *self.state.lock() = next;
        Ok(())
    }

    #[cfg(test)]
    fn active_run_for_test(&self) -> Option<ShareSessionRunRecord> {
        self.active_run()
    }

    #[cfg(test)]
    pub(crate) fn sleep_item_id_for_test(&self) -> Option<Uuid> {
        self.active_run()
            .and_then(|run| Uuid::parse_str(&run.sleep_item_id).ok())
    }
}

pub fn start(
    engine: Arc<PcCustomEngine>,
    store: Arc<ShareSessionStore>,
) -> CoreResult<ShareSessionResult> {
    let _operation = store.operation.lock();
    if store.active_run().is_some() {
        return Err(CoreError::invalid_request(
            "画面共有の準備は実行中です。先に終了してください。",
        ));
    }

    let preview = engine.preview_with_runtime_parameters(PreviewActionsRequest {
        actions: vec![
            PreviewActionRequest {
                action_id: ActionId::SetupWindowLayout.as_str().to_owned(),
                parameters: serde_json::Map::new(),
            },
            PreviewActionRequest {
                action_id: ActionId::SessionPreventSleep.as_str().to_owned(),
                parameters: serde_json::json!({ "keepDisplayOn": true })
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
            },
        ],
    })?;
    let commit = engine.commit_preview(&preview.preview_token)?;
    if commit.status != "succeeded" {
        return Err(CoreError::recovery_required(commit.message));
    }

    let expected = [
        ActionId::SessionPreventSleep.as_str(),
        ActionId::SetupWindowLayout.as_str(),
    ];
    let actual = commit
        .items
        .iter()
        .map(|item| item.action_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if commit.items.len() != SESSION_ACTION_COUNT
        || expected
            .into_iter()
            .any(|action_id| !actual.contains(action_id))
    {
        rollback_after_record_failure(&engine, &commit.items);
        return Err(CoreError::recovery_required(
            "画面共有の変更記録が不足しています。変更履歴を確認してください。",
        ));
    }

    let sleep_item_id = commit
        .items
        .iter()
        .find(|item| item.action_id == ActionId::SessionPreventSleep.as_str())
        .map(|item| item.item_id.to_string())
        .ok_or_else(|| {
            CoreError::recovery_required(
                "スリープ抑止の変更記録が見つかりません。変更履歴を確認してください。",
            )
        })?;
    let run = ShareSessionRunRecord {
        transaction_id: commit.transaction_id.to_string(),
        rollback_item_ids: commit
            .items
            .iter()
            .rev()
            .map(|item| item.item_id.to_string())
            .collect(),
        sleep_item_id,
    };
    if let Err(persist_error) = store.replace_active_run(Some(run)) {
        let rollback_failed = rollback_after_record_failure(&engine, &commit.items);
        if rollback_failed {
            return Err(CoreError::recovery_required(
                "画面共有の実行記録を保存できず、一部を復元できませんでした。変更履歴を確認してください。",
            ));
        }
        return Err(persist_error);
    }

    Ok(ShareSessionResult {
        status: "started".to_owned(),
        message: "画面共有の準備を始めました。終了すると、このセッションが変更した項目を戻します。"
            .to_owned(),
        details: commit.details,
        state: store.state(),
    })
}

pub fn finish(
    engine: Arc<PcCustomEngine>,
    store: Arc<ShareSessionStore>,
) -> CoreResult<ShareSessionResult> {
    let _operation = store.operation.lock();
    let run = store
        .active_run()
        .ok_or_else(|| CoreError::invalid_request("終了待ちの画面共有セッションはありません。"))?;
    let transaction_id = Uuid::parse_str(&run.transaction_id)
        .map_err(|_| CoreError::recovery_required("画面共有セッションの取引参照が不正です。"))?;
    let sleep_item_id = Uuid::parse_str(&run.sleep_item_id)
        .map_err(|_| CoreError::recovery_required("画面共有セッションの復元参照が不正です。"))?;
    let mut remaining = run.rollback_item_ids.clone();
    let mut details = Vec::new();

    while let Some(item_id_text) = remaining.first().cloned() {
        let item_id = Uuid::parse_str(&item_id_text).map_err(|_| {
            CoreError::recovery_required("画面共有セッションの復元参照が不正です。")
        })?;
        let (stored_transaction_id, stored_action_id, rollback_available) =
            engine.journal_item_identity(item_id)?.ok_or_else(|| {
                CoreError::recovery_required(
                    "画面共有セッションの変更記録が見つかりません。変更履歴を確認してください。",
                )
            })?;
        let expected_action_id = if item_id == sleep_item_id {
            ActionId::SessionPreventSleep
        } else {
            ActionId::SetupWindowLayout
        };
        if stored_transaction_id != transaction_id || stored_action_id != expected_action_id {
            return Err(CoreError::recovery_required(
                "画面共有セッションと変更記録の対応を確認できません。変更履歴を確認してください。",
            ));
        }
        if rollback_available {
            match engine.rollback_item(item_id) {
                Ok(result) if result.status == "rolled_back" => {
                    details.extend(result.details);
                }
                Ok(result) => return Err(CoreError::recovery_required(result.message)),
                Err(error) => {
                    let already_finished = engine.journal_item_identity(item_id)?.is_some_and(
                        |(current_transaction_id, current_action_id, available)| {
                            current_transaction_id == transaction_id
                                && current_action_id == expected_action_id
                                && !available
                        },
                    );
                    if !already_finished {
                        return Err(error);
                    }
                }
            }
        }
        remaining.remove(0);
        let active_run = if remaining.is_empty() {
            None
        } else {
            Some(ShareSessionRunRecord {
                transaction_id: run.transaction_id.clone(),
                rollback_item_ids: remaining.clone(),
                sleep_item_id: run.sleep_item_id.clone(),
            })
        };
        store.replace_active_run(active_run)?;
    }

    Ok(ShareSessionResult {
        status: "finished".to_owned(),
        message: "画面共有の準備を終了しました。このセッションが変更した窓と設定を戻しました。"
            .to_owned(),
        details,
        state: store.state(),
    })
}

fn rollback_after_record_failure(
    engine: &PcCustomEngine,
    items: &[crate::presentation::CommitItem],
) -> bool {
    let mut failed = false;
    for item in items.iter().rev() {
        match engine.rollback_item(item.item_id) {
            Ok(result) if result.status == "rolled_back" => {}
            _ => failed = true,
        }
    }
    failed
}

fn validate_file(file: &ShareSessionFile) -> CoreResult<()> {
    if file.version != FILE_VERSION {
        return Err(file_error(
            "SHARE_SESSION_FILE_VERSION",
            "画面共有セッションの記録の版が対応外です。",
        ));
    }
    let Some(run) = &file.active_run else {
        return Ok(());
    };
    let rollback_ids = run
        .rollback_item_ids
        .iter()
        .filter_map(|item_id| Uuid::parse_str(item_id).ok())
        .collect::<Vec<_>>();
    let sleep_item_id = Uuid::parse_str(&run.sleep_item_id).ok();
    if Uuid::parse_str(&run.transaction_id).is_err()
        || sleep_item_id.is_none()
        || run.rollback_item_ids.is_empty()
        || run.rollback_item_ids.len() > SESSION_ACTION_COUNT
        || rollback_ids.len() != run.rollback_item_ids.len()
        || rollback_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != rollback_ids.len()
        || !rollback_ids.contains(&sleep_item_id.expect("checked above"))
    {
        return Err(file_error(
            "SHARE_SESSION_FILE_INVALID",
            "画面共有セッションの復元参照を読み取れません。",
        ));
    }
    Ok(())
}

fn file_error(code: &'static str, message: &'static str) -> CoreError {
    CoreError::new(code, "BOOTSTRAP", false, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_round_trip_keeps_only_bounded_restore_references() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("share-session.json");
        let store = ShareSessionStore::open(path.clone()).unwrap();
        let sleep_item_id = Uuid::new_v4().to_string();
        let run = ShareSessionRunRecord {
            transaction_id: Uuid::new_v4().to_string(),
            rollback_item_ids: vec![Uuid::new_v4().to_string(), sleep_item_id.clone()],
            sleep_item_id,
        };
        store.replace_active_run(Some(run.clone())).unwrap();

        let reopened = ShareSessionStore::open(path).unwrap();
        assert_eq!(reopened.active_run_for_test(), Some(run));
        assert!(reopened.state().active);
        assert_eq!(reopened.state().reversible_item_count, 2);
    }

    #[test]
    fn screen_copy_has_three_explicit_groups_without_prohibited_claims() {
        let source = include_str!("../../src/components/ShareSessionPanel.tsx");
        for heading in [
            "このアプリが自動で確認・変更したもの",
            "利用者自身に確認してもらうもの",
            "確認できないもの",
        ] {
            assert!(source.contains(heading), "missing heading: {heading}");
        }
        for prohibited in ["安全", "絶対", "保証", "すべての通知", "準備完了"] {
            assert!(
                !source.contains(prohibited),
                "screen copy contains prohibited claim: {prohibited}"
            );
        }
    }
}
