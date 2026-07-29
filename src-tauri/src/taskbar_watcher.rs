//! 「最大化しているときだけタスクバーを隠す」の判断部分。
//!
//! ここには Windows API を呼ぶコードを置かない。観測を受け取って、何をするかだけを決める。
//! 純関数にしておけば、ちらつきや所有権の規則を実機なしで固定できる。
//!
//! # 決めていること
//!
//! - **すぐには動かない。** 同じ観測が続けて決まった回数そろってから動く。
//!   前面の窓は一瞬で入れ替わる。追従すると、隠れたり出たりを繰り返す。
//! - **分からないときは動かない。** 全画面のゲームやシェルの UI は判断材料にしない。
//!   分からないまま隠すより、何もしないほうがよい。
//! - **人が触ったら手を引く。** 自分が最後に書いた値と現在値が違えば、
//!   それは利用者か別のアプリが変えたということ。以後この機能は自分から触らない。

use std::path::PathBuf;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};

const SETTING_FILE_VERSION: u32 = 1;
const MAX_SETTING_FILE_BYTES: u64 = 4 * 1024;

/// この機能を使うかどうか。**既定は切**。勝手にタスクバーを動かさない。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskbarAutoHideSetting {
    pub enabled: bool,
    /// いま自分が隠している最中で、戻す先はこの値。
    ///
    /// **プロセスが強制終了されると `Drop` は走らない。**
    /// 隠したままにして消えないよう、隠した時点でここへ書いておき、
    /// 次に起動したときに戻す。戻したら消す。
    #[serde(default)]
    pub hiding_restore_to: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SettingFile {
    version: u32,
    setting: TaskbarAutoHideSetting,
}

/// 画面へ返す状態。手を引いたかどうかも見せる。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskbarAutoHideState {
    pub enabled: bool,
    /// 誰かが手で変えたので、この機能が手を引いた状態。
    pub released: bool,
    /// 元から常に隠す設定なので、この機能の出番が無い。
    pub not_applicable: bool,
    pub last_error: Option<String>,
}

#[derive(Debug)]
pub struct TaskbarAutoHideStore {
    path: PathBuf,
    setting: Mutex<TaskbarAutoHideSetting>,
    released: Mutex<bool>,
    not_applicable: Mutex<bool>,
    last_error: Mutex<Option<String>>,
}

impl TaskbarAutoHideStore {
    pub fn open(path: PathBuf) -> CoreResult<Self> {
        let setting = match std::fs::metadata(&path) {
            Ok(metadata) if metadata.len() > MAX_SETTING_FILE_BYTES => {
                return Err(CoreError::invalid_request(
                    "タスクバー設定ファイルが上限を超えています。",
                ));
            }
            Ok(_) => {
                let bytes = std::fs::read(&path).map_err(|_| CoreError::storage())?;
                match serde_json::from_slice::<SettingFile>(&bytes) {
                    Ok(parsed) if parsed.version == SETTING_FILE_VERSION => parsed.setting,
                    // 読めない・版違いは既定（切）にする。勝手に入れない。
                    _ => TaskbarAutoHideSetting::default(),
                }
            }
            Err(_) => TaskbarAutoHideSetting::default(),
        };
        Ok(Self {
            path,
            setting: Mutex::new(setting),
            released: Mutex::new(false),
            not_applicable: Mutex::new(false),
            last_error: Mutex::new(None),
        })
    }

    pub fn get(&self) -> TaskbarAutoHideSetting {
        *self.setting.lock()
    }

    pub fn state(&self) -> TaskbarAutoHideState {
        TaskbarAutoHideState {
            enabled: self.setting.lock().enabled,
            released: *self.released.lock(),
            not_applicable: *self.not_applicable.lock(),
            last_error: self.last_error.lock().clone(),
        }
    }

    pub fn record_released(&self) {
        *self.released.lock() = true;
    }

    pub fn record_error(&self, error: Option<String>) {
        *self.last_error.lock() = error;
    }

    /// 隠したことを記録する。戻す先も一緒に持つ。
    pub fn record_hiding(&self, restore_to: bool) -> CoreResult<()> {
        let mut current = self.setting.lock();
        let mut updated = *current;
        updated.hiding_restore_to = Some(restore_to);
        self.persist(updated)?;
        *current = updated;
        Ok(())
    }

