//! オーバーレイを「タスクバーの上」に置き続けるための土台。
//!
//! 実測で分かっていること:
//! - 自前のウィンドウはタスクバーより手前へ置ける
//! - ただし後から出た別の最前面ウィンドウに前を取られる。取り返しは必要
//! - シェルを再起動するとタスクバーの HWND ごと変わる
//!
//! そのため「どこに置くか」を毎回 Windows へ聞き直せる必要がある。
//! ここは**位置と状況を読むだけ**で、ウィンドウを作ったり動かしたりはしない。
//!
//! 安全契約: 使うのは `SHAppBarMessage(ABM_GETTASKBARPOS)`、`MonitorFromPoint`、
//! `GetMonitorInfoW`、`GetForegroundWindow`、`GetWindowRect` のみ。いずれも公開 API。

use super::{WindowsError, WindowsErrorKind, WindowsResult};
use crate::hot_corner::{ScreenPoint, ScreenRect};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorDesktopObservation {
    pub cursor: ScreenPoint,
    pub monitor: ScreenRect,
    pub monitors: Vec<ScreenRect>,
}

/// タスクバーの位置と、その画面での置かれ方。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskbarAnchor {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    /// 自動的に隠れる設定か。隠れる場合、常に同じ位置に描き続けると浮いて見える。
    pub auto_hide: bool,
    /// 画面のどの辺にあるか。上下左右で絵の向きが変わる。
    pub edge: TaskbarEdge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskbarEdge {
    Left,
    Top,
    Right,
    Bottom,
    Unknown,
}

impl TaskbarAnchor {
    pub const fn width(&self) -> i32 {
        self.right - self.left
    }
    pub const fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

#[cfg(windows)]
pub fn read_taskbar_anchor() -> WindowsResult<TaskbarAnchor> {
    use windows::Win32::UI::Shell::{
        SHAppBarMessage, ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETSTATE, ABM_GETTASKBARPOS,
        ABS_AUTOHIDE, APPBARDATA,
    };

    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..Default::default()
    };
    // 戻り値 0 は失敗。位置が分からないまま描くと画面のどこかに置き去りになる。
    if unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut data) } == 0 {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "SHAppBarMessage ABM_GETTASKBARPOS",
            None,
        ));
    }
    let edge = match data.uEdge {
        e if e == ABE_LEFT => TaskbarEdge::Left,
        e if e == ABE_TOP => TaskbarEdge::Top,
        e if e == ABE_RIGHT => TaskbarEdge::Right,
        e if e == ABE_BOTTOM => TaskbarEdge::Bottom,
        _ => TaskbarEdge::Unknown,
    };
    let mut state = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..Default::default()
    };
    let flags = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut state) };
    Ok(TaskbarAnchor {
        left: data.rc.left,
        top: data.rc.top,
        right: data.rc.right,
        bottom: data.rc.bottom,
        auto_hide: (flags as u32 & ABS_AUTOHIDE) != 0,
        edge,
    })
}

/// ウィンドウが画面を覆っているか。座標だけで決まるので、ここだけ切り出して検査する。
///
/// `>=` / `<=` にしているのは、枠なし全画面がわずかにはみ出すことがあるため。
/// 等しいときも覆っているとみなす。
pub(crate) const fn rect_covers_monitor(
    window: (i32, i32, i32, i32),
    monitor: (i32, i32, i32, i32),
) -> bool {
    window.0 <= monitor.0 && window.1 <= monitor.1 && window.2 >= monitor.2 && window.3 >= monitor.3
}

/// いま最前面のウィンドウが、その画面いっぱいに広がっているか。
///
/// 全画面のゲームや動画の上へオーバーレイを描いてはいけない。
/// 邪魔になるだけでなく、全画面表示から落ちる原因にもなる。
#[cfg(windows)]
pub fn foreground_is_fullscreen() -> WindowsResult<bool> {
    use windows::Win32::{
        Foundation::RECT,
        Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect},
    };

    let window = unsafe { GetForegroundWindow() };
    if window.is_invalid() {
        // 前面が取れないなら「全画面ではない」と決めつけない。分からないと返す方が安全だが、
        // ここは描画を止める側へ倒す（描かない方が害が小さい）。
        return Ok(true);
    }
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(window, &mut rect) }.is_err() {
        return Ok(true);
    }
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Ok(true);
    }
    Ok(rect_covers_monitor(
        (rect.left, rect.top, rect.right, rect.bottom),
        (
            info.rcMonitor.left,
            info.rcMonitor.top,
            info.rcMonitor.right,
            info.rcMonitor.bottom,
        ),
    ))
}

