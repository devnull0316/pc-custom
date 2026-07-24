//! ゲームプロファイル状態機械の受入試験(GAME_PROFILES.md §11)。
//! 実 OS/DB に触れず、FakeSink で適用/復元の呼び出しを記録して検証する。

use std::cell::RefCell;
use std::rc::Rc;

use serde_json::Value;

use super::*;
use crate::action::ProcessFileIdentity;

#[derive(Default)]
struct FakeLog {
    /// 各 apply 呼び出しで適用された Action ID 群(呼び出し回数 = len)。
    applied: Vec<Vec<ActionId>>,
    /// 復元された参照(適用順の逆で呼ばれるはず)。
    rolled_back: Vec<String>,
    counter: u64,
    fail_rollback: bool,
}

#[derive(Clone)]
struct FakeSink {
    log: Rc<RefCell<FakeLog>>,
}

impl FakeSink {
    fn new() -> Self {
        Self {
            log: Rc::new(RefCell::new(FakeLog::default())),
        }
    }
    fn apply_calls(&self) -> usize {
        self.log.borrow().applied.len()
    }
    fn total_applied(&self) -> usize {
        self.log.borrow().applied.iter().map(Vec::len).sum()
    }
    fn rolled_back(&self) -> Vec<String> {
        self.log.borrow().rolled_back.clone()
    }
}

impl ProfileActionSink for FakeSink {
    fn apply(
        &mut self,
        _session: ProfileSessionId,
        actions: &[PlannedAction],
    ) -> Result<Vec<AppliedAction>, ProfileError> {
        let mut log = self.log.borrow_mut();
        log.applied.push(actions.iter().map(|a| a.action_id).collect());
        let mut out = Vec::with_capacity(actions.len());
        for action in actions {
            log.counter += 1;
            out.push(AppliedAction {
                action_id: action.action_id,
                reference: format!("ref-{}", log.counter),
            });
        }
        Ok(out)
    }

    fn rollback(&mut self, applied: &AppliedAction) -> Result<(), ProfileError> {
        let mut log = self.log.borrow_mut();
        if log.fail_rollback {
            return Err(ProfileError::Sink("forced rollback failure".to_owned()));
        }
        log.rolled_back.push(applied.reference.clone());
        Ok(())
    }
}

// --- ヘルパ ---

fn binding() -> ProfileBinding {
    ProfileBinding {
        canonical_path: r"C:\Games\Example\game.exe".to_owned(),
        file_identity: ProcessFileIdentity {
            volume_serial_number: 1,
            file_id: [0u8; 16],
        },
        tracking: TrackingMode::ExactExecutable,
    }
}

fn action(id: ActionId, key: &str, desired: &str, optional: bool) -> PlannedAction {
    PlannedAction {
        action_id: id,
        parameters_json: Value::Null,
        intents: vec![ResourceIntent {
            resource_key: key.to_owned(),
            desired: desired.to_owned(),
        }],
        optional,
    }
}

fn profile(
    seed: u128,
    actions: Vec<PlannedAction>,
    policy: ConflictPolicy,
) -> GameProfile {
    GameProfile {
        id: GameProfileId(Uuid::from_u128(seed)),
        name: format!("Profile {seed}"),
        binding: binding(),
        actions,
        conflict_policy: policy,
        automation_enabled: true,
    }
}

fn instance(pid: u32) -> InstanceKey {
    InstanceKey {
        process_id: pid,
        creation_time_100ns: u64::from(pid) * 1000,
    }
}

fn launched(p: &GameProfile, pid: u32) -> ObservedEvent {
    ObservedEvent::Launched {
        profile: p.id,
        instance: instance(pid),
    }
}
fn exited(p: &GameProfile, pid: u32) -> ObservedEvent {
    ObservedEvent::Exited {
        profile: p.id,
        instance: instance(pid),
    }
}

// --- 受入試験 ---

#[test]
fn single_profile_applies_on_launch_and_restores_on_exit() {
    let sink = FakeSink::new();
    let mut sup = ProfileSupervisor::new(sink.clone());
    let p = profile(
        1,
        vec![action(ActionId::ThemeColorMode, "theme.color_mode", "dark", false)],
        ConflictPolicy::AbortProfile,
    );
    sup.register_profile(p.clone());

    let out = sup.handle(launched(&p, 100)).unwrap();
    assert!(matches!(out, EventOutcome::Launch(LaunchOutcome::Applied { .. })));
    assert!(sup.is_active(p.id));
    assert_eq!(sink.total_applied(), 1);

    let out = sup.handle(exited(&p, 100)).unwrap();
    match out {
        EventOutcome::Exit(ExitOutcome::Restored { rolled_back }) => {
            assert_eq!(rolled_back.len(), 1);
        }
        other => panic!("expected Restored, got {other:?}"),
    }
    assert!(!sup.is_active(p.id));
    assert_eq!(sink.rolled_back(), vec!["ref-1".to_owned()]);
}