    /// 戻し終えた。記録を消す。
    pub fn clear_hiding(&self) -> CoreResult<()> {
        let mut current = self.setting.lock();
        let mut updated = *current;
        updated.hiding_restore_to = None;
        self.persist(updated)?;
        *current = updated;
        Ok(())
    }

    /// この環境では出番が無いと分かった。
    pub fn record_not_applicable(&self) {
        *self.not_applicable.lock() = true;
    }

    fn persist(&self, setting: TaskbarAutoHideSetting) -> CoreResult<()> {
        let file = SettingFile {
            version: SETTING_FILE_VERSION,
            setting,
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|_| CoreError::storage())?;
        crate::settings_file::replace(&self.path, &bytes)
    }

    pub fn set(&self, setting: TaskbarAutoHideSetting) -> CoreResult<TaskbarAutoHideSetting> {
        let mut current = self.setting.lock();
        self.persist(setting)?;
        *current = setting;
        // 入れ直したら、手を引いた記録も出番なしの記録も消す。もう一度やらせるということ。
        *self.released.lock() = false;
        *self.not_applicable.lock() = false;
        *self.last_error.lock() = None;
        Ok(setting)
    }
}

/// 観測1回分。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForegroundObservation {
    /// 前面の窓が最大化されている。
    Maximized,
    /// 前面の窓は最大化されていない。
    NotMaximized,
    /// 判断できない。全画面、シェルの UI、読み取り失敗など。
    Unknown,
}

/// この巡回で何をするか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatcherDecision {
    /// 何もしない。
    Idle,
    /// この環境ではこの機能に出番が無い。**一度も書かずに終わる。**
    NotApplicable,
    /// タスクバーを自動的に隠す設定にする。
    Hide,
    /// 自動的に隠す設定を解く。
    Show,
    /// 所有をやめる。以後この機能は自分からタスクバーへ書かない。
    ReleaseOwnership,
}

/// 何回そろったら動くか。3秒巡回なので、およそ6秒。
const REQUIRED_STREAK: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owned {
    /// 自分が隠す設定にしている。
    Hidden,
    /// 自分が解いている（＝利用者の元の状態）。
    Shown,
}

#[derive(Debug, Default)]
pub struct TaskbarWatcher {
    /// 直近に見た観測と、それが何回続いたか。
    streak: Option<(ForegroundObservation, u8)>,
    /// 自分が最後に書いた状態。まだ何も書いていなければ `None`。
    owned: Option<Owned>,
    /// 手を引いたあとは二度と書かない。
    released: bool,
    /// **利用者が元々どうしていたか。** 最初の観測で1回だけ覚える。
    ///
    /// これが無いまま「戻す」を書くと、戻した先が既定値になる。
    /// 変更前の状態へ戻すのであって、既定値を入れるのではない。
    baseline: Option<bool>,
    /// 元から常に隠す設定だった。この機能の出番が無い。
    not_applicable: bool,
}