/// カーソル座標と、その座標を含むモニター、接続中モニターの矩形を読む。
///
/// 読み取り専用。フック、カーソル移動、ウィンドウ操作は行わない。
#[cfg(windows)]
pub fn read_cursor_desktop_observation() -> WindowsResult<CursorDesktopObservation> {
    use windows::Win32::{
        Foundation::POINT,
        Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONULL},
        UI::WindowsAndMessaging::GetCursorPos,
    };

    let mut cursor = POINT::default();
    unsafe { GetCursorPos(&mut cursor) }.map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "GetCursorPos for hot corner",
            None,
        )
    })?;
    let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONULL) };
    if monitor == Default::default() {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "MonitorFromPoint for hot corner",
            None,
        ));
    }
    let mut information = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut information) }.as_bool() {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "GetMonitorInfo for hot corner",
            None,
        ));
    }
    let monitor = screen_rect(information.rcMonitor)?;
    let monitors = connected_monitor_rects()?;
    Ok(CursorDesktopObservation {
        cursor: ScreenPoint {
            x: cursor.x,
            y: cursor.y,
        },
        monitor,
        monitors,
    })
}

#[cfg(windows)]
pub fn read_primary_monitor_rect() -> WindowsResult<ScreenRect> {
    use windows::Win32::{
        Foundation::POINT,
        Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTOPRIMARY},
    };

    let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
    if monitor == Default::default() {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "MonitorFromPoint for primary monitor",
            None,
        ));
    }
    let mut information = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut information) }.as_bool() {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "GetMonitorInfo for primary monitor",
            None,
        ));
    }
    screen_rect(information.rcMonitor)
}

#[cfg(windows)]
fn connected_monitor_rects() -> WindowsResult<Vec<ScreenRect>> {
    use windows::Win32::{
        Foundation::{BOOL, FALSE, LPARAM, TRUE},
        Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
    };

    const MAX_MONITORS: usize = 32;
    struct Enumeration {
        monitors: Vec<ScreenRect>,
        failed: bool,
        overflow: bool,
    }

    unsafe extern "system" fn callback(
        monitor: HMONITOR,
        _device_context: HDC,
        _monitor_rect: *mut windows::Win32::Foundation::RECT,
        parameter: LPARAM,
    ) -> BOOL {
        let enumeration = &mut *(parameter.0 as *mut Enumeration);
        if enumeration.monitors.len() == MAX_MONITORS {
            enumeration.overflow = true;
            return FALSE;
        }
        let mut information = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut information).as_bool() {
            enumeration.failed = true;
            return FALSE;
        }
        let rect = ScreenRect {
            left: information.rcMonitor.left,
            top: information.rcMonitor.top,
            right: information.rcMonitor.right,
            bottom: information.rcMonitor.bottom,
        };
        if !rect.is_valid() {
            enumeration.failed = true;
            return FALSE;
        }
        // ミラー表示は同じ矩形を返す。純関数側で別モニターの継ぎ目と誤認しないよう重複を除く。
        if !enumeration.monitors.contains(&rect) {
            enumeration.monitors.push(rect);
        }
        TRUE
    }

    let mut enumeration = Enumeration {
        monitors: Vec::new(),
        failed: false,
        overflow: false,
    };
    let completed = unsafe {
        EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(callback),
            LPARAM(&mut enumeration as *mut Enumeration as isize),
        )
    };
    if enumeration.overflow {
        return Err(WindowsError::new(
            WindowsErrorKind::ResourceLimit,
            "bound hot corner monitor enumeration",
            None,
        ));
    }
    if enumeration.failed || !completed.as_bool() || enumeration.monitors.is_empty() {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "enumerate hot corner monitors",
            None,
        ));
    }
    Ok(enumeration.monitors)
}

#[cfg(windows)]
fn screen_rect(rect: windows::Win32::Foundation::RECT) -> WindowsResult<ScreenRect> {
    let rect = ScreenRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    };
    if rect.is_valid() {
        Ok(rect)
    } else {
        Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate hot corner monitor rectangle",
            None,
        ))
    }
}

#[cfg(not(windows))]
pub fn read_taskbar_anchor() -> WindowsResult<TaskbarAnchor> {
    Err(WindowsError::new(
        WindowsErrorKind::UnsupportedPlatform,
        "read taskbar anchor",
        None,
    ))
}

#[cfg(not(windows))]
pub fn foreground_is_fullscreen() -> WindowsResult<bool> {
    Err(WindowsError::new(
        WindowsErrorKind::UnsupportedPlatform,
        "foreground fullscreen",
        None,
    ))
}

#[cfg(not(windows))]
pub fn read_cursor_desktop_observation() -> WindowsResult<CursorDesktopObservation> {
    Err(WindowsError::unsupported("read cursor desktop observation"))
}

#[cfg(not(windows))]
pub fn read_primary_monitor_rect() -> WindowsResult<ScreenRect> {
    Err(WindowsError::unsupported("read primary monitor rectangle"))
}