#[test]
fn duplicate_start_event_does_not_double_apply() {
    // §11-1
    let sink = FakeSink::new();
    let mut sup = ProfileSupervisor::new(sink.clone());
    let p = profile(
        1,
        vec![action(ActionId::ThemeColorMode, "theme.color_mode", "dark", false)],
        ConflictPolicy::AbortProfile,
    );
    sup.register_profile(p.clone());

    assert!(matches!(
        sup.handle(launched(&p, 100)).unwrap(),
        EventOutcome::Launch(LaunchOutcome::Applied { .. })
    ));
    // 同一 instance の重複起動イベント。
    assert_eq!(
        sup.handle(launched(&p, 100)).unwrap(),
        EventOutcome::Launch(LaunchOutcome::AlreadyActive)
    );
    assert_eq!(sink.apply_calls(), 1);
    assert_eq!(sink.total_applied(), 1);
}

#[test]
fn two_instances_apply_once_and_restore_once() {
    // §11-3
    let sink = FakeSink::new();
    let mut sup = ProfileSupervisor::new(sink.clone());
    let p = profile(
        1,
        vec![action(ActionId::ThemeColorMode, "theme.color_mode", "dark", false)],
        ConflictPolicy::AbortProfile,
    );
    sup.register_profile(p.clone());

    assert!(matches!(
        sup.handle(launched(&p, 100)).unwrap(),
        EventOutcome::Launch(LaunchOutcome::Applied { .. })
    ));
    assert_eq!(
        sup.handle(launched(&p, 200)).unwrap(),
        EventOutcome::Launch(LaunchOutcome::AlreadyActive)
    );
    // 1 instance 目終了 → まだ復元しない。
    assert_eq!(
        sup.handle(exited(&p, 100)).unwrap(),
        EventOutcome::Exit(ExitOutcome::StillActive)
    );
    assert!(sup.is_active(p.id));
    // 最後の instance 終了 → 1 回だけ復元。
    match sup.handle(exited(&p, 200)).unwrap() {
        EventOutcome::Exit(ExitOutcome::Restored { rolled_back }) => {
            assert_eq!(rolled_back.len(), 1);
        }
        other => panic!("expected Restored, got {other:?}"),
    }
    assert_eq!(sink.apply_calls(), 1);
    assert_eq!(sink.rolled_back().len(), 1);
}

#[test]
fn two_profiles_same_desired_share_lease_until_last_exit() {
    // §11-4
    let sink = FakeSink::new();
    let mut sup = ProfileSupervisor::new(sink.clone());
    let p1 = profile(
        1,
        vec![action(ActionId::ThemeColorMode, "theme.color_mode", "dark", false)],
        ConflictPolicy::AbortProfile,
    );
    let p2 = profile(
        2,
        vec![action(ActionId::ThemeColorMode, "theme.color_mode", "dark", false)],
        ConflictPolicy::AbortProfile,
    );
    sup.register_profile(p1.clone());
    sup.register_profile(p2.clone());

    assert!(matches!(
        sup.handle(launched(&p1, 100)).unwrap(),
        EventOutcome::Launch(LaunchOutcome::Applied { .. })
    ));
    // P2 は同一 desired → 相乗り(適用なし)。
    match sup.handle(launched(&p2, 200)).unwrap() {
        EventOutcome::Launch(LaunchOutcome::Applied { applied, joined, .. }) => {
            assert!(applied.is_empty());
            assert_eq!(joined, vec!["theme.color_mode".to_owned()]);
        }
        other => panic!("expected shared Applied, got {other:?}"),
    }
    assert_eq!(sink.apply_calls(), 1); // P1 の 1 回だけ

    // P1 終了 → まだ P2 が保持 → 復元しない。
    match sup.handle(exited(&p1, 100)).unwrap() {
        EventOutcome::Exit(ExitOutcome::Restored { rolled_back }) => {
            assert!(rolled_back.is_empty());
        }
        other => panic!("expected empty Restored, got {other:?}"),
    }
    assert_eq!(sink.rolled_back().len(), 0);
    // P2 終了 → 最後の owner → 復元。
    match sup.handle(exited(&p2, 200)).unwrap() {
        EventOutcome::Exit(ExitOutcome::Restored { rolled_back }) => {
            assert_eq!(rolled_back.len(), 1);
        }
        other => panic!("expected Restored, got {other:?}"),
    }
    assert_eq!(sink.rolled_back(), vec!["ref-1".to_owned()]);
}

