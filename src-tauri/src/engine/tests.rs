use std::sync::Arc;

use serde_json::{Map, Value};
use tempfile::tempdir;
use uuid::Uuid;

use crate::{
    action::{ActionId, ActionParameters, ProcessFileIdentity},
    backup::{BackupDraft, BackupEnvelope, BackupPayload, Fingerprint, ObservationBackup},
    compatibility::OsIdentity,
    game_profile::{CreateProfileRequest, ProfileStore, StoredProfileAction},
    journal::{JournalDatabase, PreparedItem},
    presentation::{PreviewActionRequest, PreviewActionsRequest},
    window_layout::{
        SavedWindowPlacement, SavedWindowPlacementEntry, SensitiveWindowTitle,
        WindowLayoutSnapshot, WindowLayoutStore, WindowPoint, WindowRect,
    },
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

fn layout_snapshot(label: &str) -> WindowLayoutSnapshot {
    WindowLayoutSnapshot {
        snapshot_id: Uuid::new_v4(),
        captured_at_unix_ms: 1,
        entries: vec![SavedWindowPlacementEntry {
            entry_id: Uuid::new_v4(),
            process_file_identity: ProcessFileIdentity {
                volume_serial_number: 1,
                file_id: [2; 16],
            },
            application_label: "layout-test.exe".to_owned(),
            class_name: "PcCustomEngineLayoutTest".to_owned(),
            title: SensitiveWindowTitle::new(label.to_owned()).expect("bounded title"),
            placement: SavedWindowPlacement {
                flags: 0,
                show_cmd: 1,
                min_position: WindowPoint { x: -1, y: -1 },
                max_position: WindowPoint { x: -1, y: -1 },
                normal_position: WindowRect {
                    left: 10,
                    top: 20,
                    right: 410,
                    bottom: 320,
                },
            },
            observed_rect: WindowRect {
                left: 10,
                top: 20,
                right: 410,
                bottom: 320,
            },
        }],
        excluded_game_windows: 0,
        skipped_windows: 0,
    }
}

fn notepad() -> String {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
    format!(r"{root}\System32\notepad.exe")
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
    assert_eq!(
        engine
            .list_timeline(10)
            .expect("timeline remains readable")
            .len(),
        0
    );
}

#[test]
fn replaced_layout_or_changed_exclusions_invalidates_private_invocation() {
    let directory = tempdir().expect("runtime stores directory");
    let profile_store = Arc::new(
        ProfileStore::open(directory.path().join("profiles.json")).expect("profile store"),
    );
    let layout_store = Arc::new(
        WindowLayoutStore::open(directory.path().join("window-layout.json")).expect("layout store"),
    );
    layout_store
        .replace(layout_snapshot("first generation"))
        .expect("save first layout");
    let journal = Arc::new(JournalDatabase::open_in_memory().expect("journal"));
    let engine = PcCustomEngine::new_with_runtime_stores(
        journal,
        Some(OsIdentity::from_test_build(26_100)),
        Some(profile_store),
        Some(layout_store.clone()),
    )
    .expect("engine with runtime stores");
    let first = engine
        .window_layout_parameters()
        .expect("first private invocation");

    layout_store
        .replace(layout_snapshot("second generation"))
        .expect("replace saved layout");
    let stale = engine
        .ensure_layout_invocations_current(std::slice::from_ref(&first))
        .expect_err("replaced snapshot must invalidate invocation");
    assert_eq!(stale.code, "STALE_PREVIEW");

    let mut current = engine
        .window_layout_parameters()
        .expect("current private invocation");
    let ActionParameters::SetupWindowLayout { invocation } = &mut current else {
        panic!("window-layout parameters");
    };
    invocation
        .excluded_game_file_identities
        .push(ProcessFileIdentity {
            volume_serial_number: 9,
            file_id: [9; 16],
        });
    let stale = engine
        .ensure_layout_invocations_current(&[current])
        .expect_err("changed exclusion set must invalidate invocation");
    assert_eq!(stale.code, "STALE_PREVIEW");
}

#[test]
fn recovery_parameters_union_current_registered_game_identity() {
    let directory = tempdir().expect("runtime stores directory");
    let profile_store = Arc::new(
        ProfileStore::open(directory.path().join("profiles.json")).expect("profile store"),
    );
    profile_store
        .create(CreateProfileRequest {
            name: "recovery game".to_owned(),
            executable_path: Some(notepad()),
            conflict_policy: None,
            actions: vec![StoredProfileAction {
                action_id: "theme.color_mode".to_owned(),
                parameters: serde_json::json!({ "mode": "dark" }),
            }],
        })
        .expect("register current executable");
    let journal = Arc::new(JournalDatabase::open_in_memory().expect("journal"));
    let engine = PcCustomEngine::new_with_runtime_stores(
        journal,
        Some(OsIdentity::from_test_build(26_100)),
        Some(profile_store.clone()),
        None,
    )
    .expect("engine with profile store");
    let parameters = ActionParameters::SetupWindowLayout {
        invocation: crate::window_layout::WindowLayoutInvocation {
            desired: layout_snapshot("recovery target"),
            excluded_game_file_identities: Vec::new(),
        },
    };

    let effective = engine
        .recovery_parameters(&parameters)
        .expect("current exclusions can be added");
    let ActionParameters::SetupWindowLayout { invocation } = effective else {
        panic!("window-layout parameters");
    };
    assert_eq!(
        invocation.excluded_game_file_identities,
        profile_store
            .registered_game_file_identities()
            .expect("authoritative exclusions")
    );
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
    // この環境で変更が許される場合だけ、変更経路を検証する。
    // 許されない場合に互換性ゲートが読み取り専用へ倒すのは**仕様どおり**なので、
    // ここで失敗と報告してはいけない。
    //
    // 判定はビルド番号を自前で見ずに、製品と同じ `decision_for_identity` を使う。
    // 一度ビルド番号で書いて CI がまだ落ちた。実際の理由は build ではなく
    // product_type で、GitHub の windows ランナーは Windows Server（client でない）。
    // 判定ルールを二重に書くと、こうして必ずずれる。
    let decision = crate::compatibility::CompatibilityCatalog::decision_for_identity(&identity);
    if !matches!(
        decision.mode,
        crate::compatibility::CompatibilityMode::TestedMutable
    ) {
        println!(
            "この環境は変更対象外のため検証しない: build={} product_type={} 判定={:?}",
            identity.base_build, identity.product_type, decision.mode
        );
        return;
    }
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
    assert!(
        !preview.changes[0].before.is_empty(),
        "現在の状態が示される"
    );
    assert!(!preview.changes[0].after.is_empty(), "適用後が示される");

    // 2. 内容を確認して適用する
    let commit = engine
        .commit_preview(&preview.preview_token)
        .expect("commit succeeds");
    assert_eq!(
        commit.status, "succeeded",
        "適用が成功する: {}",
        commit.message
    );

    // 3. タイムラインに1件残り、戻せる状態になっている
    let timeline = engine.list_timeline(10).expect("timeline");
    let item = timeline
        .iter()
        .find(|entry| entry.action_id == "session.prevent_sleep")
        .expect("適用した項目が履歴に出る");
    assert_eq!(item.status, "succeeded");
    assert!(item.rollback_available, "この1件だけ戻せる");

    // 4. その1件だけを元へ戻す
    let rolled = engine
        .rollback_item(item.item_id)
        .expect("rollback succeeds");
    assert_eq!(
        rolled.status, "rolled_back",
        "復元が成功する: {}",
        rolled.message
    );

    let after = engine.list_timeline(10).expect("timeline after rollback");
    let restored = after
        .iter()
        .find(|entry| entry.item_id == item.item_id)
        .expect("同じ項目が残る");
    assert_eq!(restored.status, "rolled_back", "履歴に復元済みとして残る");
}

/// 試用は、確定しなければ期限切れとして拾われる。確定すれば拾われない。
///
/// 実際のロールバックまでは走らせない（実機の設定を触るため）。
/// ここで確かめるのは **期限の判定と確定の記録** で、そこが壊れると
/// 「保存したのに戻る」または「戻るはずが戻らない」のどちらかになる。
#[test]
fn trial_expires_and_is_reverted_unless_confirmed() {
    use crate::journal::JournalDatabase;

    let journal = JournalDatabase::open_in_memory().expect("journal");
    let transaction_id = Uuid::new_v4();
    // trials は transactions を参照するので、先に1件用意する。
    journal
        .record_prepared_transaction(transaction_id, "trial-test", "test", "26200", &[], 0)
        .expect("prepare transaction");

    // 期限が過去のものは拾われる。
    journal
        .begin_trial(transaction_id, 1_000, 500)
        .expect("begin");
    let expired = journal.expired_trials(2_000).expect("list");
    assert!(expired.contains(&transaction_id), "期限切れは拾われること");

    // まだ先のものは拾われない。
    journal
        .begin_trial(transaction_id, 9_000, 500)
        .expect("begin again");
    let not_yet = journal.expired_trials(2_000).expect("list");
    assert!(!not_yet.contains(&transaction_id), "期限前は拾わないこと");

    // 確定したものは、期限を過ぎても拾われない。
    journal
        .begin_trial(transaction_id, 1_000, 500)
        .expect("begin third");
    assert!(journal
        .confirm_trial(transaction_id, 1_500)
        .expect("confirm"));
    let after_confirm = journal.expired_trials(9_999).expect("list");
    assert!(
        !after_confirm.contains(&transaction_id),
        "確定済みは自動で戻さないこと"
    );

    // 二重確定は false を返す（既に確定済みで、何も変えない）。
    assert!(!journal
        .confirm_trial(transaction_id, 2_000)
        .expect("second confirm"));
}