impl TaskbarWatcher {
    /// 巡回1回分の判断。
    ///
    /// `auto_hide_now` は、いま実際にタスクバーが自動的に隠す設定かどうか。
    /// 自分が書いた値と食い違えば、誰かが触ったということ。
    pub fn evaluate(
        &mut self,
        foreground: ForegroundObservation,
        auto_hide_now: bool,
    ) -> WatcherDecision {
        if self.released {
            return WatcherDecision::Idle;
        }
        if self.not_applicable {
            return WatcherDecision::NotApplicable;
        }

        // 利用者が元々どうしていたかを、最初の1回だけ覚える。
        let baseline = *self.baseline.get_or_insert(auto_hide_now);
        if baseline {
            // 元から常に隠す設定だった。
            //
            // この機能が約束しているのは「最大化のときだけ隠す」。
            // 元が常に隠すなら、最大化していないときに**出す**ことになり、
            // それは利用者の設定を変えることになる。約束の外なので何もしない。
            // ここで `Show` を書くと、利用者が選んでいた設定を黙って解除する。
            self.not_applicable = true;
            return WatcherDecision::NotApplicable;
        }

        // 自分が書いた値と違っていたら、そこで手を引く。
        // **書き戻して取り返さない。** 利用者が変えたのかもしれない。
        if let Some(owned) = self.owned {
            let expected = matches!(owned, Owned::Hidden);
            if auto_hide_now != expected {
                self.released = true;
                self.streak = None;
                return WatcherDecision::ReleaseOwnership;
            }
        }

        // 分からないときは、続き具合も含めて何も進めない。
        if foreground == ForegroundObservation::Unknown {
            return WatcherDecision::Idle;
        }

        let streak = match self.streak {
            Some((previous, count)) if previous == foreground => count.saturating_add(1),
            _ => 1,
        };
        self.streak = Some((foreground, streak));
        if streak < REQUIRED_STREAK {
            return WatcherDecision::Idle;
        }

        let wanted = match foreground {
            ForegroundObservation::Maximized => Owned::Hidden,
            ForegroundObservation::NotMaximized => Owned::Shown,
            ForegroundObservation::Unknown => return WatcherDecision::Idle,
        };
        if self.owned == Some(wanted) {
            return WatcherDecision::Idle;
        }
        // まだ一度も隠していないのに「出す」を書かない。
        // 書く理由が無い。書けば、触っていない設定を触ったことになる。
        if wanted == Owned::Shown && self.owned.is_none() {
            return WatcherDecision::Idle;
        }
        self.owned = Some(wanted);
        match wanted {
            Owned::Hidden => WatcherDecision::Hide,
            Owned::Shown => WatcherDecision::Show,
        }
    }

    /// 手を引いたかどうか。画面に出すために使う。
    pub const fn has_released(&self) -> bool {
        self.released
    }

    /// 自分がいま隠す設定にしているか。終了時に戻すかどうかの判断に使う。
    pub const fn owns_hidden(&self) -> bool {
        matches!(self.owned, Some(Owned::Hidden))
    }

    /// この環境では出番が無いと分かったか。画面にそう出すために使う。
    pub const fn is_not_applicable(&self) -> bool {
        self.not_applicable
    }

