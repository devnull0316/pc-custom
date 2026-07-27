//! プロセス観測の中核(OS 非依存)。
//!
//! 実際の列挙(Toolhelp スナップショット / WMI process trace / handle wait)は
//! `ObservedProcess` の列を供給するだけにし、ここでは「どの instance がどの profile に
//! 一致するか」「前回との差分で Launched / Exited を出す」という判断だけを行う。
//! こうすることで PID 再利用・identity 不一致・多重 instance の扱いを完全にテストできる。
//!
//! 安全方針(GAME_PROFILES.md §4): 登録済み canonical path と file identity の**両方**が
//! 一致したときだけ本人と認める。identity を読めない(保護 process / access denied)場合は
//! 「不明」として一致させない = 自動適用しない。名前だけで別 EXE へ追従しない。

use std::collections::{BTreeSet, HashMap};

use crate::action::ProcessFileIdentity;

use super::{GameProfileId, InstanceKey, ObservedEvent, ProfileBinding};

/// 観測層が 1 プロセスについて確認できた事実。file_identity は読めない場合 None。
#[derive(Debug, Clone, PartialEq)]
pub struct ObservedProcess {
    pub instance: InstanceKey,
    pub canonical_path: String,
    pub file_identity: Option<ProcessFileIdentity>,
}

#[derive(Debug, Clone)]
struct MatchBinding {
    canonical_path_lower: String,
    file_identity: ProcessFileIdentity,
}

pub struct ProcessMatcher {
    bindings: HashMap<GameProfileId, MatchBinding>,
    /// profile ごとに現在一致中の instance 集合(前回スナップショット結果)。
    seen: HashMap<GameProfileId, BTreeSet<InstanceKey>>,
}

impl ProcessMatcher {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            seen: HashMap::new(),
        }
    }

    pub fn register(&mut self, profile_id: GameProfileId, binding: &ProfileBinding) {
        self.bindings.insert(
            profile_id,
            MatchBinding {
                canonical_path_lower: binding.canonical_path.to_ascii_lowercase(),
                file_identity: binding.file_identity,
            },
        );
        self.seen.entry(profile_id).or_default();
    }

    pub fn unregister(&mut self, profile_id: GameProfileId) {
        self.bindings.remove(&profile_id);
        self.seen.remove(&profile_id);
    }

    /// 登録解除前に、現在一致中として保持している全instanceの終了イベントを返す。
    ///
    /// この呼び出し自体は状態を捨てない。呼び側は終了イベントをsupervisorへ渡して
    /// 既適用resourceのrollbackを完了してから[`Self::unregister`]を呼ぶ。
    pub fn exit_events_before_unregister(&self, profile_id: GameProfileId) -> Vec<ObservedEvent> {
        self.seen
            .get(&profile_id)
            .into_iter()
            .flat_map(|instances| instances.iter().copied())
            .map(|instance| ObservedEvent::Exited {
                profile: profile_id,
                instance,
            })
            .collect()
    }

    /// 全体 snapshot から消えた、終了確認が必要な既知 instance。
    pub fn missing_instances(&self, snapshot: &[ObservedProcess]) -> BTreeSet<InstanceKey> {
        let mut missing = BTreeSet::new();
        for (profile_id, binding) in &self.bindings {
            let current: BTreeSet<InstanceKey> = snapshot
                .iter()
                .filter(|process| Self::is_match(binding, process))
                .map(|process| process.instance)
                .collect();
            if let Some(previous) = self.seen.get(profile_id) {
                missing.extend(previous.difference(&current).copied());
            }
        }
        missing
    }

    /// 全体列挙そのものが失敗した場合に個別確認する全既知 instance。
    pub fn tracked_instances(&self) -> BTreeSet<InstanceKey> {
        self.seen
            .values()
            .flat_map(|instances| instances.iter().copied())
            .collect()
    }

    fn is_match(binding: &MatchBinding, process: &ObservedProcess) -> bool {
        if process.canonical_path.to_ascii_lowercase() != binding.canonical_path_lower {
            return false;
        }
        // identity を確認できないものは「不明」として一致させない(安全側)。
        match &process.file_identity {
            Some(identity) => *identity == binding.file_identity,
            None => false,
        }
    }

    /// 1 スナップショットを受け取り、前回との差分イベントを返す。
    /// Launched を先に、Exited を後に返す(適用より先に消えることはない前提)。
    pub fn observe(&mut self, snapshot: &[ObservedProcess]) -> Vec<ObservedEvent> {
        self.observe_with_confirmed_exits(snapshot, None)
    }

    /// 不完全なスナップショットを処理する。
    ///
    /// 本人性を確認できた新規 instance の Launched は通知する一方、列挙漏れを終了と
    /// 誤認しないよう、個別の handle/creation-time 確認で終了が確定した instance だけ
    /// Exited にし、それ以外の既知 instance は保持する。
    pub fn observe_incomplete(
        &mut self,
        snapshot: &[ObservedProcess],
        confirmed_exits: &BTreeSet<InstanceKey>,
    ) -> Vec<ObservedEvent> {
        self.observe_with_confirmed_exits(snapshot, Some(confirmed_exits))
    }

    fn observe_with_confirmed_exits(
        &mut self,
        snapshot: &[ObservedProcess],
        confirmed_exits: Option<&BTreeSet<InstanceKey>>,
    ) -> Vec<ObservedEvent> {
        let mut events = Vec::new();
        for (profile_id, binding) in &self.bindings {
            let current: BTreeSet<InstanceKey> = snapshot
                .iter()
                .filter(|process| Self::is_match(binding, process))
                .map(|process| process.instance)
                .collect();
            let previous = self.seen.entry(*profile_id).or_default();

            // PID 再利用は instance key(PID + creation time)が変わるため、
            // 旧 instance は current から外れて Exited、新 instance は Launched になる。
            for instance in current.difference(previous) {
                events.push(ObservedEvent::Launched {
                    profile: *profile_id,
                    instance: *instance,
                });
            }
            let mut next = if confirmed_exits.is_some() {
                let mut retained = previous.clone();
                retained.extend(current.iter().copied());
                retained
            } else {
                current.clone()
            };
            for instance in previous.difference(&current) {
                if confirmed_exits.is_none_or(|confirmed| confirmed.contains(instance)) {
                    events.push(ObservedEvent::Exited {
                        profile: *profile_id,
                        instance: *instance,
                    });
                    next.remove(instance);
                }
            }
            *previous = next;
        }
        events
    }
}

