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

use super::TotonoeEngine;

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
    let engine = TotonoeEngine::new(
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
    let engine = TotonoeEngine::new(journal, Some(OsIdentity::from_test_build(99_999)))
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
