//! ゲームプロファイルの核心ロジック。
//!
//! 責務: 登録ゲームの起動を検知したら、そのプロファイルの Action 集合を適用し、
//! 終了時に「このプロファイルが最後の所有者になった resource だけ」を復元する。
//! resource_key 単位の lease 共有 / 競合停止 / instance key による多重適用防止 /
//! 逆順復元を担う。実プロセス監視(WMI/Toolhelp)と実適用(TotonoeEngine)は
//! trait 越しに注入し、ここは OS にも DB にも直接触れない純粋な状態機械にする。
//!
//! GAME_PROFILES.md §3(状態機械) §5(多重適用と資源リース) §6(適用/復元順) §11(受入試験) を実装する。

use std::collections::{BTreeSet, HashMap, HashSet};

use uuid::Uuid;

use crate::action::{ActionId, ProcessFileIdentity};

// ---------------------------------------------------------------------------
// 識別子
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GameProfileId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProfileSessionId(pub Uuid);

/// プロセスの本人性。PID 再利用を避けるため作成時刻と組にする。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstanceKey {
    pub process_id: u32,
    pub creation_time_100ns: u64,
}

// ---------------------------------------------------------------------------
// プロファイル定義
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackingMode {
    /// 登録 EXE の各 instance が存在する間をプレイ中とする。
    ExactExecutable,
    /// ユーザーが明示登録した launcher/本体 EXE 集合で開始/終了を判定する。
    ExplicitProcessGroup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// 競合する必須 Action が 1 つでもあればプロファイル全体の自動適用を中止する(既定)。
    AbortProfile,
    /// 競合する Action だけ skip し、残りは適用する(ユーザーが明示選択した場合)。
    SkipConflicting,
}

/// ある resource を、どの desired 状態へ揃えたいか。desired は
/// 「同じ resource に対する要求が同一か反対か」を比較するための不透明トークン。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceIntent {
    pub resource_key: String,
    pub desired: String,
}

