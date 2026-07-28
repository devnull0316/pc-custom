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
    pub last_error: Option<String>,
}

#[derive(Debug)]
pub struct TaskbarAutoHideStore {
    path: PathBuf,
    setting: Mutex<TaskbarAutoHideSetting>,
    released: Mutex<bool>,
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
            last_error: self.last_error.lock().clone(),
        }
    }

    pub fn record_released(&self) {
        *self.released.lock() = true;
    }

    pub fn record_error(&self, error: Option<String>) {
        *self.last_error.lock() = error;
    }

    pub fn set(&self, setting: TaskbarAutoHideSetting) -> CoreResult<TaskbarAutoHideSetting> {
        let file = SettingFile {
            version: SETTING_FILE_VERSION,
            setting,
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|_| CoreError::storage())?;
        let mut temp = self.path.clone();
        temp.set_extension("json.tmp");
        std::fs::write(&temp, &bytes).map_err(|_| CoreError::storage())?;
        std::fs::rename(&temp, &self.path).map_err(|_| CoreError::storage())?;
        *self.setting.lock() = setting;
        // 入れ直したら、手を引いた記録も消す。もう一度やらせるということ。
        *self.released.lock() = false;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use ForegroundObservation::{Maximized, NotMaximized, Unknown};
    use WatcherDecision::{Hide, Idle, ReleaseOwnership, Show};

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
    fn nothing_is_owned_before_the_first_write() {
        // まだ何も書いていない間は、外の値がどうであろうと手を引かない。
        let mut watcher = TaskbarWatcher::default();
        assert_eq!(watcher.evaluate(NotMaximized, true), Idle);
        assert!(!watcher.has_released());
        assert_eq!(watcher.evaluate(NotMaximized, true), Show);
    }

    #[test]
    fn ownership_is_reported_for_the_shutdown_path() {
        let mut watcher = TaskbarWatcher::default();
        assert!(!watcher.owns_hidden());
        watcher.evaluate(Maximized, false);
        watcher.evaluate(Maximized, false);
        assert!(watcher.owns_hidden(), "終了時に戻す対象だと分かること");
    }
}
