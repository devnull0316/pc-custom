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
}
