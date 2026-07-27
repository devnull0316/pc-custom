//! 永続プロファイル定義(`StoredProfile`)を実行時状態機械へ橋渡しするランタイム。
//!
//! `ProcessMatcher`(検知) → `ProfileSupervisor`(適用/復元) を束ね、外からは
//! 「有効なプロファイル一覧を同期する」「プロセススナップショットを 1 tick 流す」の
//! 2 操作だけを見せる。実際のプロセス列挙(WMI/Toolhelp)と実適用(PcCustomEngine)は
//! `ObservedProcess` の供給と `ProfileActionSink` 実装という薄い I/O シムに閉じ込める。

use std::collections::{BTreeSet, HashSet};

use uuid::Uuid;

use crate::action::ActionId;
use crate::error::{CoreError, CoreResult};

use super::{
    ConflictPolicy, EventOutcome, ExitOutcome, GameProfile, GameProfileId, ObservedProcess,
    PlannedAction, ProcessMatcher, ProfileActionSink, ProfileBinding, ProfileError,
    ProfileSupervisor, ResourceIntent, StoredProfile, StoredProfileAction, TrackingMode,
};
use crate::action::ProcessFileIdentity;

/// 保存済み 1 Action を、resource ごとの desired 意図つきの実行時 Action へ変換する。
/// resource_key は登録済み Action メタデータから取得し、desired は正規化済みパラメータ列で表す
/// (同一パラメータ=共有可 / 異なるパラメータ=競合、を安定に判定するため)。
pub fn to_planned_action(stored: &StoredProfileAction) -> CoreResult<PlannedAction> {
    let parameters = super::store::parse_stored_profile_action(stored)?;
    let action_id: ActionId = parameters.action_id();
    let action = crate::action::ACTION_REGISTRY
        .get(action_id)
        .ok_or_else(|| CoreError::invalid_request("登録済みActionを解決できませんでした。"))?;
    if !action.metadata().auto_apply_eligible
        || matches!(
            action.metadata().kind,
            crate::action::ActionKind::Observation | crate::action::ActionKind::Guided
        )
    {
        // ProfileStoreを経由しない旧定義・移行データでも、プロセス検知からの
        // 無人適用へ到達させない最終防御。通常の手動preview/commitには影響しない。
        return Err(CoreError::invalid_request(
            "このActionは自動適用が許可されていないため、ゲームプロファイルでは実行できません。",
        ));
    }
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
    let executable_path = stored.executable_path.as_ref().ok_or_else(|| {
        CoreError::invalid_request("手動モードはプロセス監視の対象ではありません。")
    })?;
    let volume_serial_number = stored
        .volume_serial_number
        .ok_or_else(|| CoreError::invalid_request("プロファイルの本人性情報が不正です。"))?;
    let file_id_hex = stored
        .file_id_hex
        .as_ref()
        .ok_or_else(|| CoreError::invalid_request("プロファイルの本人性情報が不正です。"))?;
    let file_id_bytes = hex::decode(file_id_hex)
        .ok()
        .filter(|bytes| bytes.len() == 16)
        .ok_or_else(|| CoreError::invalid_request("プロファイルの本人性情報が不正です。"))?;
    let mut file_id = [0u8; 16];
    file_id.copy_from_slice(&file_id_bytes);

    let conflict_policy = match stored.conflict_policy.as_str() {
        "skip_conflicting" => ConflictPolicy::SkipConflicting,
        "abort_profile" => ConflictPolicy::AbortProfile,
        _ => {
            return Err(CoreError::invalid_request(
                "プロファイルの競合方針が不正です。",
            ))
        }
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
            canonical_path: executable_path.clone(),
            file_identity: ProcessFileIdentity {
                volume_serial_number,
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

        // 無効化・削除されたものは、matcherが保持するinstanceを終了扱いにして
        // 既適用resourceを先にrollbackする。seenを捨てるunregisterを先に行うと
        // Exitedを永久に失うため、復元完了後にだけ監視・定義を解除する。
        let to_remove: Vec<GameProfileId> =
            self.registered.difference(&desired_ids).copied().collect();
        for id in to_remove {
            for event in self.matcher.exit_events_before_unregister(id) {
                match self.supervisor.handle(event) {
                    Ok(EventOutcome::Exit(ExitOutcome::PartiallyFailed { .. })) => {
                        // supervisorはfailed参照を保持しない。EngineProfileSinkの
                        // durable journal/reconcileを唯一の復旧元として明示する。
                        skipped.push((
                            id.0.to_string(),
                            CoreError::recovery_required(
                                "無効化されたプロファイルの一部設定を復元できませんでした。変更記録からの復旧が必要です。",
                            ),
                        ));
                    }
                    Ok(_) => {}
                    Err(_) => {
                        skipped.push((
                            id.0.to_string(),
                            CoreError::recovery_required(
                                "無効化されたプロファイルの設定を復元できませんでした。変更記録からの復旧が必要です。",
                            ),
                        ));
                        break;
                    }
                }
            }
            if self.supervisor.is_active(id) {
                continue;
            }
            // 部分失敗も上でRECOVERY_REQUIREDとして呼び側へ返した上で、
            // runtimeには再試行可能なfailed stateが無いため監視状態を片付ける。
            self.supervisor.unregister_profile(id);
            self.matcher.unregister(id);
            self.registered.remove(&id);
        }

        skipped
    }

    /// プロセススナップショットを 1 回処理し、検知差分を状態機械へ流す。
    pub fn tick(
        &mut self,
        snapshot: &[ObservedProcess],
    ) -> Result<Vec<EventOutcome>, ProfileError> {
        let events = self.matcher.observe(snapshot);
        self.handle_events(events)
    }

    /// 列挙漏れの証拠があるスナップショットを処理する。
    /// 確認済みの起動だけを反映し、消失は完全なスナップショットまで保留する。
    pub fn tick_incomplete(
        &mut self,
        snapshot: &[ObservedProcess],
        confirmed_exits: &BTreeSet<super::InstanceKey>,
    ) -> Result<Vec<EventOutcome>, ProfileError> {
        let events = self.matcher.observe_incomplete(snapshot, confirmed_exits);
        self.handle_events(events)
    }

    pub fn missing_instances(&self, snapshot: &[ObservedProcess]) -> BTreeSet<super::InstanceKey> {
        self.matcher.missing_instances(snapshot)
    }

    pub fn tracked_instances(&self) -> BTreeSet<super::InstanceKey> {
        self.matcher.tracked_instances()
    }

    fn handle_events(
        &mut self,
        events: Vec<super::ObservedEvent>,
    ) -> Result<Vec<EventOutcome>, ProfileError> {
        let mut outcomes = Vec::with_capacity(events.len());
        for event in events {
            outcomes.push(self.supervisor.handle(event)?);
        }
        Ok(outcomes)
    }

    pub fn is_active(&self, profile: GameProfileId) -> bool {
        self.supervisor.is_active(profile)
    }

    /// 検知対象(有効かつ変換成功)のプロファイルが 1 件でもあるか。
    /// false の間は監視ループが重いプロセススナップショットを省ける。
    pub fn has_targets(&self) -> bool {
        !self.registered.is_empty()
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
        fail_rollbacks: bool,
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
            let mut log = self.log.borrow_mut();
            log.rollbacks += 1;
            if log.fail_rollbacks {
                Err(ProfileError::Sink("injected rollback failure".to_owned()))
            } else {
                Ok(())
            }
        }
    }

    fn stored(enabled: bool) -> StoredProfile {
        StoredProfile {
            id: Uuid::from_u128(42).to_string(),
            name: "テスト".to_owned(),
            executable_path: Some(r"C:\Games\Example\game.exe".to_owned()),
            volume_serial_number: Some(7),
            file_id_hex: Some("0102030405060708090a0b0c0d0e0f10".to_owned()),
            conflict_policy: "abort_profile".to_owned(),
            automation_enabled: enabled,
            actions: vec![StoredProfileAction {
                action_id: "theme.color_mode".to_owned(),
                parameters: serde_json::json!({ "mode": "dark" }),
            }],
            active_run: None,
        }
    }

    fn identity() -> ProcessFileIdentity {
        ProcessFileIdentity {
            volume_serial_number: 7,
            file_id: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
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
    fn manual_only_action_is_rejected_at_runtime_boundary() {
        let stored = StoredProfileAction {
            action_id: "taskbar.search_mode".to_owned(),
            parameters: serde_json::json!({ "mode": "hidden" }),
        };
        let error = to_planned_action(&stored).expect_err("manual-only Action must not be planned");
        assert_eq!(error.code, "INVALID_REQUEST");
        assert!(error.user_message.contains("自動適用"));
    }

    #[test]
    fn observation_action_is_rejected_at_runtime_boundary() {
        let stored = StoredProfileAction {
            action_id: "power.active_scheme_check".to_owned(),
            parameters: serde_json::json!({}),
        };
        let error = to_planned_action(&stored).expect_err("observation Action must not be planned");
        assert_eq!(error.code, "INVALID_REQUEST");
        assert!(error.user_message.contains("自動適用"));
    }

    #[test]
    fn malformed_parameters_are_rejected_at_runtime_boundary() {
        let stored = StoredProfileAction {
            action_id: "theme.color_mode".to_owned(),
            parameters: serde_json::json!({ "mode": "neon", "extra": true }),
        };
        let error = to_planned_action(&stored)
            .expect_err("legacy malformed parameters must not reach preview");
        assert_eq!(error.code, "INVALID_REQUEST");
    }

    #[test]
    fn runtime_applies_on_launch_and_restores_on_exit() {
        let sink = FakeSink {
            log: Rc::new(RefCell::new(Log::default())),
        };
        let log = sink.log.clone();
        let mut runtime = ProfileRuntime::new(sink);
        let profile = stored(true);
        assert!(runtime.sync(std::slice::from_ref(&profile)).is_empty());

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
    fn incomplete_snapshot_restores_only_with_individual_exit_confirmation() {
        let sink = FakeSink {
            log: Rc::new(RefCell::new(Log::default())),
        };
        let log = sink.log.clone();
        let mut runtime = ProfileRuntime::new(sink);
        assert!(runtime.sync(&[stored(true)]).is_empty());
        runtime.tick(&[proc(1000)]).expect("tick launch");
        assert_eq!(log.borrow().applies, 1);

        assert!(runtime
            .tick_incomplete(&[], &BTreeSet::new())
            .expect("partial snapshot")
            .is_empty());
        assert_eq!(log.borrow().rollbacks, 0);

        let confirmed = BTreeSet::from([proc(1000).instance]);
        let outcomes = runtime
            .tick_incomplete(&[], &confirmed)
            .expect("individually confirmed exit");
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
        bad.file_id_hex = Some("zz".to_owned()); // 不正な本人性
        let skipped = runtime.sync(&[bad]);
        assert_eq!(skipped.len(), 1);
    }

    fn assert_running_profile_is_restored_when_removed(next: Vec<StoredProfile>) {
        let sink = FakeSink {
            log: Rc::new(RefCell::new(Log::default())),
        };
        let log = sink.log.clone();
        let mut runtime = ProfileRuntime::new(sink);
        let profile = stored(true);
        let profile_id = GameProfileId(Uuid::parse_str(&profile.id).expect("profile id"));
        assert!(runtime.sync(&[profile]).is_empty());
        runtime.tick(&[proc(1000)]).expect("apply running profile");
        assert!(runtime.is_active(profile_id));
        assert_eq!(log.borrow().applies, 1);

        let skipped = runtime.sync(&next);
        assert!(skipped.is_empty());
        assert_eq!(log.borrow().rollbacks, 1);
        assert!(!runtime.is_active(profile_id));
        assert!(!runtime.has_targets());

        // 同じprocessが動作中でも、監視解除後に再適用されない。
        assert!(runtime
            .tick(&[proc(1000)])
            .expect("tick after removal")
            .is_empty());
        assert_eq!(log.borrow().applies, 1);
    }

    #[test]
    fn disabling_running_profile_restores_before_unregister() {
        assert_running_profile_is_restored_when_removed(vec![stored(false)]);
    }

    #[test]
    fn deleting_running_profile_restores_before_unregister() {
        assert_running_profile_is_restored_when_removed(Vec::new());
    }

    #[test]
    fn rollback_failure_during_sync_is_reported_for_journal_recovery() {
        let sink = FakeSink {
            log: Rc::new(RefCell::new(Log {
                fail_rollbacks: true,
                ..Log::default()
            })),
        };
        let log = sink.log.clone();
        let mut runtime = ProfileRuntime::new(sink);
        let profile = stored(true);
        let profile_id = GameProfileId(Uuid::parse_str(&profile.id).expect("profile id"));
        assert!(runtime.sync(&[profile]).is_empty());
        runtime.tick(&[proc(1000)]).expect("apply running profile");

        let skipped = runtime.sync(&[stored(false)]);
        assert_eq!(log.borrow().rollbacks, 1);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].1.code, "RECOVERY_REQUIRED");
        assert!(!runtime.is_active(profile_id));
        assert!(!runtime.has_targets());
    }
}