/// プロファイルが適用する 1 Action。parameters_json は sink(実適用層)専用で、
/// 状態機械の判断には使わない(コアはパラメータ非依存)。
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedAction {
    pub action_id: ActionId,
    pub parameters_json: serde_json::Value,
    pub intents: Vec<ResourceIntent>,
    /// ユーザーが「競合時に skip 可」と明示した任意 Action か。
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileBinding {
    pub canonical_path: String,
    pub file_identity: ProcessFileIdentity,
    pub tracking: TrackingMode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameProfile {
    pub id: GameProfileId,
    pub name: String,
    pub binding: ProfileBinding,
    pub actions: Vec<PlannedAction>,
    pub conflict_policy: ConflictPolicy,
    /// 実行ファイル再確認と互換性検査を通った場合だけ true。
    pub automation_enabled: bool,
}

// ---------------------------------------------------------------------------
// 注入シーム: 実適用 / 実復元
// ---------------------------------------------------------------------------

/// sink が適用済み変更を後で復元できるように返す不透明参照(実装では journal item / transaction id)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedAction {
    pub action_id: ActionId,
    pub reference: String,
}

pub trait ProfileActionSink {
    /// actions を 1 トランザクションとして適用し、各 Action の復元参照を順に返す。
    fn apply(
        &mut self,
        session: ProfileSessionId,
        actions: &[PlannedAction],
    ) -> Result<Vec<AppliedAction>, ProfileError>;

    /// 適用済み 1 Action を元へ戻す。
    fn rollback(&mut self, applied: &AppliedAction) -> Result<(), ProfileError>;
}

// ---------------------------------------------------------------------------
// イベント / 結果 / エラー
// ---------------------------------------------------------------------------

/// 観測層が binding 照合済みで渡すイベント。どの instance がどの profile かは呼び側が確定する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedEvent {
    Launched {
        profile: GameProfileId,
        instance: InstanceKey,
    },
    Exited {
        profile: GameProfileId,
        instance: InstanceKey,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum LaunchOutcome {
    /// 新規に適用した(applied)＋既存 lease に相乗りした(joined resource_key)。
    Applied {
        session: ProfileSessionId,
        applied: Vec<AppliedAction>,
        joined: Vec<String>,
    },
    /// 競合により自動適用を中止(適用も復元もしていない)。
    ConflictStopped { conflicts: Vec<String> },
    /// 既に別 instance が動作中、または重複イベント。適用しない。
    AlreadyActive,
    /// automation 無効・未登録などで何もしない。
    Ignored,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExitOutcome {
    /// 最後の owner が抜けた resource を復元した。
    Restored { rolled_back: Vec<AppliedAction> },
    /// 別 instance が残るため復元しない。
    StillActive,
    /// 復元中に失敗した項目がある(残りは復旧要)。
    PartiallyFailed {
        rolled_back: Vec<AppliedAction>,
        failed: Vec<AppliedAction>,
    },
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    Sink(String),
    Invariant(String),
}

// ---------------------------------------------------------------------------
// 内部状態
// ---------------------------------------------------------------------------

struct ResourceLease {
    desired: String,
    owners: BTreeSet<ProfileSessionId>,
    /// この resource を最初に適用した Action の復元参照(相乗り owner は復元参照を持たない)。
    applied: Option<AppliedAction>,
}

struct ActiveSession {
    #[allow(dead_code)]
    profile: GameProfileId,
    owned_resources: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionDecision {
    /// 新規 resource を含むので適用する。
    Apply,
    /// 全 resource が同一 desired で既存 → 相乗りのみ。
    JoinOnly,
    /// 競合するが skip 許容 → 適用しない。
    Skip,
    /// 競合し skip 不可 → プロファイル中止要因。
    Conflict,
}

// ---------------------------------------------------------------------------
// スーパーバイザ
// ---------------------------------------------------------------------------

pub struct ProfileSupervisor<S: ProfileActionSink> {
    sink: S,
    profiles: HashMap<GameProfileId, GameProfile>,
    active_instances: HashMap<GameProfileId, BTreeSet<InstanceKey>>,
    session_of_profile: HashMap<GameProfileId, ProfileSessionId>,
    sessions: HashMap<ProfileSessionId, ActiveSession>,
    leases: HashMap<String, ResourceLease>,
}

impl<S: ProfileActionSink> ProfileSupervisor<S> {
    pub fn new(sink: S) -> Self {
        Self {
            sink,
            profiles: HashMap::new(),
            active_instances: HashMap::new(),
            session_of_profile: HashMap::new(),
            sessions: HashMap::new(),
            leases: HashMap::new(),
        }
    }

    pub fn register_profile(&mut self, profile: GameProfile) {
        self.profiles.insert(profile.id, profile);
    }

    pub fn is_active(&self, profile: GameProfileId) -> bool {
        self.active_instances
            .get(&profile)
            .is_some_and(|set| !set.is_empty())
    }

    /// テスト/監視層から観測イベントを 1 件処理する。
    pub fn handle(&mut self, event: ObservedEvent) -> Result<EventOutcome, ProfileError> {
        match event {
            ObservedEvent::Launched { profile, instance } => {
                Ok(EventOutcome::Launch(self.on_launched(profile, instance)?))
            }
            ObservedEvent::Exited { profile, instance } => {
                Ok(EventOutcome::Exit(self.on_exited(profile, instance)?))
            }
        }
    }

    fn on_launched(
        &mut self,
        profile_id: GameProfileId,
        instance: InstanceKey,
    ) -> Result<LaunchOutcome, ProfileError> {
        let Some(profile) = self.profiles.get(&profile_id) else {
            return Ok(LaunchOutcome::Ignored);
        };
        if !profile.automation_enabled {
            return Ok(LaunchOutcome::Ignored);
        }

        let instances = self.active_instances.entry(profile_id).or_default();
        // 同一 instance の重複起動イベント → 冪等に無視(受入試験 §11-1)。
        if instances.contains(&instance) {
            return Ok(LaunchOutcome::AlreadyActive);
        }
        let was_empty = instances.is_empty();
        instances.insert(instance);
        // 既に別 instance が動作中 → 再適用しない(受入試験 §11-3 の 2 instance 目)。
        if !was_empty {
            return Ok(LaunchOutcome::AlreadyActive);
        }

        // ここから初回起動: セッションを開始し、lease を評価して適用する。
        let profile = self.profiles.get(&profile_id).expect("profile present").clone();
        let session = ProfileSessionId(Uuid::new_v4());

        // 各 Action の判断を先に決める(借用衝突を避けるため lease は読み取りのみ)。
        let mut decisions: Vec<ActionDecision> = Vec::with_capacity(profile.actions.len());
        for action in &profile.actions {
            decisions.push(self.decide_action(action, profile.conflict_policy));
        }

        // 競合(skip 不可)が 1 つでもあれば、AbortProfile 既定でプロファイル全体を中止する。
        let conflicts: Vec<String> = profile
            .actions
            .iter()
            .zip(&decisions)
            .filter(|(_, decision)| **decision == ActionDecision::Conflict)
            .flat_map(|(action, _)| action.intents.iter().map(|intent| intent.resource_key.clone()))
            .collect();
        if !conflicts.is_empty() {
            // 中止でも instance は記録済み。空セッションを登録し、終了時に active_instances を掃除する。
            self.session_of_profile.insert(profile_id, session);
            self.sessions.insert(
                session,
                ActiveSession {
                    profile: profile_id,
                    owned_resources: BTreeSet::new(),
                },
            );
            return Ok(LaunchOutcome::ConflictStopped { conflicts });
        }

        // 適用対象 Action(新規 resource を含む)を集める。
        let to_apply: Vec<PlannedAction> = profile
            .actions
            .iter()
            .zip(&decisions)
            .filter(|(_, decision)| **decision == ActionDecision::Apply)
            .map(|(action, _)| action.clone())
            .collect();

        let applied = if to_apply.is_empty() {
            Vec::new()
        } else {
            self.sink.apply(session, &to_apply)?
        };
        if applied.len() != to_apply.len() {
            return Err(ProfileError::Invariant(
                "sink が適用 Action 数と一致する復元参照を返しませんでした。".to_owned(),
            ));
        }

        // lease を更新する。Apply Action の新規 resource には復元参照を紐付け、
        // 既存同一 desired resource には owner を追加(相乗り)する。
        let mut owned: BTreeSet<String> = BTreeSet::new();
        let mut joined: Vec<String> = Vec::new();

        for (action, applied_ref) in to_apply.iter().zip(&applied) {
            for intent in &action.intents {
                owned.insert(intent.resource_key.clone());
                match self.leases.get_mut(&intent.resource_key) {
                    Some(lease) => {
                        // 既存 lease(同一 desired は decide_action で保証済み)へ相乗り。
                        lease.owners.insert(session);
                        joined.push(intent.resource_key.clone());
                    }
                    None => {
                        self.leases.insert(
                            intent.resource_key.clone(),
                            ResourceLease {
                                desired: intent.desired.clone(),
                                owners: BTreeSet::from([session]),
                                applied: Some(applied_ref.clone()),
                            },
                        );
                    }
                }
            }
        }

        // JoinOnly Action: 適用はしないが、全 resource の既存 lease へ owner を追加する。
        for (action, decision) in profile.actions.iter().zip(&decisions) {
            if *decision != ActionDecision::JoinOnly {
                continue;
            }
            for intent in &action.intents {
                owned.insert(intent.resource_key.clone());
                if let Some(lease) = self.leases.get_mut(&intent.resource_key) {
                    lease.owners.insert(session);
                    joined.push(intent.resource_key.clone());
                }
            }
        }

        self.session_of_profile.insert(profile_id, session);
        self.sessions.insert(
            session,
            ActiveSession {
                profile: profile_id,
                owned_resources: owned,
            },
        );

        Ok(LaunchOutcome::Applied {
            session,
            applied,
            joined,
        })
    }

    /// lease の現況だけを見て 1 Action の扱いを決める(適用や owner 変更はしない)。
    fn decide_action(&self, action: &PlannedAction, policy: ConflictPolicy) -> ActionDecision {
        let mut has_conflict = false;
        let mut has_new = false;
        let mut all_shared = true;
        for intent in &action.intents {
            match self.leases.get(&intent.resource_key) {
                Some(lease) if lease.desired == intent.desired => {
                    // 同一 desired の既存 lease → 相乗り可。
                }
                Some(_) => {
                    has_conflict = true;
                    all_shared = false;
                }
                None => {
                    has_new = true;
                    all_shared = false;
                }
            }
        }
        if has_conflict {
            let skippable = action.optional || policy == ConflictPolicy::SkipConflicting;
            return if skippable {
                ActionDecision::Skip
            } else {
                ActionDecision::Conflict
            };
        }
        if has_new {
            ActionDecision::Apply
        } else if all_shared && !action.intents.is_empty() {
            ActionDecision::JoinOnly
        } else {
            // intents が空の Action(観測など) → 適用扱いにし、sink 側で no-op を返させる。
            ActionDecision::Apply
        }
    }

    fn on_exited(
        &mut self,
        profile_id: GameProfileId,
        instance: InstanceKey,
    ) -> Result<ExitOutcome, ProfileError> {
        let Some(instances) = self.active_instances.get_mut(&profile_id) else {
            return Ok(ExitOutcome::Ignored);
        };
        if !instances.remove(&instance) {
            // 未知 / 重複の終了イベント。
            return Ok(ExitOutcome::Ignored);
        }
        if !instances.is_empty() {
            // 別 instance が残る → 復元しない(受入試験 §11-3)。
            return Ok(ExitOutcome::StillActive);
        }
        self.active_instances.remove(&profile_id);

        let Some(session) = self.session_of_profile.remove(&profile_id) else {
            return Ok(ExitOutcome::Ignored);
        };
        let Some(active) = self.sessions.remove(&session) else {
            return Ok(ExitOutcome::Ignored);
        };

        // このセッションが owner の resource から抜ける。owner が 0 になった resource だけ復元する。
        // 復元参照の重複(複数 resource を持つ Action)は一度だけ戻す。
        let mut to_restore: Vec<AppliedAction> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for resource_key in &active.owned_resources {
            let Some(lease) = self.leases.get_mut(resource_key) else {
                continue;
            };
            lease.owners.remove(&session);
            if lease.owners.is_empty() {
                if let Some(applied) = lease.applied.take() {
                    if seen.insert(applied.reference.clone()) {
                        to_restore.push(applied);
                    }
                }
                self.leases.remove(resource_key);
            }
        }

        // 適用の逆順に復元する(GAME_PROFILES.md §6-7)。
        to_restore.reverse();
        let mut rolled_back: Vec<AppliedAction> = Vec::new();
        let mut failed: Vec<AppliedAction> = Vec::new();
        for applied in to_restore {
            match self.sink.rollback(&applied) {
                Ok(()) => rolled_back.push(applied),
                Err(_) => failed.push(applied),
            }
        }

        if !failed.is_empty() {
            return Ok(ExitOutcome::PartiallyFailed {
                rolled_back,
                failed,
            });
        }
        Ok(ExitOutcome::Restored { rolled_back })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventOutcome {
    Launch(LaunchOutcome),
    Exit(ExitOutcome),
}

#[cfg(test)]
mod tests;
