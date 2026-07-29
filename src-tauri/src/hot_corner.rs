//! 安全なホットコーナー。
//!
//! 角への割り当て・滞在時間・クールダウンはアプリ内の data-only JSON に保存する。
//! 角へ着いたこと自体では Windows 設定も Action も変更しない。発火時に行うのは
//! PCカスタムの画面を前へ出し、モード画面を開くイベントを送ることだけ。

use std::path::PathBuf;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};

const SETTING_FILE_VERSION: u32 = 1;
const MAX_SETTING_FILE_BYTES: u64 = 8 * 1024;
pub const DEFAULT_DWELL_MS: u64 = 1_500;
pub const DEFAULT_COOLDOWN_MS: u64 = 15_000;
pub const MIN_DWELL_MS: u64 = 600;
pub const MAX_DWELL_MS: u64 = 10_000;
pub const MIN_COOLDOWN_MS: u64 = 3_000;
pub const MAX_COOLDOWN_MS: u64 = 60_000;
const HOT_ZONE_RADIUS: i32 = 1;
pub const HOT_CORNER_EVENT: &str = "hot-corner-open-modes";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotCornerAction {
    None,
    OpenModes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ScreenRect {
    pub const fn is_valid(self) -> bool {
        self.right > self.left && self.bottom > self.top
    }

    const fn contains(self, point: ScreenPoint) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }

    pub const fn corner_point(self, corner: HotCorner) -> ScreenPoint {
        match corner {
            HotCorner::TopLeft => ScreenPoint {
                x: self.left,
                y: self.top,
            },
            HotCorner::TopRight => ScreenPoint {
                x: self.right - 1,
                y: self.top,
            },
            HotCorner::BottomLeft => ScreenPoint {
                x: self.left,
                y: self.bottom - 1,
            },
            HotCorner::BottomRight => ScreenPoint {
                x: self.right - 1,
                y: self.bottom - 1,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HotCornerSetting {
    pub top_left: HotCornerAction,
    pub top_right: HotCornerAction,
    pub bottom_left: HotCornerAction,
    pub bottom_right: HotCornerAction,
    pub dwell_ms: u64,
    pub cooldown_ms: u64,
}

impl Default for HotCornerSetting {
    fn default() -> Self {
        Self {
            top_left: HotCornerAction::None,
            top_right: HotCornerAction::None,
            bottom_left: HotCornerAction::None,
            bottom_right: HotCornerAction::None,
            dwell_ms: DEFAULT_DWELL_MS,
            cooldown_ms: DEFAULT_COOLDOWN_MS,
        }
    }
}

impl HotCornerSetting {
    pub fn validate(self) -> CoreResult<()> {
        if !(MIN_DWELL_MS..=MAX_DWELL_MS).contains(&self.dwell_ms) {
            return Err(CoreError::invalid_request(
                "角の滞在時間は0.6秒〜10秒の範囲で指定してください。",
            ));
        }
        if !(MIN_COOLDOWN_MS..=MAX_COOLDOWN_MS).contains(&self.cooldown_ms) {
            return Err(CoreError::invalid_request(
                "角のクールダウンは3秒〜60秒の範囲で指定してください。",
            ));
        }
        Ok(())
    }

    pub const fn action(self, corner: HotCorner) -> HotCornerAction {
        match corner {
            HotCorner::TopLeft => self.top_left,
            HotCorner::TopRight => self.top_right,
            HotCorner::BottomLeft => self.bottom_left,
            HotCorner::BottomRight => self.bottom_right,
        }
    }

    pub const fn is_disabled(self) -> bool {
        matches!(self.top_left, HotCornerAction::None)
            && matches!(self.top_right, HotCornerAction::None)
            && matches!(self.bottom_left, HotCornerAction::None)
            && matches!(self.bottom_right, HotCornerAction::None)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SettingFile {
    version: u32,
    setting: HotCornerSetting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotCornerState {
    pub setting: HotCornerSetting,
    pub last_error: Option<String>,
}

#[derive(Debug)]
pub struct HotCornerStore {
    path: PathBuf,
    setting: Mutex<HotCornerSetting>,
    last_error: Mutex<Option<String>>,
}

impl HotCornerStore {
    pub fn open(path: PathBuf) -> CoreResult<Self> {
        let setting = match std::fs::metadata(&path) {
            Ok(metadata) if metadata.len() > MAX_SETTING_FILE_BYTES => HotCornerSetting::default(),
            Ok(_) => {
                let bytes = std::fs::read(&path).map_err(|_| CoreError::storage())?;
                match serde_json::from_slice::<SettingFile>(&bytes) {
                    Ok(parsed)
                        if parsed.version == SETTING_FILE_VERSION
                            && parsed.setting.validate().is_ok() =>
                    {
                        parsed.setting
                    }
                    // 読めない設定で誤発火させない。必ず「全角何もしない」へ倒す。
                    _ => HotCornerSetting::default(),
                }
            }
            Err(_) => HotCornerSetting::default(),
        };
        Ok(Self {
            path,
            setting: Mutex::new(setting),
            last_error: Mutex::new(None),
        })
    }

    pub fn get(&self) -> HotCornerSetting {
        *self.setting.lock()
    }

    pub fn state(&self) -> HotCornerState {
        HotCornerState {
            setting: *self.setting.lock(),
            last_error: self.last_error.lock().clone(),
        }
    }

    pub fn set(&self, setting: HotCornerSetting) -> CoreResult<HotCornerSetting> {
        setting.validate()?;
        let bytes = serde_json::to_vec_pretty(&SettingFile {
            version: SETTING_FILE_VERSION,
            setting,
        })
        .map_err(|_| CoreError::storage())?;
        crate::settings_file::replace(&self.path, &bytes)?;
        *self.setting.lock() = setting;
        *self.last_error.lock() = None;
        Ok(setting)
    }

    pub fn record_error(&self, error: Option<String>) {
        *self.last_error.lock() = error;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotCornerSample {
    pub now_ms: u64,
    pub position: ScreenPoint,
    pub corner: Option<HotCorner>,
    pub fullscreen: bool,
    pub maximized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotCornerDecision {
    Idle,
    OpenModes,
}

/// I/Oを持たない誤発火防止の状態機械。
#[derive(Debug, Default)]
pub struct HotCornerTracker {
    last_position: Option<ScreenPoint>,
    dwell_corner: Option<HotCorner>,
    stationary_since_ms: Option<u64>,
    cooldown_until_ms: Option<u64>,
    fired_corner: Option<HotCorner>,
}

impl HotCornerTracker {
    pub fn evaluate(
        &mut self,
        setting: HotCornerSetting,
        sample: HotCornerSample,
    ) -> HotCornerDecision {
        if sample.corner.is_none() {
            self.reset_dwell();
            self.fired_corner = None;
            self.last_position = Some(sample.position);
            return HotCornerDecision::Idle;
        }

        let corner = sample.corner.expect("checked above");
        if setting.is_disabled()
            || matches!(setting.action(corner), HotCornerAction::None)
            || sample.fullscreen
            || sample.maximized
        {
            // 最大化中も抑制する。ウィンドウを前へ出せば利用者の作業を覆うため、
            // 全画面と同じく「迷ったら出さない」を優先する。
            self.reset_dwell();
            self.last_position = Some(sample.position);
            return HotCornerDecision::Idle;
        }

        if self.fired_corner == Some(corner) {
            self.last_position = Some(sample.position);
            return HotCornerDecision::Idle;
        }

        let moved = self.last_position != Some(sample.position);
        let switched_corner = self.dwell_corner != Some(corner);
        self.last_position = Some(sample.position);
        if moved || switched_corner || self.stationary_since_ms.is_none() {
            self.dwell_corner = Some(corner);
            self.stationary_since_ms = Some(sample.now_ms);
            return HotCornerDecision::Idle;
        }

        let stationary_since = self.stationary_since_ms.unwrap_or(sample.now_ms);
        if sample.now_ms.saturating_sub(stationary_since) < setting.dwell_ms {
            return HotCornerDecision::Idle;
        }
        if self
            .cooldown_until_ms
            .is_some_and(|until| sample.now_ms < until)
        {
            return HotCornerDecision::Idle;
        }

        self.cooldown_until_ms = Some(sample.now_ms.saturating_add(setting.cooldown_ms));
        self.fired_corner = Some(corner);
        HotCornerDecision::OpenModes
    }

    fn reset_dwell(&mut self) {
        self.dwell_corner = None;
        self.stationary_since_ms = None;
    }
}

/// 現在座標が、現在モニターの外周角にあるときだけ角を返す。
pub fn external_corner_at(
    position: ScreenPoint,
    monitor: ScreenRect,
    monitors: &[ScreenRect],
) -> Option<HotCorner> {
    if !monitor.is_valid() || !monitor.contains(position) {
        return None;
    }
    [
        HotCorner::TopLeft,
        HotCorner::TopRight,
        HotCorner::BottomLeft,
        HotCorner::BottomRight,
    ]
    .into_iter()
    .find(|corner| {
        near(position, monitor.corner_point(*corner))
            && corner_is_on_desktop_outer_edge(monitor, monitors, *corner)
    })
}

pub fn external_corner_points(
    monitor: ScreenRect,
    monitors: &[ScreenRect],
) -> Vec<(HotCorner, ScreenPoint)> {
    [
        HotCorner::TopLeft,
        HotCorner::TopRight,
        HotCorner::BottomLeft,
        HotCorner::BottomRight,
    ]
    .into_iter()
    .filter(|corner| corner_is_on_desktop_outer_edge(monitor, monitors, *corner))
    .map(|corner| (corner, monitor.corner_point(corner)))
    .collect()
}

fn near(left: ScreenPoint, right: ScreenPoint) -> bool {
    left.x.abs_diff(right.x) <= HOT_ZONE_RADIUS as u32
        && left.y.abs_diff(right.y) <= HOT_ZONE_RADIUS as u32
}

fn corner_is_on_desktop_outer_edge(
    monitor: ScreenRect,
    monitors: &[ScreenRect],
    corner: HotCorner,
) -> bool {
    let point = monitor.corner_point(corner);
    let (horizontal, vertical, diagonal) = match corner {
        HotCorner::TopLeft => (
            ScreenPoint {
                x: monitor.left - 1,
                y: point.y,
            },
            ScreenPoint {
                x: point.x,
                y: monitor.top - 1,
            },
            ScreenPoint {
                x: monitor.left - 1,
                y: monitor.top - 1,
            },
        ),
        HotCorner::TopRight => (
            ScreenPoint {
                x: monitor.right,
                y: point.y,
            },
            ScreenPoint {
                x: point.x,
                y: monitor.top - 1,
            },
            ScreenPoint {
                x: monitor.right,
                y: monitor.top - 1,
            },
        ),
        HotCorner::BottomLeft => (
            ScreenPoint {
                x: monitor.left - 1,
                y: point.y,
            },
            ScreenPoint {
                x: point.x,
                y: monitor.bottom,
            },
            ScreenPoint {
                x: monitor.left - 1,
                y: monitor.bottom,
            },
        ),
        HotCorner::BottomRight => (
            ScreenPoint {
                x: monitor.right,
                y: point.y,
            },
            ScreenPoint {
                x: point.x,
                y: monitor.bottom,
            },
            ScreenPoint {
                x: monitor.right,
                y: monitor.bottom,
            },
        ),
    };
    !monitors.iter().copied().any(|other| {
        other != monitor
            && (other.contains(horizontal) || other.contains(vertical) || other.contains(diagonal))
    })
}

type PresentCallback = dyn Fn() -> CoreResult<()> + Send + Sync + 'static;

#[derive(Default)]
pub struct HotCornerPresenter {
    callback: Mutex<Option<Box<PresentCallback>>>,
}

impl std::fmt::Debug for HotCornerPresenter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HotCornerPresenter")
            .field("registered", &self.callback.lock().is_some())
            .finish()
    }
}

impl HotCornerPresenter {
    pub fn register(&self, callback: impl Fn() -> CoreResult<()> + Send + Sync + 'static) {
        *self.callback.lock() = Some(Box::new(callback));
    }

    pub fn open_modes(&self) -> CoreResult<()> {
        let callback = self.callback.lock();
        let callback = callback.as_ref().ok_or_else(|| {
            CoreError::new(
                "HOT_CORNER_WINDOW_UNAVAILABLE",
                "HOT_CORNER",
                true,
                "PCカスタムの画面をまだ開けません。角から離れて再試行してください。",
            )
        })?;
        callback()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> HotCornerSetting {
        HotCornerSetting {
            top_left: HotCornerAction::OpenModes,
            ..HotCornerSetting::default()
        }
    }

    fn sample(now_ms: u64, position: ScreenPoint, corner: Option<HotCorner>) -> HotCornerSample {
        HotCornerSample {
            now_ms,
            position,
            corner,
            fullscreen: false,
            maximized: false,
        }
    }

    #[test]
    fn dwell_time_must_elapse_before_firing() {
        let mut tracker = HotCornerTracker::default();
        let position = ScreenPoint { x: 0, y: 0 };
        assert_eq!(
            tracker.evaluate(enabled(), sample(0, position, Some(HotCorner::TopLeft))),
            HotCornerDecision::Idle
        );
        assert_eq!(
            tracker.evaluate(
                enabled(),
                sample(DEFAULT_DWELL_MS - 1, position, Some(HotCorner::TopLeft))
            ),
            HotCornerDecision::Idle
        );
        assert_eq!(
            tracker.evaluate(
                enabled(),
                sample(DEFAULT_DWELL_MS, position, Some(HotCorner::TopLeft))
            ),
            HotCornerDecision::OpenModes
        );
    }

    #[test]
    fn moving_through_the_corner_never_accumulates_dwell() {
        let mut tracker = HotCornerTracker::default();
        for tick in 0..20 {
            let position = ScreenPoint {
                x: tick % 2,
                y: (tick / 2) % 2,
            };
            assert_eq!(
                tracker.evaluate(
                    enabled(),
                    sample(
                        tick as u64 * DEFAULT_DWELL_MS,
                        position,
                        Some(HotCorner::TopLeft)
                    )
                ),
                HotCornerDecision::Idle
            );
        }
    }

    #[test]
    fn cooldown_blocks_a_second_activation() {
        let mut tracker = HotCornerTracker::default();
        let corner = ScreenPoint { x: 0, y: 0 };
        let away = ScreenPoint { x: 50, y: 50 };
        tracker.evaluate(enabled(), sample(0, corner, Some(HotCorner::TopLeft)));
        assert_eq!(
            tracker.evaluate(
                enabled(),
                sample(DEFAULT_DWELL_MS, corner, Some(HotCorner::TopLeft))
            ),
            HotCornerDecision::OpenModes
        );
        tracker.evaluate(enabled(), sample(DEFAULT_DWELL_MS + 1, away, None));
        tracker.evaluate(
            enabled(),
            sample(DEFAULT_DWELL_MS + 2, corner, Some(HotCorner::TopLeft)),
        );
        assert_eq!(
            tracker.evaluate(
                enabled(),
                sample(DEFAULT_DWELL_MS * 2 + 2, corner, Some(HotCorner::TopLeft))
            ),
            HotCornerDecision::Idle
        );
    }

    #[test]
    fn fullscreen_and_maximized_windows_suppress_activation() {
        for (fullscreen, maximized) in [(true, false), (false, true), (true, true)] {
            let mut tracker = HotCornerTracker::default();
            let position = ScreenPoint { x: 0, y: 0 };
            let mut current = sample(0, position, Some(HotCorner::TopLeft));
            current.fullscreen = fullscreen;
            current.maximized = maximized;
            tracker.evaluate(enabled(), current);
            current.now_ms = DEFAULT_DWELL_MS * 2;
            assert_eq!(
                tracker.evaluate(enabled(), current),
                HotCornerDecision::Idle
            );
        }
    }

    #[test]
    fn corners_on_a_monitor_seam_are_not_external() {
        let left = ScreenRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let right = ScreenRect {
            left: 1920,
            top: 0,
            right: 3840,
            bottom: 1080,
        };
        let monitors = [left, right];
        assert_eq!(
            external_corner_at(left.corner_point(HotCorner::TopRight), left, &monitors),
            None
        );
        assert_eq!(
            external_corner_at(left.corner_point(HotCorner::BottomRight), left, &monitors),
            None
        );
        assert_eq!(
            external_corner_at(left.corner_point(HotCorner::TopLeft), left, &monitors),
            Some(HotCorner::TopLeft)
        );
    }

    #[test]
    fn default_setting_never_fires_any_corner() {
        let mut tracker = HotCornerTracker::default();
        let setting = HotCornerSetting::default();
        for (index, corner) in [
            HotCorner::TopLeft,
            HotCorner::TopRight,
            HotCorner::BottomLeft,
            HotCorner::BottomRight,
        ]
        .into_iter()
        .enumerate()
        {
            let position = ScreenPoint {
                x: index as i32,
                y: index as i32,
            };
            tracker.evaluate(
                setting,
                sample(index as u64 * 10_000, position, Some(corner)),
            );
            assert_eq!(
                tracker.evaluate(
                    setting,
                    sample(index as u64 * 10_000 + 9_000, position, Some(corner))
                ),
                HotCornerDecision::Idle
            );
        }
    }

    #[test]
    fn store_defaults_to_all_corners_disabled_and_round_trips() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let path = temporary.path().join("hot-corners.json");
        let store = HotCornerStore::open(path.clone()).expect("open");
        assert!(store.get().is_disabled());
        let setting = enabled();
        store.set(setting).expect("set");
        assert_eq!(HotCornerStore::open(path).expect("reopen").get(), setting);
    }

    #[test]
    fn store_can_replace_an_existing_setting_file() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let path = temporary.path().join("hot-corners.json");
        let store = HotCornerStore::open(path.clone()).expect("open");
        store.set(enabled()).expect("first set");
        let replacement = HotCornerSetting {
            top_left: HotCornerAction::None,
            top_right: HotCornerAction::OpenModes,
            ..enabled()
        };
        store.set(replacement).expect("replace existing setting");
        assert_eq!(
            HotCornerStore::open(path).expect("reopen").get(),
            replacement
        );
    }
}
