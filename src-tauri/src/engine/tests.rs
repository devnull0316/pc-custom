use std::sync::Arc;

use serde_json::{Map, Value};
use tempfile::tempdir;
use uuid::Uuid;

use crate::{
    action::{ActionId, ActionParameters},
    backup::{
        BackupDraft, BackupEnvelope, BackupPayload, Fingerprint, ObservationBackup,
    },
    compatibility::OsIdentity,
    journal::{JournalDatabase, PreparedItem},
    presentation::{PreviewActionRequest, PreviewActionsRequest},
};

use super::PcCustomEngine;

fn power_observation_item(transaction_id: Uuid, item_id: Uuid) -> PreparedItem {
    let before = Fingerprint::of_bytes(b"power-before");
    let draft = BackupDraft {
        precondition_fingerprint: before,
        intended_fingerprint: before,
        payload: BackupPayload::Observation(ObservationBackup {
            source: "PowerGetActiveScheme test observation".to_owned(),
        }),
    };
    let backup = BackupEnvelope::from_draft(
        draft,
        transaction_id,
        item_id,
        ActionId::PowerActiveSchemeCheck,
        1,
        1,
        26_100,
    );
    PreparedItem {
        item_id,
        ordinal: 0,
        action_id: ActionId::PowerActiveSchemeCheck,
        action_version: 1,
        parameters: ActionParameters::PowerActiveSchemeCheck {},
        resource_keys: vec!["power:active-scheme:read".to_owned()],
        backup,
    }
}

#[test]
fn kill_after_item_applying_is_reconciled_on_reopen() {
    let directory = tempdir().expect("create isolated journal directory");
    let database_path = directory.path().join("kill-point.db");
    let transaction_id = Uuid::new_v4();
    let item_id = Uuid::new_v4();
    {
        let journal = JournalDatabase::open(&database_path).expect("open first journal");
        let prepared = power_observation_item(transaction_id, item_id);
        journal
            .record_prepared_transaction(
                transaction_id,
                "kill-point test",
                "test",
                "stable-os-fingerprint",
                &[prepared],
                1,
            )
            .expect("durably prepare backup");
        journal
            .mark_item_applying(transaction_id, item_id, 0, 2)
            .expect("persist applying kill point");
        assert!(journal.recovery_count().expect("count orphan") > 0);
        journal.checkpoint().expect("checkpoint kill point");
    }

    let reopened = Arc::new(JournalDatabase::open(&database_path).expect("reopen journal"));
    let engine = PcCustomEngine::new(
        Arc::clone(&reopened),
        Some(OsIdentity::from_test_build(26_100)),
    )
    .expect("startup reconcile succeeds");
    let timeline = engine.list_timeline(10).expect("load reconciled timeline");
    let item = timeline
        .iter()
        .find(|candidate| candidate.item_id == item_id)
        .expect("reconciled item remains visible");
    assert_eq!(item.status, "rolled_back");
    assert_eq!(reopened.recovery_count().expect("count after reconcile"), 0);
}

#[test]
fn journal_items_are_reversed_from_durable_apply_order() {
    let journal = JournalDatabase::open_in_memory().expect("open isolated journal");
    let transaction_id = Uuid::new_v4();
    let first = power_observation_item(transaction_id, Uuid::new_v4());
    let mut second = power_observation_item(transaction_id, Uuid::new_v4());
    let mut third = power_observation_item(transaction_id, Uuid::new_v4());
    second.ordinal = 1;
    third.ordinal = 2;
    let expected = vec![third.item_id, second.item_id, first.item_id];
    journal
        .record_prepared_transaction(
            transaction_id,
            "reverse-order test",
            "test",
            "stable-os-fingerprint",
            &[first, second, third],
            1,
        )
        .expect("prepare ordered items");
    let transactions = journal
        .load_recovery_transactions()
        .expect("load recovery order");
    let actual = transactions[0]
        .items
        .iter()
        .rev()
        .map(|item| item.item_id)
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn unknown_build_rejects_persistent_preview_without_registry_write() {
    let journal = Arc::new(JournalDatabase::open_in_memory().expect("open journal"));
    let engine = PcCustomEngine::new(journal, Some(OsIdentity::from_test_build(99_999)))
        .expect("start fail-closed engine");
    let mut parameters = Map::new();
    parameters.insert("show".to_owned(), Value::Bool(true));
    let error = engine
        .preview(PreviewActionsRequest {
            actions: vec![PreviewActionRequest {
                action_id: "explorer.show_extensions".to_owned(),
                parameters,
            }],
        })
        .expect_err("unknown build must reject persistent mutation preview");
    assert_eq!(error.code, "RECOVERY_REQUIRED");
    assert_eq!(engine.list_timeline(10).expect("timeline remains readable").len(), 0);
}

/// 実機・実エンジンでの通し確認。利用者が画面で行う順番そのままを、
/// preview → 適用 → タイムライン確認 → その1件だけ元へ戻す、まで通す。
///
/// 対象は `session.prevent_sleep`。OSの設定ファイルやレジストリを一切書かず、
/// このプロセスのスリープ抑止要求だけを扱うため、実機で走らせても副作用が残らない。
#[test]
fn full_user_journey_preview_commit_timeline_rollback_on_real_machine() {
    let identity = match OsIdentity::load() {
        Ok(identity) => identity,
        Err(_) => return, // 実機以外では検出できないので何も主張しない
    };
    let journal = Arc::new(JournalDatabase::open_in_memory().expect("open journal"));
    let engine = PcCustomEngine::new(journal, Some(identity)).expect("start engine");

    // 1. 利用者が「適用プレビュー」を押す
    let mut parameters = Map::new();
    parameters.insert("keepDisplayOn".to_owned(), Value::Bool(false));
    let preview = engine
        .preview(PreviewActionsRequest {
            actions: vec![PreviewActionRequest {
                action_id: "session.prevent_sleep".to_owned(),
                parameters,
            }],
        })
        .expect("preview succeeds on a supported build");
    assert_eq!(preview.changes.len(), 1, "変更内容が1件提示される");
    assert!(!preview.changes[0].before.is_empty(), "現在の状態が示される");
    assert!(!preview.changes[0].after.is_empty(), "適用後が示される");

    // 2. 内容を確認して適用する
    let commit = engine
        .commit_preview(&preview.preview_token)
        .expect("commit succeeds");
    assert_eq!(commit.status, "succeeded", "適用が成功する: {}", commit.message);

    // 3. タイムラインに1件残り、戻せる状態になっている
    let timeline = engine.list_timeline(10).expect("timeline");
    let item = timeline
        .iter()
        .find(|entry| entry.action_id == "session.prevent_sleep")
        .expect("適用した項目が履歴に出る");
    assert_eq!(item.status, "succeeded");
    assert!(item.rollback_available, "この1件だけ戻せる");

    // 4. その1件だけを元へ戻す
    let rolled = engine.rollback_item(item.item_id).expect("rollback succeeds");
    assert_eq!(rolled.status, "rolled_back", "復元が成功する: {}", rolled.message);

    let after = engine.list_timeline(10).expect("timeline after rollback");
    let restored = after
        .iter()
        .find(|entry| entry.item_id == item.item_id)
        .expect("同じ項目が残る");
    assert_eq!(restored.status, "rolled_back", "履歴に復元済みとして残る");
}
