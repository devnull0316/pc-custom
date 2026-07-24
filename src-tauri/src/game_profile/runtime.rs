//! 永続プロファイル定義(`StoredProfile`)を実行時状態機械へ橋渡しするランタイム。
//!
//! `ProcessMatcher`(検知) → `ProfileSupervisor`(適用/復元) を束ね、外からは
//! 「有効なプロファイル一覧を同期する」「プロセススナップショットを 1 tick 流す」の
//! 2 操作だけを見せる。実際のプロセス列挙(WMI/Toolhelp)と実適用(TotonoeEngine)は
//! `ObservedProcess` の供給と `ProfileActionSink` 実装という薄い I/O シムに閉じ込める。

use std::collections::HashSet;

use uuid::Uuid;

use crate::action::ActionId;
use crate::error::{CoreError, CoreResult};

use super::{
    ConflictPolicy, EventOutcome, GameProfile, GameProfileId, ObservedProcess, PlannedAction,
    ProcessMatcher, ProfileActionSink, ProfileBinding, ProfileError, ProfileSupervisor,
    ResourceIntent, StoredProfile, StoredProfileAction, TrackingMode,
};
use crate::action::ProcessFileIdentity;

/// 保存済み 1 Action を、resource ごとの desired 意図つきの実行時 Action へ変換する。
/// resource_key は登録済み Action メタデータから取得し、desired は正規化済みパラメータ列で表す
/// (同一パラメータ=共有可 / 異なるパラメータ=競合、を安定に判定するため)。
pub fn to_planned_action(stored: &StoredProfileAction) -> CoreResult<PlannedAction> {
    let action_id: ActionId = stored
        .action_id
        .parse()
        .map_err(|_| CoreError::invalid_request("登録されていないActionが含まれています。"))?;
    let action = crate::action::ACTION_REGISTRY.get(action_id).ok_or_else(|| {
        CoreError::invalid_request("登録済みActionを解決できませんでした。")
    })?;
    // BTreeMap ベースの serde_json はキー順が安定するため、desired トークンは決定的。
    let desired = serde_json::to_string(&stored.parameters).unwrap_or_default();
    let intents = action
        .metadata()
        .resource_keys
        .iter()
        .map(|key| ResourceIntent {
            resource_key: (*key).to_owned(),
            desired: desired.clone(),
        })
        .collect();
    Ok(PlannedAction {
        action_id,
        parameters_json: stored.parameters.clone(),
        intents,
        optional: false,
    })
}

pub fn to_game_profile(stored: &StoredProfile) -> CoreResult<GameProfile> {
    let id = Uuid::parse_str(&stored.id)
        .map_err(|_| CoreError::invalid_request("プロファイルIDが不正です。"))?;
    let file_id_bytes = hex::decode(&stored.file_id_hex)
        .ok()
        .filter(|bytes| bytes.len() == 16)
        .ok_or_else(|| CoreError::invalid_request("プロファイルの本人性情報が不正です。"))?;
    let mut file_id = [0u8; 16];
    file_id.copy_from_slice(&file_id_bytes);

    let conflict_policy = match stored.conflict_policy.as_str() {
        "skip_conflicting" => ConflictPolicy::SkipConflicting,
        _ => ConflictPolicy::AbortProfile,
    };

    let actions = stored
        .actions
        .iter()
        .map(to_planned_action)
        .collect::<CoreResult<Vec<_>>>()?;

    Ok(GameProfile {
        id: GameProfileId(id),
        name: stored.name.clone(),
        binding: ProfileBinding {
            canonical_path: stored.executable_path.clone(),
            file_identity: ProcessFileIdentity {
                volume_serial_number: stored.volume_serial_number,
                file_id,
            },
            tracking: TrackingMode::ExactExecutable,
        },
        actions,
        conflict_policy,
        automation_enabled: stored.automation_enabled,
    })
}

pub struct ProfileRuntime<S: ProfileActionSink> {
    matcher: ProcessMatcher,
    supervisor: ProfileSupervisor<S>,
    /// 現在検知対象として登録済み(＝有効かつ変換成功)のプロファイル。
    registered: HashSet<GameProfileId>,
}

impl<S: ProfileActionSink> ProfileRuntime<S> {
    pub fn new(sink: S) -> Self {
        Self {
            matcher: ProcessMatcher::new(),
            supervisor: ProfileSupervisor::new(sink),
            registered: HashSet::new(),
        }
    }

    /// 保存済みプロファイル一覧を反映する。自動適用が有効で変換に成功したものだけを検知対象にする。
    /// 変換に失敗したプロファイルはスキップし、理由を返す(呼び側がログ/表示する)。
    pub fn sync(&mut self, stored: &[StoredProfile]) -> Vec<(String, CoreError)> {
        let mut skipped = Vec::new();
        let mut desired_ids: HashSet<GameProfileId> = HashSet::new();

        for profile in stored {
            if !profile.automation_enabled {
                continue;
            }
            match to_game_profile(profile) {
                Ok(game_profile) => {
                    let id = game_profile.id;
                    self.matcher.register(id, &game_profile.binding);
                    self.supervisor.register_profile(game_profile);
                    self.registered.insert(id);
                    desired_ids.insert(id);
                }
                Err(error) => skipped.push((profile.id.clone(), error)),
            }
        }

        // 無効化・削除されたものは検知対象から外す(実行中セッションは終了検知まで維持)。
        let to_remove: Vec<GameProfileId> = self
            .registered
            .difference(&desired_ids)
            .copied()
            .collect();
        for id in to_remove {
            self.matcher.unregister(id);
            self.registered.remove(&id);
        }

        skipped
    }