#[test]
fn opposite_desired_second_profile_is_conflict_stopped() {
    // §11-5
    let sink = FakeSink::new();
    let mut sup = ProfileSupervisor::new(sink.clone());
    let p1 = profile(
        1,
        vec![action(ActionId::ThemeColorMode, "theme.color_mode", "dark", false)],
        ConflictPolicy::AbortProfile,
    );
    let p2 = profile(
        2,
        vec![action(ActionId::ThemeColorMode, "theme.color_mode", "light", false)],
        ConflictPolicy::AbortProfile,
    );
    sup.register_profile(p1.clone());
    sup.register_profile(p2.clone());

    assert!(matches!(
        sup.handle(launched(&p1, 100)).unwrap(),
        EventOutcome::Launch(LaunchOutcome::Applied { .. })
    ));
    match sup.handle(launched(&p2, 200)).unwrap() {
        EventOutcome::Launch(LaunchOutcome::ConflictStopped { conflicts }) => {
            assert_eq!(conflicts, vec!["theme.color_mode".to_owned()]);
        }
        other => panic!("expected ConflictStopped, got {other:?}"),
    }
    // P2 は何も適用していない。
    assert_eq!(sink.apply_calls(), 1);
    assert_eq!(sink.total_applied(), 1);

    // P2 終了 → 復元対象なし。P1 は影響を受けない。
    match sup.handle(exited(&p2, 200)).unwrap() {
        EventOutcome::Exit(ExitOutcome::Restored { rolled_back }) => assert!(rolled_back.is_empty()),
        other => panic!("expected empty Restored, got {other:?}"),
    }
    assert_eq!(sink.rolled_back().len(), 0);
    // P1 終了 → 先行状態を復元。
    match sup.handle(exited(&p1, 100)).unwrap() {
        EventOutcome::Exit(ExitOutcome::Restored { rolled_back }) => {
            assert_eq!(rolled_back.len(), 1);
        }
        other => panic!("expected Restored, got {other:?}"),
    }
}

#[test]
fn skip_policy_applies_nonconflicting_and_skips_conflicting() {
    let sink = FakeSink::new();
    let mut sup = ProfileSupervisor::new(sink.clone());
    let p1 = profile(
        1,
        vec![action(ActionId::ThemeColorMode, "theme.color_mode", "dark", false)],
        ConflictPolicy::AbortProfile,
    );
    let p2 = profile(
        2,
        vec![
            action(ActionId::ThemeColorMode, "theme.color_mode", "light", false),
            action(
                ActionId::ExplorerShowExtensions,
                "explorer.hide_file_ext",
                "show",
                false,
            ),
        ],
        ConflictPolicy::SkipConflicting,
    );
    sup.register_profile(p1.clone());
    sup.register_profile(p2.clone());

    sup.handle(launched(&p1, 100)).unwrap();
    match sup.handle(launched(&p2, 200)).unwrap() {
        EventOutcome::Launch(LaunchOutcome::Applied { applied, .. }) => {
            // 競合する theme は skip、show_extensions だけ適用。
            assert_eq!(applied.len(), 1);
            assert_eq!(applied[0].action_id, ActionId::ExplorerShowExtensions);
        }
        other => panic!("expected Applied(1), got {other:?}"),
    }
}

#[test]
fn optional_conflicting_action_is_skipped_under_abort_policy() {
    let sink = FakeSink::new();
    let mut sup = ProfileSupervisor::new(sink.clone());
    let p1 = profile(
        1,
        vec![action(ActionId::ThemeColorMode, "theme.color_mode", "dark", false)],
        ConflictPolicy::AbortProfile,
    );
    // optional=true の競合 Action は AbortProfile でも skip される。
    let p2 = profile(
        2,
        vec![action(ActionId::ThemeColorMode, "theme.color_mode", "light", true)],
        ConflictPolicy::AbortProfile,
    );
    sup.register_profile(p1.clone());
    sup.register_profile(p2.clone());

    sup.handle(launched(&p1, 100)).unwrap();
    match sup.handle(launched(&p2, 200)).unwrap() {
        EventOutcome::Launch(LaunchOutcome::Applied { applied, joined, .. }) => {
            assert!(applied.is_empty());
            assert!(joined.is_empty());
        }
        other => panic!("expected empty Applied, got {other:?}"),
    }
}

#[test]
fn disabled_or_unregistered_profile_is_ignored() {
    let sink = FakeSink::new();
    let mut sup = ProfileSupervisor::new(sink.clone());
    let mut p = profile(
        1,
        vec![action(ActionId::ThemeColorMode, "theme.color_mode", "dark", false)],
        ConflictPolicy::AbortProfile,
    );
    p.automation_enabled = false;
    sup.register_profile(p.clone());

    assert_eq!(
        sup.handle(launched(&p, 100)).unwrap(),
        EventOutcome::Launch(LaunchOutcome::Ignored)
    );
    assert_eq!(sink.apply_calls(), 0);
}

#[test]
fn rollback_failure_is_reported_not_swallowed() {
    let sink = FakeSink::new();
    sink.log.borrow_mut().fail_rollback = true;
    let mut sup = ProfileSupervisor::new(sink.clone());
    let p = profile(
        1,
        vec![action(ActionId::ThemeColorMode, "theme.color_mode", "dark", false)],
        ConflictPolicy::AbortProfile,
    );
    sup.register_profile(p.clone());

    sup.handle(launched(&p, 100)).unwrap();
    match sup.handle(exited(&p, 100)).unwrap() {
        EventOutcome::Exit(ExitOutcome::PartiallyFailed { rolled_back, failed }) => {
            assert!(rolled_back.is_empty());
            assert_eq!(failed.len(), 1);
        }
        other => panic!("expected PartiallyFailed, got {other:?}"),
    }
}