    /// 戻す先。**既定値ではなく、最初に観測した利用者の状態。**
    /// 一度も観測していなければ `None`（戻す対象が無い）。
    pub const fn baseline(&self) -> Option<bool> {
        self.baseline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ForegroundObservation::{Maximized, NotMaximized, Unknown};
    use WatcherDecision::{Hide, Idle, NotApplicable, ReleaseOwnership, Show};

    #[test]
    fn one_sighting_is_not_enough_to_move_the_taskbar() {
        let mut watcher = TaskbarWatcher::default();
        assert_eq!(watcher.evaluate(Maximized, false), Idle);
        assert_eq!(watcher.evaluate(Maximized, false), Hide);
    }

    #[test]
    fn a_window_flicking_past_never_reaches_the_threshold() {
        // 最大化・非最大化が交互に来る間は、何度巡回しても動かない。
        let mut watcher = TaskbarWatcher::default();
        for _ in 0..10 {
            assert_eq!(watcher.evaluate(Maximized, false), Idle);
            assert_eq!(watcher.evaluate(NotMaximized, false), Idle);
        }
    }

    #[test]
    fn leaving_a_maximised_window_brings_the_taskbar_back() {
        let mut watcher = TaskbarWatcher::default();
        watcher.evaluate(Maximized, false);
        assert_eq!(watcher.evaluate(Maximized, false), Hide);
        assert_eq!(watcher.evaluate(NotMaximized, true), Idle);
        assert_eq!(watcher.evaluate(NotMaximized, true), Show);
    }

    #[test]
    fn staying_maximised_does_not_write_again_every_cycle() {
        let mut watcher = TaskbarWatcher::default();
        watcher.evaluate(Maximized, false);
        assert_eq!(watcher.evaluate(Maximized, false), Hide);
        for _ in 0..5 {
            assert_eq!(watcher.evaluate(Maximized, true), Idle);
        }
    }

    #[test]
    fn an_unknown_foreground_does_not_advance_anything() {
        // 全画面ゲーム中に届いた観測で、隠す判断が進まないこと。
        let mut watcher = TaskbarWatcher::default();
        assert_eq!(watcher.evaluate(Maximized, false), Idle);
        assert_eq!(watcher.evaluate(Unknown, false), Idle);
        // Unknown を挟んでも連続は途切れない（前面が読めないだけで状況は続いている）。
        assert_eq!(watcher.evaluate(Maximized, false), Hide);
    }

    #[test]
    fn a_hand_change_makes_it_let_go_and_stay_let_go() {
        let mut watcher = TaskbarWatcher::default();
        watcher.evaluate(Maximized, false);
        assert_eq!(watcher.evaluate(Maximized, false), Hide);
        // 利用者が設定画面で自動的に隠すを解いた。
        assert_eq!(watcher.evaluate(Maximized, false), ReleaseOwnership);
        assert!(watcher.has_released());
        // 以後どう転んでも自分からは書かない。
        for observation in [Maximized, NotMaximized, Unknown] {
            for auto_hide in [true, false] {
                assert_eq!(watcher.evaluate(observation, auto_hide), Idle);
            }
        }
    }

    #[test]
    fn it_never_takes_the_setting_back_after_letting_go() {
        let mut watcher = TaskbarWatcher::default();
        watcher.evaluate(Maximized, false);
        watcher.evaluate(Maximized, false);
        watcher.evaluate(Maximized, false); // ReleaseOwnership
                                            // 元の状態へ戻ったように見えても、取り返さない。
        assert_eq!(watcher.evaluate(Maximized, true), Idle);
    }

    #[test]
    fn a_user_who_already_hides_the_taskbar_is_left_alone() {
        // ここが以前は壊れていた。元から常に隠す設定の人に対して、
        // この機能が `Show` を書いて設定を勝手に解除し、二度と戻さなかった。
        // 以前のテストはその壊れた挙動のほうを固定していた。
        let mut watcher = TaskbarWatcher::default();
        for _ in 0..10 {
            assert_eq!(watcher.evaluate(NotMaximized, true), NotApplicable);
            assert_eq!(watcher.evaluate(Maximized, true), NotApplicable);
        }
        assert!(watcher.is_not_applicable());
        assert!(!watcher.owns_hidden(), "何も所有していない");
    }

    #[test]
    fn nothing_is_written_before_anything_has_been_hidden() {
        // 隠していないのに「出す」を書く理由が無い。
        // 触っていない設定を触ったことにしない。
        let mut watcher = TaskbarWatcher::default();
        for _ in 0..6 {
            assert_eq!(watcher.evaluate(NotMaximized, false), Idle);
        }
        assert!(!watcher.has_released());
        assert_eq!(watcher.baseline(), Some(false));
    }

    #[test]
    fn the_baseline_is_taken_once_and_not_re_taken() {
        let mut watcher = TaskbarWatcher::default();
        watcher.evaluate(NotMaximized, false);
        assert_eq!(watcher.baseline(), Some(false));
        // 途中で外の値が変わっても、戻す先は最初に見た値のまま。
        watcher.evaluate(Maximized, false);
        assert_eq!(watcher.evaluate(Maximized, false), Hide);
        assert_eq!(watcher.baseline(), Some(false));
    }

    #[test]
    fn ownership_is_reported_for_the_shutdown_path() {
        let mut watcher = TaskbarWatcher::default();
        assert!(!watcher.owns_hidden());
        watcher.evaluate(Maximized, false);
        watcher.evaluate(Maximized, false);
        assert!(watcher.owns_hidden(), "終了時に戻す対象だと分かること");
    }

    #[test]
    fn crash_recovery_marker_survives_reopen_and_is_only_cleared_explicitly() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let path = temporary.path().join("taskbar-auto-hide.json");
        let store = TaskbarAutoHideStore::open(path.clone()).expect("open");
        store
            .set(TaskbarAutoHideSetting {
                enabled: true,
                hiding_restore_to: None,
            })
            .expect("persist enabled setting");
        store.record_hiding(false).expect("persist recovery marker");
        assert_eq!(
            TaskbarAutoHideStore::open(path.clone())
                .expect("reopen with marker")
                .get()
                .hiding_restore_to,
            Some(false)
        );
        store.clear_hiding().expect("clear after verified restore");
        assert_eq!(
            TaskbarAutoHideStore::open(path)
                .expect("reopen without marker")
                .get()
                .hiding_restore_to,
            None
        );
    }
}
