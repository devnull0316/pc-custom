//! Explicit execution and restoration for profiles without an executable binding.

use std::sync::Arc;

use serde::Serialize;
use uuid::Uuid;

use crate::{
    engine::PcCustomEngine,
    error::{CoreError, CoreResult},
    presentation::{PreviewActionRequest, PreviewActionsRequest},
};

use super::{ManualRunRecord, ProfileStore};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualProfileResult {
    pub transaction_id: String,
    pub status: String,
    pub reversible_item_count: usize,
    pub message: String,
}

pub fn run_manual_profile(
    engine: Arc<PcCustomEngine>,
    store: Arc<ProfileStore>,
    id: &str,
) -> CoreResult<ManualProfileResult> {
    let profile = store.get(id)?;
    if !profile.is_manual() {
        return Err(CoreError::invalid_request(
            "ゲームプロファイルは『いま実行』の対象ではありません。",
        ));
    }
    if profile.active_run.is_some() {
        return Err(CoreError::invalid_request(
            "この手動モードは既に実行中です。先に実行した分を復元してください。",
        ));
    }
    if profile.actions.is_empty() {
        return Err(CoreError::invalid_request("実行するActionがありません。"));
    }

    let actions = profile
        .actions
        .iter()
        .map(|stored| {
            let parameters = super::store::parse_stored_profile_action(stored)?;
            let action = crate::action::ACTION_REGISTRY
                .get(parameters.action_id())
                .ok_or_else(|| {
                    CoreError::invalid_request("登録済みActionを解決できませんでした。")
                })?;
            if !matches!(
                action.metadata().kind,
                crate::action::ActionKind::Persistent
                    | crate::action::ActionKind::Session
                    | crate::action::ActionKind::OneWay
            ) {
                return Err(CoreError::invalid_request(
                    "手動モードで実行できないActionが含まれています。",
                ));
            }
            Ok(PreviewActionRequest {
                action_id: stored.action_id.clone(),
                parameters: stored.parameters.as_object().cloned().unwrap_or_default(),
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;

    let preview = engine.preview(PreviewActionsRequest { actions })?;
    let commit = engine.commit_preview(&preview.preview_token)?;
    if commit.status != "succeeded" {
        return Err(CoreError::recovery_required(commit.message));
    }

    // list_timeline returns transaction items in reverse durable ordinal, which is
    // exactly the rollback order. Persist that order rather than the profile UI order.
    let timeline = engine.list_timeline(128)?;
    let transaction_items = timeline
        .iter()
        .filter(|item| item.transaction_id == commit.transaction_id)
        .collect::<Vec<_>>();
    if transaction_items.len() != profile.actions.len() {
        return Err(CoreError::recovery_required(
            "手動モードの変更記録が不足しています。",
        ));
    }
    let reversible_item_ids = transaction_items
        .into_iter()
        .filter(|item| item.rollback_available)
        .map(|item| item.item_id.to_string())
        .collect::<Vec<_>>();
    if !reversible_item_ids.is_empty() {
        let record = ManualRunRecord {
            transaction_id: commit.transaction_id.to_string(),
            reversible_item_ids: reversible_item_ids.clone(),
        };
        if let Err(persist_error) = store.set_active_run(id, record) {
            let mut rollback_failed = false;
            for item_id in reversible_item_ids.iter().rev() {
                match Uuid::parse_str(item_id)
                    .ok()
                    .and_then(|item| engine.rollback_item(item).ok())
                {
                    Some(result) if result.status == "rolled_back" => {}
                    _ => rollback_failed = true,
                }
            }
            if rollback_failed {
                return Err(CoreError::recovery_required("手動モードの実行記録を保存できず、一部を復元できませんでした。変更履歴を確認してください。"));
            }
            return Err(persist_error);
        }
    }

    Ok(ManualProfileResult {
        transaction_id: commit.transaction_id.to_string(),
        status: "succeeded".to_owned(),
        reversible_item_count: reversible_item_ids.len(),
        message: if reversible_item_ids.is_empty() {
            "アプリを起動しました。起動したアプリは自動で終了しません。".to_owned()
        } else {
            "手動モードを実行しました。『実行した分を戻す』で可逆な変更だけを元へ戻せます。起動したアプリは終了しません。".to_owned()
        },
    })
}

pub fn restore_manual_profile(
    engine: Arc<PcCustomEngine>,
    store: Arc<ProfileStore>,
    id: &str,
) -> CoreResult<ManualProfileResult> {
    let profile = store.get(id)?;
    if !profile.is_manual() {
        return Err(CoreError::invalid_request(
            "ゲームプロファイルは手動復元の対象ではありません。",
        ));
    }
    let run = profile.active_run.ok_or_else(|| {
        CoreError::invalid_request("この手動モードに復元待ちの変更はありません。")
    })?;
    let mut remaining = run.reversible_item_ids.clone();
    let restored_count = remaining.len();
    while let Some(item_id) = remaining.first().cloned() {
        let item_id = Uuid::parse_str(&item_id)
            .map_err(|_| CoreError::recovery_required("手動モードの復元参照が不正です。"))?;
        let result = engine.rollback_item(item_id)?;
        if result.status != "rolled_back" {
            return Err(CoreError::recovery_required(result.message));
        }
        remaining.remove(0);
        store.update_active_run_items(id, &run.transaction_id, remaining.clone())?;
    }
    Ok(ManualProfileResult {
        transaction_id: run.transaction_id,
        status: "rolled_back".to_owned(),
        reversible_item_count: restored_count,
        message: "この手動モードが変更した可逆項目を逆順に元へ戻しました。起動したアプリは終了していません。".to_owned(),
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::{
        compatibility::OsIdentity,
        game_profile::{CreateProfileRequest, StoredProfileAction},
        journal::JournalDatabase,
    };

    #[test]
    fn manual_profile_uses_engine_journal_and_restores_its_items() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(ProfileStore::open(dir.path().join("profiles.json")).unwrap());
        let profile = store
            .create(CreateProfileRequest {
                name: "集中".to_owned(),
                executable_path: None,
                conflict_policy: None,
                ribbon_color: None,
                actions: vec![StoredProfileAction {
                    action_id: "session.prevent_sleep".to_owned(),
                    parameters: serde_json::json!({"keepDisplayOn": false}),
                }],
            })
            .unwrap();
        let journal = Arc::new(JournalDatabase::open_in_memory().unwrap());
        let engine = Arc::new(
            PcCustomEngine::new(journal, Some(OsIdentity::from_test_build(26_200))).unwrap(),
        );

        let run = run_manual_profile(engine.clone(), store.clone(), &profile.id).unwrap();
        assert_eq!(run.reversible_item_count, 1);
        assert!(store.get(&profile.id).unwrap().active_run.is_some());

        let restored = restore_manual_profile(engine, store.clone(), &profile.id).unwrap();
        assert_eq!(restored.status, "rolled_back");
        assert!(store.get(&profile.id).unwrap().active_run.is_none());
    }
}
