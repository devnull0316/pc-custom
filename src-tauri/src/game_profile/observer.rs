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
                file_identity: binding.file_identity.clone(),
            },
        );
        self.seen.entry(profile_id).or_default();
    }

    pub fn unregister(&mut self, profile_id: GameProfileId) {
        self.bindings.remove(&profile_id);
        self.seen.remove(&profile_id);
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
            for instance in previous.difference(&current) {
                events.push(ObservedEvent::Exited {
                    profile: *profile_id,
                    instance: *instance,
                });
            }
            *previous = current;
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
        assert_eq!(events, vec![ObservedEvent::Launched { profile: p, instance: pid(100) }]);

        // 同じスナップショットが続く間はイベントを出さない。
        assert!(m.observe(&[proc(100, path, Some(identity(7)))]).is_empty());

        // 消えたら Exited。
        let events = m.observe(&[]);
        assert_eq!(events, vec![ObservedEvent::Exited { profile: p, instance: pid(100) }]);
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
            instance: InstanceKey { process_id: 100, creation_time_100ns: 999_999 },
            canonical_path: path.to_owned(),
            file_identity: Some(identity(7)),
        };
        let events = m.observe(&[reused.clone()]);
        assert!(events.contains(&ObservedEvent::Exited { profile: p, instance: pid(100) }));
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
        assert_eq!(events, vec![ObservedEvent::Exited { profile: p, instance: pid(100) }]);
    }
}