impl Default for ProcessMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_profile::TrackingMode;
    use uuid::Uuid;

    fn identity(seed: u8) -> ProcessFileIdentity {
        ProcessFileIdentity {
            volume_serial_number: u64::from(seed) + 1,
            file_id: [seed; 16],
        }
    }

    fn binding(path: &str, id: ProcessFileIdentity) -> ProfileBinding {
        ProfileBinding {
            canonical_path: path.to_owned(),
            file_identity: id,
            tracking: TrackingMode::ExactExecutable,
        }
    }

    fn proc(pid: u32, path: &str, id: Option<ProcessFileIdentity>) -> ObservedProcess {
        ObservedProcess {
            instance: InstanceKey {
                process_id: pid,
                creation_time_100ns: u64::from(pid) * 1000,
            },
            canonical_path: path.to_owned(),
            file_identity: id,
        }
    }

    fn pid(id: u32) -> InstanceKey {
        InstanceKey {
            process_id: id,
            creation_time_100ns: u64::from(id) * 1000,
        }
    }

    #[test]
    fn detects_launch_then_exit() {
        let mut m = ProcessMatcher::new();
        let p = GameProfileId(Uuid::from_u128(1));
        let path = r"C:\Games\Example\game.exe";
        m.register(p, &binding(path, identity(7)));

        let events = m.observe(&[proc(100, path, Some(identity(7)))]);
        assert_eq!(
            events,
            vec![ObservedEvent::Launched {
                profile: p,
                instance: pid(100)
            }]
        );

        // 同じスナップショットが続く間はイベントを出さない。
        assert!(m.observe(&[proc(100, path, Some(identity(7)))]).is_empty());

        // 消えたら Exited。
        let events = m.observe(&[]);
        assert_eq!(
            events,
            vec![ObservedEvent::Exited {
                profile: p,
                instance: pid(100)
            }]
        );
    }

    #[test]
    fn identity_mismatch_is_not_followed() {
        // 同じパスでも file identity が違えば別物(差し替え/リネーム) → 一致させない。
        let mut m = ProcessMatcher::new();
        let p = GameProfileId(Uuid::from_u128(1));
        let path = r"C:\Games\Example\game.exe";
        m.register(p, &binding(path, identity(7)));
        assert!(m.observe(&[proc(100, path, Some(identity(9)))]).is_empty());
    }

    #[test]
    fn unreadable_identity_is_treated_as_unknown_not_match() {
        let mut m = ProcessMatcher::new();
        let p = GameProfileId(Uuid::from_u128(1));
        let path = r"C:\Games\Example\game.exe";
        m.register(p, &binding(path, identity(7)));
        // identity を読めない(None) → 一致させない(自動適用しない)。
        assert!(m.observe(&[proc(100, path, None)]).is_empty());
    }

    #[test]
    fn pid_reuse_emits_exit_then_launch_for_new_instance() {
        let mut m = ProcessMatcher::new();
        let p = GameProfileId(Uuid::from_u128(1));
        let path = r"C:\Games\Example\game.exe";
        m.register(p, &binding(path, identity(7)));
        m.observe(&[proc(100, path, Some(identity(7)))]);

        // 同じ PID 100 だが creation time が違う別 instance に置き換わった。
        let reused = ObservedProcess {
            instance: InstanceKey {
                process_id: 100,
                creation_time_100ns: 999_999,
            },
            canonical_path: path.to_owned(),
            file_identity: Some(identity(7)),
        };
        let events = m.observe(std::slice::from_ref(&reused));
        assert!(events.contains(&ObservedEvent::Exited {
            profile: p,
            instance: pid(100)
        }));
        assert!(events.contains(&ObservedEvent::Launched {
            profile: p,
            instance: reused.instance,
        }));
    }

    #[test]
    fn case_insensitive_path_match() {
        let mut m = ProcessMatcher::new();
        let p = GameProfileId(Uuid::from_u128(1));
        m.register(p, &binding(r"C:\Games\Example\game.exe", identity(7)));
        let events = m.observe(&[proc(100, r"c:\games\example\GAME.EXE", Some(identity(7)))]);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn multiple_instances_tracked_independently() {
        let mut m = ProcessMatcher::new();
        let p = GameProfileId(Uuid::from_u128(1));
        let path = r"C:\Games\Example\game.exe";
        m.register(p, &binding(path, identity(7)));
        let events = m.observe(&[
            proc(100, path, Some(identity(7))),
            proc(200, path, Some(identity(7))),
        ]);
        assert_eq!(events.len(), 2);
        // 片方だけ終了 → その instance だけ Exited。
        let events = m.observe(&[proc(200, path, Some(identity(7)))]);
        assert_eq!(
            events,
            vec![ObservedEvent::Exited {
                profile: p,
                instance: pid(100)
            }]
        );
    }

    #[test]
    fn incomplete_snapshot_never_emits_exit_and_retains_seen_instances() {
        let mut m = ProcessMatcher::new();
        let p = GameProfileId(Uuid::from_u128(1));
        let path = r"C:\Games\Example\game.exe";
        m.register(p, &binding(path, identity(7)));

        assert_eq!(
            m.observe(&[proc(100, path, Some(identity(7)))]),
            vec![ObservedEvent::Launched {
                profile: p,
                instance: pid(100),
            }]
        );
        assert!(m.observe_incomplete(&[], &BTreeSet::new()).is_empty());
        assert!(m.observe_incomplete(&[], &BTreeSet::new()).is_empty());
        assert_eq!(
            m.observe(&[]),
            vec![ObservedEvent::Exited {
                profile: p,
                instance: pid(100),
            }]
        );
    }

    #[test]
    fn incomplete_snapshot_can_add_verified_launch_without_forgetting_prior_seen() {
        let mut m = ProcessMatcher::new();
        let p = GameProfileId(Uuid::from_u128(1));
        let path = r"C:\Games\Example\game.exe";
        m.register(p, &binding(path, identity(7)));
        m.observe(&[proc(100, path, Some(identity(7)))]);

        assert_eq!(
            m.observe_incomplete(&[proc(200, path, Some(identity(7)))], &BTreeSet::new()),
            vec![ObservedEvent::Launched {
                profile: p,
                instance: pid(200),
            }]
        );

        let events = m.observe(&[]);
        assert_eq!(events.len(), 2);
        assert!(events.contains(&ObservedEvent::Exited {
            profile: p,
            instance: pid(100),
        }));
        assert!(events.contains(&ObservedEvent::Exited {
            profile: p,
            instance: pid(200),
        }));
    }

    #[test]
    fn incomplete_snapshot_emits_only_individually_confirmed_exit() {
        let mut m = ProcessMatcher::new();
        let p = GameProfileId(Uuid::from_u128(1));
        let path = r"C:\Games\Example\game.exe";
        m.register(p, &binding(path, identity(7)));
        m.observe(&[
            proc(100, path, Some(identity(7))),
            proc(200, path, Some(identity(7))),
        ]);

        let confirmed = BTreeSet::from([pid(100)]);
        assert_eq!(
            m.observe_incomplete(&[], &confirmed),
            vec![ObservedEvent::Exited {
                profile: p,
                instance: pid(100),
            }]
        );
        assert_eq!(m.tracked_instances(), BTreeSet::from([pid(200)]));
    }
}
