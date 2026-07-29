//! 時間帯によるライト/ダーク自動切り替え（BRIEF §1「時間帯によるテーマ切り替え」）。
//!
//! 方針:
//! - 判定は純関数。時計値(分)を受け取り、望ましいモードを返すだけでI/Oしない。
//! - 適用は検証済みの `theme.color_mode` Action を、通常の preview→commit 経路で通す。
//!   よって変更はタイムラインに1件ずつ残り、個別に元へ戻せる。
//! - **境界をまたいだ時だけ**適用する。利用者が途中で手動変更した場合に張り合わない。
//! - 設定は data-only JSON。任意コードやコマンドは持たない。

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};

const THEME_SCHEDULE_FILE_VERSION: u32 = 1;
const MAX_THEME_SCHEDULE_FILE_BYTES: u64 = 8 * 1024;
pub const MINUTES_PER_DAY: u32 = 24 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledMode {
    Light,
    Dark,
}

impl ScheduledMode {
    /// `theme.color_mode` の parameters へ渡す値。
    pub fn as_parameter(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeSchedule {
    pub enabled: bool,
    /// 0..MINUTES_PER_DAY。ライトへ切り替える時刻（分）。
    pub light_at_minutes: u32,
    /// 0..MINUTES_PER_DAY。ダークへ切り替える時刻（分）。
    pub dark_at_minutes: u32,
}

impl Default for ThemeSchedule {
    fn default() -> Self {
        Self {
            enabled: false,
            light_at_minutes: 7 * 60,
            dark_at_minutes: 19 * 60,
        }
    }
}

impl ThemeSchedule {
    pub fn validate(&self) -> CoreResult<()> {
        if self.light_at_minutes >= MINUTES_PER_DAY || self.dark_at_minutes >= MINUTES_PER_DAY {
            return Err(CoreError::invalid_request(
                "時刻は0:00〜23:59の範囲で指定してください。",
            ));
        }
        if self.light_at_minutes == self.dark_at_minutes {
            return Err(CoreError::invalid_request(
                "明るくする時刻と暗くする時刻を別々に指定してください。",
            ));
        }
        Ok(())
    }

    /// 指定時刻に望ましいモード。無効なら None（＝自動切り替えしない）。
    pub fn desired_mode(&self, now_minutes: u32) -> Option<ScheduledMode> {
        if !self.enabled || self.validate().is_err() {
            return None;
        }
        let now = now_minutes % MINUTES_PER_DAY;
        let light = self.light_at_minutes;
        let dark = self.dark_at_minutes;
        let is_light = if light < dark {
            // 例: 07:00 明るく → 19:00 暗く（日中がライト）
            now >= light && now < dark
        } else {
            // 日をまたぐ。例: 20:00 明るく → 06:00 暗く
            now >= light || now < dark
        };
        Some(if is_light {
            ScheduledMode::Light
        } else {
            ScheduledMode::Dark
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeScheduleFile {
    version: u32,
    schedule: ThemeSchedule,
}

/// UIへ返す状態。自動適用の失敗を握り潰さず、直近の失敗理由を持ち回る。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeScheduleState {
    pub schedule: ThemeSchedule,
    pub last_error: Option<String>,
}

#[derive(Debug)]
pub struct ThemeScheduleStore {
    path: PathBuf,
    schedule: Mutex<ThemeSchedule>,
    last_error: Mutex<Option<String>>,
}

impl ThemeScheduleStore {
    pub fn open(path: PathBuf) -> CoreResult<Self> {
        let schedule = match std::fs::metadata(&path) {
            Ok(metadata) if metadata.len() > MAX_THEME_SCHEDULE_FILE_BYTES => {
                return Err(CoreError::invalid_request(
                    "テーマ切り替え設定ファイルが上限を超えています。",
                ));
            }
            Ok(_) => {
                let bytes = std::fs::read(&path).map_err(|_| CoreError::storage())?;
                let parsed: ThemeScheduleFile = serde_json::from_slice(&bytes)
                    .map_err(|_| CoreError::invalid_request("テーマ切り替え設定を読めません。"))?;
                if parsed.version != THEME_SCHEDULE_FILE_VERSION {
                    return Err(CoreError::invalid_request(
                        "テーマ切り替え設定の版が対応外です。",
                    ));
                }
                // 外部編集された可能性があるため、読み込み時も同じ検証を通す。
                if parsed.schedule.validate().is_err() {
                    ThemeSchedule::default()
                } else {
                    parsed.schedule
                }
            }
            Err(_) => ThemeSchedule::default(),
        };
        Ok(Self {
            path,
            schedule: Mutex::new(schedule),
            last_error: Mutex::new(None),
        })
    }

    pub fn get(&self) -> ThemeSchedule {
        *self.schedule.lock()
    }

    pub fn state(&self) -> ThemeScheduleState {
        ThemeScheduleState {
            schedule: *self.schedule.lock(),
            last_error: self.last_error.lock().clone(),
        }
    }

    /// 背景適用の結果を記録する。成功時は直近の失敗表示を消す。
    pub fn record_apply_result(&self, error: Option<String>) {
        *self.last_error.lock() = error;
    }

    pub fn set(&self, schedule: ThemeSchedule) -> CoreResult<ThemeSchedule> {
        schedule.validate()?;
        Self::persist(&self.path, &schedule)?;
        *self.schedule.lock() = schedule;
        *self.last_error.lock() = None;
        Ok(schedule)
    }

    fn persist(path: &Path, schedule: &ThemeSchedule) -> CoreResult<()> {
        let file = ThemeScheduleFile {
            version: THEME_SCHEDULE_FILE_VERSION,
            schedule: *schedule,
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|_| CoreError::storage())?;
        crate::settings_file::replace(path, &bytes)
    }
}

/// 直前の判定を覚えておき、**境界をまたいだ時だけ** Some(mode) を返す。
/// 手動でテーマを変えた利用者と張り合わないための状態機械。
#[derive(Debug, Default)]
pub struct ThemeScheduleTracker {
    last: Option<ScheduledMode>,
}

impl ThemeScheduleTracker {
    pub fn evaluate(
        &mut self,
        schedule: &ThemeSchedule,
        now_minutes: u32,
    ) -> Option<ScheduledMode> {
        let desired = schedule.desired_mode(now_minutes);
        match desired {
            None => {
                // 無効化されたら次回有効化時に必ず1回適用されるよう忘れる。
                self.last = None;
                None
            }
            Some(mode) if self.last == Some(mode) => None,
            Some(mode) => {
                self.last = Some(mode);
                Some(mode)
            }
        }
    }
}

pub type SharedThemeScheduleStore = Arc<ThemeScheduleStore>;

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule(light: u32, dark: u32) -> ThemeSchedule {
        ThemeSchedule {
            enabled: true,
            light_at_minutes: light,
            dark_at_minutes: dark,
        }
    }

    #[test]
    fn daytime_light_schedule_resolves_each_boundary() {
        let s = schedule(7 * 60, 19 * 60);
        assert_eq!(s.desired_mode(6 * 60 + 59), Some(ScheduledMode::Dark));
        assert_eq!(
            s.desired_mode(7 * 60),
            Some(ScheduledMode::Light),
            "開始時刻は含む"
        );
        assert_eq!(s.desired_mode(12 * 60), Some(ScheduledMode::Light));
        assert_eq!(
            s.desired_mode(19 * 60),
            Some(ScheduledMode::Dark),
            "終了時刻は含まない"
        );
        assert_eq!(s.desired_mode(23 * 60), Some(ScheduledMode::Dark));
    }

    #[test]
    fn wrapping_schedule_crosses_midnight() {
        // 20:00 明るく → 06:00 暗く（夜間がライト）
        let s = schedule(20 * 60, 6 * 60);
        assert_eq!(s.desired_mode(21 * 60), Some(ScheduledMode::Light));
        assert_eq!(
            s.desired_mode(0),
            Some(ScheduledMode::Light),
            "日をまたいでも継続"
        );
        assert_eq!(s.desired_mode(5 * 60 + 59), Some(ScheduledMode::Light));
        assert_eq!(s.desired_mode(6 * 60), Some(ScheduledMode::Dark));
        assert_eq!(s.desired_mode(19 * 60), Some(ScheduledMode::Dark));
    }

    #[test]
    fn disabled_or_invalid_schedule_never_requests_a_mode() {
        let mut s = schedule(7 * 60, 19 * 60);
        s.enabled = false;
        assert_eq!(s.desired_mode(12 * 60), None);

        let same = schedule(9 * 60, 9 * 60);
        assert!(same.validate().is_err());
        assert_eq!(
            same.desired_mode(9 * 60),
            None,
            "同一時刻は自動切り替えしない"
        );

        let out_of_range = schedule(MINUTES_PER_DAY, 60);
        assert!(out_of_range.validate().is_err());
        assert_eq!(out_of_range.desired_mode(60), None);
    }

    #[test]
    fn tracker_applies_only_when_crossing_a_boundary() {
        let s = schedule(7 * 60, 19 * 60);
        let mut tracker = ThemeScheduleTracker::default();
        // 初回は現在の時間帯に合わせて1回適用する。
        assert_eq!(tracker.evaluate(&s, 12 * 60), Some(ScheduledMode::Light));
        // 同じ時間帯の間は繰り返し適用しない（手動変更と張り合わない）。
        assert_eq!(tracker.evaluate(&s, 13 * 60), None);
        assert_eq!(tracker.evaluate(&s, 18 * 60 + 59), None);
        // 境界をまたいだら1回だけ適用する。
        assert_eq!(tracker.evaluate(&s, 19 * 60), Some(ScheduledMode::Dark));
        assert_eq!(tracker.evaluate(&s, 22 * 60), None);
        // 翌朝また1回。
        assert_eq!(tracker.evaluate(&s, 7 * 60), Some(ScheduledMode::Light));
    }

    #[test]
    fn tracker_reapplies_after_being_disabled() {
        let s = schedule(7 * 60, 19 * 60);
        let mut disabled = s;
        disabled.enabled = false;
        let mut tracker = ThemeScheduleTracker::default();
        assert_eq!(tracker.evaluate(&s, 12 * 60), Some(ScheduledMode::Light));
        assert_eq!(tracker.evaluate(&disabled, 13 * 60), None);
        // 再度有効化したら、同じ時間帯でももう一度合わせる。
        assert_eq!(tracker.evaluate(&s, 14 * 60), Some(ScheduledMode::Light));
    }

    #[test]
    fn store_round_trips_and_rejects_invalid_input() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("theme-schedule.json");
        let store = ThemeScheduleStore::open(path.clone()).expect("open new");
        assert!(!store.get().enabled, "既定は自動切り替えオフ");

        let updated = store.set(schedule(8 * 60, 20 * 60)).expect("set");
        assert!(updated.enabled);

        let reopened = ThemeScheduleStore::open(path.clone()).expect("reopen");
        assert_eq!(reopened.get(), updated, "ディスクから復元できる");

        let replaced = store
            .set(schedule(9 * 60, 21 * 60))
            .expect("replace existing setting");
        assert_eq!(
            ThemeScheduleStore::open(path.clone())
                .expect("reopen replacement")
                .get(),
            replaced,
            "既存ファイルも置換して保存できる"
        );

        assert!(
            store.set(schedule(9 * 60, 9 * 60)).is_err(),
            "同一時刻は拒否"
        );
        assert!(
            store.set(schedule(MINUTES_PER_DAY + 1, 60)).is_err(),
            "範囲外は拒否"
        );
        assert_eq!(
            ThemeScheduleStore::open(path)
                .expect("reopen after rejects")
                .get(),
            replaced,
            "拒否された値は保存されない"
        );
    }
}
