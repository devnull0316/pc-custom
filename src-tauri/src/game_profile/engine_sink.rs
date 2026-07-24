//! ゲームプロファイルの実適用/復元シム。既存エンジンの公開経路
//! (preview → commit_preview → list_timeline / rollback_item)だけを使い、
//! エンジン本体には手を入れない。適用は「そのプロファイルが新規所有する Action 集合」を
//! 1 トランザクションとして通し、各 Action の journal item id を復元参照として返す。
//! item 単位の rollback により、lease 共有下でも所有者ごとに正しく戻せる。

use std::sync::Arc;

use crate::engine::TotonoeEngine;
use crate::error::CoreError;
use crate::presentation::{PreviewActionRequest, PreviewActionsRequest};

use super::{AppliedAction, PlannedAction, ProfileActionSink, ProfileError, ProfileSessionId};

pub struct EngineProfileSink {
    engine: Arc<TotonoeEngine>,
}

impl EngineProfileSink {
    pub fn new(engine: Arc<TotonoeEngine>) -> Self {
        Self { engine }
    }
}

impl ProfileActionSink for EngineProfileSink {
    fn apply(
        &mut self,
        _session: ProfileSessionId,
        actions: &[PlannedAction],
    ) -> Result<Vec<AppliedAction>, ProfileError> {
        if actions.is_empty() {
            return Ok(Vec::new());
        }
        let request = PreviewActionsRequest {
            actions: actions
                .iter()
                .map(|action| PreviewActionRequest {
                    action_id: action.action_id.as_str().to_owned(),
                    parameters: action
                        .parameters_json
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                })
                .collect(),
        };

        let preview = self.engine.preview(request).map_err(sink_error)?;
        let commit = self
            .engine
            .commit_preview(&preview.preview_token)
            .map_err(sink_error)?;
        if commit.status != "succeeded" {
            return Err(ProfileError::Sink(commit.message));
        }

        // commit したトランザクションの各 item id を journal から引く。
        let timeline = self.engine.list_timeline(256).map_err(sink_error)?;
        let mut applied = Vec::with_capacity(actions.len());
        for action in actions {
            let item = timeline
                .iter()
                .find(|entry| {
                    entry.transaction_id == commit.transaction_id
                        && entry.action_id == action.action_id.as_str()
                })
                .ok_or_else(|| {
                    ProfileError::Invariant(
                        "適用後のjournal項目を特定できませんでした。".to_owned(),
                    )
                })?;
            applied.push(AppliedAction {
                action_id: action.action_id,
                reference: item.item_id.to_string(),
            });
        }
        Ok(applied)
    }

    fn rollback(&mut self, applied: &AppliedAction) -> Result<(), ProfileError> {
        let item_id = uuid::Uuid::parse_str(&applied.reference)
            .map_err(|_| ProfileError::Invariant("復元参照が不正です。".to_owned()))?;
        let result = self.engine.rollback_item(item_id).map_err(sink_error)?;
        if result.status == "recovery_required" || result.status == "rollback_failed" {
            return Err(ProfileError::Sink(result.message));
        }
        Ok(())
    }
}

fn sink_error(error: CoreError) -> ProfileError {
    ProfileError::Sink(error.user_message)
}

/// 実機スモーク: 実 HKCU の HideFileExt に対して apply→rollback を通し、正確に原状復帰するか確認する。
/// 実ユーザー設定を一時的に変えるため既定の suite からは除外(`#[ignore]`)。
/// 明示実行: `cargo test --lib -- --ignored show_extensions_apply_then_rollback`。
#[cfg(all(test, windows))]
mod smoke {
    use std::sync::Arc;

    use uuid::Uuid;

    use super::EngineProfileSink;
    use crate::action::ActionId;
    use crate::backup::{read_registry_state, RegistryLocation, RegistryTarget, RegistryValueState};
    use crate::compatibility::OsIdentity;
    use crate::engine::TotonoeEngine;
    use crate::game_profile::{PlannedAction, ProfileActionSink, ProfileSessionId};
    use crate::journal::JournalDatabase;

    const ADVANCED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";

    /// 試験失敗で unwind しても、元の HideFileExt を必ず書き戻す安全網。
    struct RestoreGuard {
        location: RegistryLocation,
        original: RegistryValueState,
    }
    impl Drop for RestoreGuard {
        fn drop(&mut self) {
            if self.original.value_existed {
                if let Some(value_type) = self.original.value_type {
                    let _ = crate::windows::write_raw_value(
                        &self.location,
                        value_type,
                        &self.original.raw_bytes,
                    );
                }
            } else {
                let _ = crate::windows::delete_value(&self.location);
            }
        }
    }

    fn first_u32(bytes: &[u8]) -> u32 {
        let mut buffer = [0u8; 4];
        let count = bytes.len().min(4);
        buffer[..count].copy_from_slice(&bytes[..count]);
        u32::from_le_bytes(buffer)
    }

    #[test]
    #[ignore = "実HKCUのHideFileExtを一時変更する実機スモーク。--ignored で明示実行"]
    fn show_extensions_apply_then_rollback_restores_real_registry() {
        let location = RegistryTarget::current_user_64(ADVANCED, "HideFileExt").location();
        let original = read_registry_state(&location).expect("read original HideFileExt");
        let _guard = RestoreGuard {
            location: location.clone(),
            original: original.clone(),
        };

        // 現在値と必ず変わる向きを選ぶ(隠れていれば表示、表示なら隠す)。
        let current_hide = if original.value_existed {
            first_u32(&original.raw_bytes)
        } else {
            1
        };
        let show = current_hide != 0;
        let expected_hide_after = if show { 0u32 } else { 1u32 };

        let journal = Arc::new(JournalDatabase::open_in_memory().expect("in-memory journal"));
        let engine = Arc::new(
            TotonoeEngine::new(journal, Some(OsIdentity::from_test_build(26_200)))
                .expect("engine with a TestedMutable build"),
        );
        let mut sink = EngineProfileSink::new(engine);

        let action = PlannedAction {
            action_id: ActionId::ExplorerShowExtensions,
            parameters_json: serde_json::json!({ "show": show }),
            intents: Vec::new(),
            optional: false,
        };

        let applied = sink
            .apply(ProfileSessionId(Uuid::new_v4()), std::slice::from_ref(&action))
            .expect("apply via engine sink");
        assert_eq!(applied.len(), 1);

        let after_apply = read_registry_state(&location).expect("read after apply");
        assert!(after_apply.value_existed, "適用後は値が存在する");
        assert_eq!(
            first_u32(&after_apply.raw_bytes),
            expected_hide_after,
            "適用で実レジストリ値が変化する"
        );

        sink.rollback(&applied[0]).expect("rollback via engine sink");

        let after_rollback = read_registry_state(&location).expect("read after rollback");
        assert_eq!(
            after_rollback, original,
            "rollbackで元の状態(型・値・有無)へ正確復元する"
        );
    }
}