#[cfg(all(test, windows))]
mod tests {
    /// 実機のタスクバーの位置・辺・自動非表示と、前面が全画面かを読み出す。
    /// **何も変更しない。** 読み取りだけ。
    #[test]
    #[ignore = "実機の状態を読むだけ。変更しない"]
    fn dump_taskbar_anchor_and_fullscreen_state() {
        match super::read_taskbar_anchor() {
            Ok(anchor) => {
                println!(
                    "taskbar: ({}, {}) - ({}, {})  {}x{}  edge={:?} auto_hide={}",
                    anchor.left,
                    anchor.top,
                    anchor.right,
                    anchor.bottom,
                    anchor.width(),
                    anchor.height(),
                    anchor.edge,
                    anchor.auto_hide
                );
                assert!(anchor.width() > 0 && anchor.height() > 0, "面積を持つこと");
            }
            Err(error) => println!("タスクバー位置を読めなかった: {error:?}"),
        }
        match super::foreground_is_fullscreen() {
            Ok(value) => println!("前面が全画面: {value}"),
            Err(error) => println!("全画面判定に失敗: {error:?}"),
        }
    }

    /// 覆っているかの判定そのものを検査する。
    ///
    /// 以前ここには「コード内にコメントが残っているか」を見るだけのテストを書いていた。
    /// それは実装が壊れても通ってしまう。判定を純粋関数へ出して、値で確かめる。
    #[test]
    fn covering_the_monitor_is_decided_by_coordinates_alone() {
        let monitor = (0, 0, 1920, 1080);
        assert!(
            super::rect_covers_monitor(monitor, monitor),
            "画面と同じなら覆っている"
        );
        assert!(
            super::rect_covers_monitor((-8, -8, 1928, 1088), monitor),
            "はみ出していても覆っている"
        );
        assert!(
            !super::rect_covers_monitor((0, 0, 1920, 1000), monitor),
            "下が足りなければ覆っていない"
        );
        assert!(
            !super::rect_covers_monitor((100, 100, 800, 600), monitor),
            "普通の窓は覆っていない"
        );
        // タスクバーの分だけ短い「最大化」は全画面ではない。ここを取り違えると、
        // 最大化しただけの窓の上に描かなくなる。
        assert!(
            !super::rect_covers_monitor((0, 0, 1920, 1032), monitor),
            "最大化は全画面ではない"
        );
    }

    /// 読み取り専用の実機証拠。利用者のマウスは動かさない。
    #[test]
    #[ignore = "実機のカーソル・モニター配置と5HzポーリングのCPU時間を読む"]
    fn dump_hot_corner_cursor_primary_outer_corners_and_cpu() {
        let observation = super::read_cursor_desktop_observation().expect("GetCursorPos");
        let primary = super::read_primary_monitor_rect().expect("primary monitor");
        let outer = crate::hot_corner::external_corner_points(primary, &observation.monitors);
        // 名前のとおり CPU 時間も測る。**常駐するものは、費用を数字で出す。**
        // 25回のポーリングの前後で、このプロセスが使ったカーネル時間とユーザー時間の差を取る。
        fn process_cpu_100ns() -> super::WindowsResult<u64> {
            use windows::Win32::{
                Foundation::FILETIME,
                System::Threading::{GetCurrentProcess, GetProcessTimes},
            };
            let mut creation = FILETIME::default();
            let mut exit = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            unsafe {
                GetProcessTimes(
                    GetCurrentProcess(),
                    &mut creation,
                    &mut exit,
                    &mut kernel,
                    &mut user,
                )
            }
            .map_err(|error| {
                super::WindowsError::new(
                    super::WindowsErrorKind::ApiFailure,
                    "GetProcessTimes for hot-corner evidence",
                    Some(i64::from(error.code().0)),
                )
            })?;
            let join = |value: FILETIME| {
                (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
            };
            Ok(join(kernel) + join(user))
        }

        let cpu_before = process_cpu_100ns().expect("read CPU time before polling");
        let wall_before = std::time::Instant::now();
        for _ in 0..25 {
            let _ = super::read_cursor_desktop_observation().expect("poll cursor and monitors");
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let wall_ms = wall_before.elapsed().as_millis();
        // 100ns 単位で返るので、マイクロ秒へ直す。
        //
        // **この数字は分解能の底に張り付く。** `GetProcessTimes` が刻むのは
        // 既定のタイマー間隔（15.625ms）単位なので、5秒で 15625us と出たら
        // それは「1ティック分」であって「15.6ms 使った」ではない。
        // 実際の消費は 0 から 15.6ms のどこか。**上限しか言えない。**
        // 上限として、1コアの約 0.3% を超えないことだけが分かる。
        let cpu_us = process_cpu_100ns()
            .expect("read CPU time after polling")
            .saturating_sub(cpu_before)
            / 10;
        println!(
            "EVIDENCE: hot_corner measured=true cursor=({}, {}) primary=({}, {}, {}, {}) outer_corners={outer:?} polls=25 wall_ms={wall_ms} cpu_us={cpu_us}",
            observation.cursor.x,
            observation.cursor.y,
            primary.left,
            primary.top,
            primary.right,
            primary.bottom,
        );
        assert!(!outer.is_empty(), "主モニターに外周角が1つ以上ある");
    }
}