    /// プロセススナップショットを 1 回処理し、検知差分を状態機械へ流す。
    pub fn tick(&mut self, snapshot: &[ObservedProcess]) -> Result<Vec<EventOutcome>, ProfileError> {
        let events = self.matcher.observe(snapshot);
        let mut outcomes = Vec::with_capacity(events.len());
        for event in events {
            outcomes.push(self.supervisor.handle(event)?);
        }
        Ok(outcomes)
    }

    pub fn is_active(&self, profile: GameProfileId) -> bool {
        self.supervisor.is_active(profile)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::game_profile::{AppliedAction, InstanceKey, LaunchOutcome, ProfileSessionId};

    #[derive(Default)]
    struct Log {
        applies: usize,
        rollbacks: usize,
        counter: u64,
    }
    #[derive(Clone)]
    struct FakeSink {
        log: Rc<RefCell<Log>>,
    }
    impl ProfileActionSink for FakeSink {
        fn apply(
            &mut self,
            _session: ProfileSessionId,
            actions: &[PlannedAction],
        ) -> Result<Vec<AppliedAction>, ProfileError> {
            let mut log = self.log.borrow_mut();
            log.applies += 1;
            let mut out = Vec::new();
            for action in actions {
                log.counter += 1;
                out.push(AppliedAction {
                    action_id: action.action_id,
                    reference: format!("r{}", log.counter),
                });
            }
            Ok(out)
        }
        fn rollback(&mut self, _applied: &AppliedAction) -> Result<(), ProfileError> {
            self.log.borrow_mut().rollbacks += 1;
            Ok(())
        }
    }

    fn stored(enabled: bool) -> StoredProfile {
        StoredProfile {
            id: Uuid::from_u128(42).to_string(),
            name: "テスト".to_owned(),
            executable_path: r"C:\Games\Example\game.exe".to_owned(),
            volume_serial_number: 7,
            file_id_hex: "0102030405060708090a0b0c0d0e0f10".to_owned(),
            conflict_policy: "abort_profile".to_owned(),
            automation_enabled: enabled,
            actions: vec![StoredProfileAction {
                action_id: "theme.color_mode".to_owned(),
                parameters: serde_json::json!({ "mode": "dark" }),
            }],
        }
    }

    fn identity() -> ProcessFileIdentity {
        ProcessFileIdentity {
            volume_serial_number: 7,
            file_id: [
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
            ],
        }
    }

    fn proc(pid: u32) -> ObservedProcess {
        ObservedProcess {
            instance: InstanceKey {
                process_id: pid,
                creation_time_100ns: u64::from(pid) * 1000,
            },
            canonical_path: r"C:\Games\Example\game.exe".to_owned(),
            file_identity: Some(identity()),
        }
    }

    #[test]
    fn conversion_derives_intents_from_metadata_and_params() {
        let planned = to_planned_action(&stored(true).actions[0]).expect("convert");
        assert_eq!(planned.action_id, ActionId::ThemeColorMode);
        assert!(!planned.intents.is_empty());
        // desired はパラメータ由来で決定的。
        assert!(planned.intents[0].desired.contains("dark"));
        // 同一パラメータは同一 desired、別パラメータは別 desired。
        let other = StoredProfileAction {
            action_id: "theme.color_mode".to_owned(),
            parameters: serde_json::json!({ "mode": "light" }),
        };
        let planned_light = to_planned_action(&other).unwrap();
        assert_ne!(planned.intents[0].desired, planned_light.intents[0].desired);
    }

    #[test]
    fn runtime_applies_on_launch_and_restores_on_exit() {
        let sink = FakeSink {
            log: Rc::new(RefCell::new(Log::default())),
        };
        let log = sink.log.clone();
        let mut runtime = ProfileRuntime::new(sink);
        let profile = stored(true);
        assert!(runtime.sync(&[profile.clone()]).is_empty());

        // ゲーム起動を検知 → 適用。
        let outcomes = runtime.tick(&[proc(1000)]).expect("tick launch");
        assert!(matches!(
            outcomes.as_slice(),
            [EventOutcome::Launch(LaunchOutcome::Applied { .. })]
        ));
        assert_eq!(log.borrow().applies, 1);

        // 終了を検知 → 復元。
        let outcomes = runtime.tick(&[]).expect("tick exit");
        assert!(matches!(outcomes.as_slice(), [EventOutcome::Exit(_)]));
        assert_eq!(log.borrow().rollbacks, 1);
    }

    #[test]
    fn disabled_profile_is_not_detected() {
        let sink = FakeSink {
            log: Rc::new(RefCell::new(Log::default())),
        };
        let log = sink.log.clone();
        let mut runtime = ProfileRuntime::new(sink);
        runtime.sync(&[stored(false)]); // automation 無効
        let outcomes = runtime.tick(&[proc(1000)]).expect("tick");
        assert!(outcomes.is_empty());
        assert_eq!(log.borrow().applies, 0);
    }

    #[test]
    fn invalid_profile_is_skipped_with_reason() {
        let sink = FakeSink {
            log: Rc::new(RefCell::new(Log::default())),
        };
        let mut runtime = ProfileRuntime::new(sink);
        let mut bad = stored(true);
        bad.file_id_hex = "zz".to_owned(); // 不正な本人性
        let skipped = runtime.sync(&[bad]);
        assert_eq!(skipped.len(), 1);
    }
}
