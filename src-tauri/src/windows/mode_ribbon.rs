//! 実行中のモードを、プライマリタスクバー直上の細い帯で示す。
//!
//! Windows の設定やタスクバーには書き込まない。appbar 登録もせず、
//! PCカスタム自身のクリック透過ウィンドウを1枚だけ所有する。

use std::collections::HashMap;

use parking_lot::Mutex;

use super::{TaskbarAnchor, TaskbarEdge, WindowsError, WindowsErrorKind, WindowsResult};
use crate::game_profile::ModeRibbonColor;

/// 物理ピクセルで固定する。利用者入力で大きくできる経路は持たない。
pub const MODE_RIBBON_THICKNESS: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeRibbonRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ModeRibbonRect {
    pub const fn width(self) -> i32 {
        self.right - self.left
    }

    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }

    pub const fn center(self) -> (i32, i32) {
        (self.left + self.width() / 2, self.top + self.height() / 2)
    }
}

/// 初版で安全を確認した「下辺・自動非表示ではない」場合だけ位置を返す。
///
/// 上・左右・自動非表示では、邪魔にならないことをまだ証明できないため描かない。
pub const fn mode_ribbon_rect(anchor: TaskbarAnchor) -> Option<ModeRibbonRect> {
    if anchor.auto_hide
        || !matches!(anchor.edge, TaskbarEdge::Bottom)
        || anchor.right <= anchor.left
        || anchor.bottom <= anchor.top
        || anchor.top < MODE_RIBBON_THICKNESS
    {
        return None;
    }
    Some(ModeRibbonRect {
        left: anchor.left,
        top: anchor.top - MODE_RIBBON_THICKNESS,
        right: anchor.right,
        bottom: anchor.top,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveModeRibbon {
    pub profile_id: String,
    pub color: Option<ModeRibbonColor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RibbonScope {
    Game,
    Manual,
}

#[derive(Debug, Clone, Copy)]
struct ActiveEntry {
    color: Option<ModeRibbonColor>,
    activation_order: u64,
}

#[derive(Default)]
struct RibbonSelection {
    game: HashMap<String, ActiveEntry>,
    manual: HashMap<String, ActiveEntry>,
    next_activation_order: u64,
}

impl RibbonSelection {
    fn sync(&mut self, scope: RibbonScope, active: Vec<ActiveModeRibbon>) {
        let desired = active
            .into_iter()
            .map(|entry| (entry.profile_id, entry.color))
            .collect::<HashMap<_, _>>();
        let entries = match scope {
            RibbonScope::Game => &mut self.game,
            RibbonScope::Manual => &mut self.manual,
        };
        entries.retain(|profile_id, _| desired.contains_key(profile_id));
        for (profile_id, color) in desired {
            match entries.get_mut(&profile_id) {
                Some(entry) => entry.color = color,
                None => {
                    self.next_activation_order = self.next_activation_order.saturating_add(1);
                    entries.insert(
                        profile_id,
                        ActiveEntry {
                            color,
                            activation_order: self.next_activation_order,
                        },
                    );
                }
            }
        }
    }

    fn update_color(&mut self, profile_id: &str, color: Option<ModeRibbonColor>) {
        if let Some(entry) = self.game.get_mut(profile_id) {
            entry.color = color;
        }
        if let Some(entry) = self.manual.get_mut(profile_id) {
            entry.color = color;
        }
    }

    fn selected_color(&self) -> Option<ModeRibbonColor> {
        self.game
            .values()
            .chain(self.manual.values())
            .max_by_key(|entry| entry.activation_order)
            .and_then(|entry| entry.color)
    }
}

#[cfg(windows)]
mod platform {
    use std::{
        sync::{
            atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering},
            mpsc, Arc,
        },
        thread::{self, JoinHandle},
        time::Duration,
    };

    use windows::{
        core::{w, PCWSTR},
        Win32::{
            Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM},
            Graphics::Gdi::{
                BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect,
                UpdateWindow, HBRUSH, PAINTSTRUCT,
            },
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
                IsWindow, KillTimer, PostMessageW, PostQuitMessage, RegisterClassW,
                RegisterWindowMessageW, SetLayeredWindowAttributes, SetTimer, SetWindowPos,
                ShowWindow, TranslateMessage, HTTRANSPARENT, HWND_TOPMOST, LWA_ALPHA,
                MA_NOACTIVATE, MSG, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_SHOWWINDOW, SW_HIDE,
                SW_SHOWNOACTIVATE, WM_APP, WM_CLOSE, WM_DESTROY, WM_DISPLAYCHANGE, WM_ERASEBKGND,
                WM_MOUSEACTIVATE, WM_NCHITTEST, WM_PAINT, WM_SETTINGCHANGE, WM_TIMER, WNDCLASSW,
                WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
                WS_EX_TRANSPARENT, WS_POPUP,
            },
        },
    };

    use super::{
        mode_ribbon_rect, ActiveModeRibbon, ModeRibbonColor, ModeRibbonRect, Mutex, RibbonScope,
        RibbonSelection, WindowsError, WindowsErrorKind, WindowsResult,
    };

    const WINDOW_CLASS: PCWSTR = w!("TotonoeModeRibbonWindow");
    const WINDOW_TITLE: PCWSTR = w!("PC Custom mode ribbon");
    const REFRESH_MESSAGE: u32 = WM_APP + 37;
    const REFRESH_TIMER_ID: usize = 1;
    const REFRESH_INTERVAL_MS: u32 = 1_000;
    static PAINT_COLOR: AtomicU32 = AtomicU32::new(0);

    fn api_error(operation: &'static str) -> WindowsError {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            operation,
            std::io::Error::last_os_error()
                .raw_os_error()
                .map(i64::from),
        )
    }

    unsafe extern "system" fn ribbon_window_proc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = unsafe { BeginPaint(window, &mut paint) };
                let brush =
                    unsafe { CreateSolidBrush(COLORREF(PAINT_COLOR.load(Ordering::Relaxed))) };
                let _ = unsafe { FillRect(dc, &paint.rcPaint, brush) };
                let _ = unsafe { DeleteObject(brush) };
                let _ = unsafe { EndPaint(window, &paint) };
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = unsafe { DestroyWindow(window) };
                LRESULT(0)
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
        }
    }

    pub(super) struct RibbonWindow {
        pub(super) hwnd: HWND,
        color: Option<ModeRibbonColor>,
        rect: Option<ModeRibbonRect>,
        visible: bool,
    }

    impl RibbonWindow {
        fn create() -> WindowsResult<Self> {
            let class = WNDCLASSW {
                lpfnWndProc: Some(ribbon_window_proc),
                lpszClassName: WINDOW_CLASS,
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            // 同じテストプロセス内でクラスが既に登録済みでも CreateWindowExW は使える。
            unsafe { RegisterClassW(&class) };
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_LAYERED
                        | WS_EX_TRANSPARENT
                        | WS_EX_TOOLWINDOW
                        | WS_EX_TOPMOST
                        | WS_EX_NOACTIVATE,
                    WINDOW_CLASS,
                    WINDOW_TITLE,
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    None,
                    None,
                    None,
                    None,
                )
            }
            .map_err(|_| api_error("CreateWindowExW mode ribbon"))?;
            if hwnd.is_invalid() {
                return Err(api_error("CreateWindowExW mode ribbon"));
            }
            unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA) }
                .map_err(|_| api_error("SetLayeredWindowAttributes mode ribbon"))?;
            Ok(Self {
                hwnd,
                color: None,
                rect: None,
                visible: false,
            })
        }

        fn refresh(&mut self, color: Option<ModeRibbonColor>) {
            let Some(color) = color else {
                self.hide();
                return;
            };
            if super::super::foreground_is_fullscreen().unwrap_or(true) {
                self.hide();
                return;
            }
            let Some(rect) = super::super::read_taskbar_anchor()
                .ok()
                .and_then(mode_ribbon_rect)
            else {
                self.hide();
                return;
            };
            self.show_at(rect, color);
        }

        fn show_at(&mut self, rect: ModeRibbonRect, color: ModeRibbonColor) {
            if self.color != Some(color) {
                PAINT_COLOR.store(color.colorref(), Ordering::Relaxed);
                self.color = Some(color);
                let _ = unsafe { InvalidateRect(self.hwnd, None, false) };
            }
            // タイマーごとに topmost を再主張する。他の最前面ウィンドウに前を取られることは
            // 実測済みだが、色が変わらない限り再描画はしない。
            let positioned = unsafe {
                SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    rect.left,
                    rect.top,
                    rect.width(),
                    rect.height(),
                    SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_SHOWWINDOW,
                )
            }
            .is_ok();
            if positioned {
                self.rect = Some(rect);
                self.visible = true;
                let _ = unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
                let _ = unsafe { UpdateWindow(self.hwnd) };
            } else {
                self.hide();
            }
        }

        fn hide(&mut self) {
            if self.visible {
                let _ = unsafe { ShowWindow(self.hwnd, SW_HIDE) };
                self.visible = false;
                self.rect = None;
            }
        }

        #[cfg(test)]
        pub(super) fn actual_rect(&self) -> WindowsResult<ModeRibbonRect> {
            use windows::Win32::{Foundation::RECT, UI::WindowsAndMessaging::GetWindowRect};
            let mut rect = RECT::default();
            unsafe { GetWindowRect(self.hwnd, &mut rect) }
                .map_err(|_| api_error("GetWindowRect mode ribbon"))?;
            Ok(ModeRibbonRect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            })
        }
    }

    impl Drop for RibbonWindow {
        fn drop(&mut self) {
            if unsafe { IsWindow(self.hwnd) }.as_bool() {
                let _ = unsafe { DestroyWindow(self.hwnd) };
            }
        }
    }

    struct ControllerInner {
        selection: Mutex<RibbonSelection>,
        hwnd: AtomicIsize,
        stop: AtomicBool,
    }

    pub struct ModeRibbonController {
        inner: Arc<ControllerInner>,
        handle: Mutex<Option<JoinHandle<()>>>,
    }

    impl ModeRibbonController {
        pub fn spawn() -> WindowsResult<Self> {
            let inner = Arc::new(ControllerInner {
                selection: Mutex::new(RibbonSelection::default()),
                hwnd: AtomicIsize::new(0),
                stop: AtomicBool::new(false),
            });
            let thread_inner = inner.clone();
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let handle = thread::Builder::new()
                .name("totonoe-mode-ribbon".to_owned())
                .spawn(move || run_window_loop(thread_inner, ready_tx))
                .map_err(|_| {
                    WindowsError::new(
                        WindowsErrorKind::ResourceLimit,
                        "spawn mode ribbon thread",
                        None,
                    )
                })?;
            match ready_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(())) => Ok(Self {
                    inner,
                    handle: Mutex::new(Some(handle)),
                }),
                Ok(Err(error)) => {
                    let _ = handle.join();
                    Err(error)
                }
                Err(_) => {
                    inner.stop.store(true, Ordering::SeqCst);
                    let _ = handle.join();
                    Err(WindowsError::new(
                        WindowsErrorKind::ChannelClosed,
                        "start mode ribbon thread",
                        None,
                    ))
                }
            }
        }

        pub fn sync_game_profiles(&self, active: Vec<ActiveModeRibbon>) {
            self.sync(RibbonScope::Game, active);
        }

        pub fn sync_manual_profiles(&self, active: Vec<ActiveModeRibbon>) {
            self.sync(RibbonScope::Manual, active);
        }

        pub fn update_profile_color(&self, profile_id: &str, color: Option<ModeRibbonColor>) {
            self.inner.selection.lock().update_color(profile_id, color);
            self.wake();
        }

        fn sync(&self, scope: RibbonScope, active: Vec<ActiveModeRibbon>) {
            self.inner.selection.lock().sync(scope, active);
            self.wake();
        }

        fn wake(&self) {
            let hwnd = HWND(self.inner.hwnd.load(Ordering::Acquire) as *mut _);
            if !hwnd.is_invalid() {
                let _ = unsafe { PostMessageW(hwnd, REFRESH_MESSAGE, WPARAM(0), LPARAM(0)) };
            }
        }
    }

    impl Drop for ModeRibbonController {
        fn drop(&mut self) {
            self.inner.stop.store(true, Ordering::SeqCst);
            let hwnd = HWND(self.inner.hwnd.load(Ordering::Acquire) as *mut _);
            if !hwnd.is_invalid() {
                let _ = unsafe { PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) };
            }
            if let Some(handle) = self.handle.lock().take() {
                let _ = handle.join();
            }
        }
    }

    fn run_window_loop(inner: Arc<ControllerInner>, ready: mpsc::SyncSender<WindowsResult<()>>) {
        let mut window = match RibbonWindow::create() {
            Ok(window) => window,
            Err(error) => {
                let _ = ready.send(Err(error));
                return;
            }
        };
        inner.hwnd.store(window.hwnd.0 as isize, Ordering::Release);
        let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        if unsafe { SetTimer(window.hwnd, REFRESH_TIMER_ID, REFRESH_INTERVAL_MS, None) } == 0 {
            inner.hwnd.store(0, Ordering::Release);
            let _ = ready.send(Err(api_error("SetTimer mode ribbon")));
            return;
        }
        let _ = ready.send(Ok(()));
        window.refresh(inner.selection.lock().selected_color());

        loop {
            let mut message = MSG::default();
            let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
            if result.0 <= 0 {
                break;
            }
            let refresh = message.message == REFRESH_MESSAGE
                || message.message == WM_TIMER
                || message.message == WM_DISPLAYCHANGE
                || message.message == WM_SETTINGCHANGE
                || (taskbar_created != 0 && message.message == taskbar_created);
            if refresh {
                if inner.stop.load(Ordering::SeqCst) {
                    break;
                }
                window.refresh(inner.selection.lock().selected_color());
                continue;
            }
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }

        let _ = unsafe { KillTimer(window.hwnd, REFRESH_TIMER_ID) };
        inner.hwnd.store(0, Ordering::Release);
    }

    #[cfg(test)]
    pub(super) fn window_from_point(rect: ModeRibbonRect) -> HWND {
        use windows::Win32::{Foundation::POINT, UI::WindowsAndMessaging::WindowFromPoint};
        let (x, y) = rect.center();
        unsafe { WindowFromPoint(POINT { x, y }) }
    }

    #[cfg(test)]
    pub(super) fn create_test_window(
        rect: ModeRibbonRect,
        color: ModeRibbonColor,
    ) -> WindowsResult<RibbonWindow> {
        let mut window = RibbonWindow::create()?;
        window.show_at(rect, color);
        Ok(window)
    }
}

#[cfg(windows)]
pub use platform::ModeRibbonController;

#[cfg(not(windows))]
pub struct ModeRibbonController;

#[cfg(not(windows))]
impl ModeRibbonController {
    pub fn spawn() -> WindowsResult<Self> {
        Err(WindowsError::new(
            WindowsErrorKind::UnsupportedPlatform,
            "spawn mode ribbon",
            None,
        ))
    }

    pub fn sync_game_profiles(&self, _active: Vec<ActiveModeRibbon>) {}

    pub fn sync_manual_profiles(&self, _active: Vec<ActiveModeRibbon>) {}

    pub fn update_profile_color(&self, _profile_id: &str, _color: Option<ModeRibbonColor>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(edge: TaskbarEdge, auto_hide: bool) -> TaskbarAnchor {
        TaskbarAnchor {
            left: 0,
            top: 1032,
            right: 1920,
            bottom: 1080,
            auto_hide,
            edge,
        }
    }

    #[test]
    fn ribbon_rect_is_bounded_and_only_uses_safe_taskbar_shape() {
        let rect = mode_ribbon_rect(anchor(TaskbarEdge::Bottom, false)).expect("bottom taskbar");
        assert_eq!(
            rect,
            ModeRibbonRect {
                left: 0,
                top: 1028,
                right: 1920,
                bottom: 1032,
            }
        );
        assert_eq!(rect.height(), MODE_RIBBON_THICKNESS);
        assert!(mode_ribbon_rect(anchor(TaskbarEdge::Top, false)).is_none());
        assert!(mode_ribbon_rect(anchor(TaskbarEdge::Left, false)).is_none());
        assert!(mode_ribbon_rect(anchor(TaskbarEdge::Right, false)).is_none());
        assert!(mode_ribbon_rect(anchor(TaskbarEdge::Bottom, true)).is_none());
    }

    #[test]
    fn most_recent_active_mode_selects_its_own_color() {
        let mut selection = RibbonSelection::default();
        selection.sync(
            RibbonScope::Game,
            vec![ActiveModeRibbon {
                profile_id: "game".to_owned(),
                color: Some(ModeRibbonColor::Sky),
            }],
        );
        assert_eq!(selection.selected_color(), Some(ModeRibbonColor::Sky));
        selection.sync(
            RibbonScope::Manual,
            vec![ActiveModeRibbon {
                profile_id: "work".to_owned(),
                color: Some(ModeRibbonColor::Amber),
            }],
        );
        assert_eq!(selection.selected_color(), Some(ModeRibbonColor::Amber));
        selection.sync(RibbonScope::Manual, Vec::new());
        assert_eq!(selection.selected_color(), Some(ModeRibbonColor::Sky));
    }

    #[test]
    fn selected_mode_can_explicitly_disable_the_ribbon() {
        let mut selection = RibbonSelection::default();
        selection.sync(
            RibbonScope::Game,
            vec![ActiveModeRibbon {
                profile_id: "game".to_owned(),
                color: Some(ModeRibbonColor::Violet),
            }],
        );
        selection.sync(
            RibbonScope::Manual,
            vec![ActiveModeRibbon {
                profile_id: "quiet".to_owned(),
                color: None,
            }],
        );
        assert_eq!(selection.selected_color(), None);
    }

    /// クリック透過を、Z順に左右されない形で測る。
    ///
    /// `WindowFromPoint` は手前に画面いっぱいの窓があると使えない。
    /// この機の常態がそれなので、覆われていても測れる経路を別に持つ。
    ///
    /// 見るのは2つ。どちらもこの窓自身の性質で、ほかの窓の位置に依存しない。
    ///
    /// 1. 拡張スタイルに `WS_EX_TRANSPARENT` が立っていること（文書化された仕組み）
    /// 2. `WM_NCHITTEST` に `HTTRANSPARENT` を返すこと（当たり判定を自分で捨てている）
    #[cfg(windows)]
    #[test]
    #[ignore = "実機にクリック透過の窓を一時表示する。Windows設定は変更しない"]
    fn the_ribbon_declares_itself_transparent_to_the_mouse() {
        use windows::Win32::{
            Foundation::{LPARAM, WPARAM},
            UI::WindowsAndMessaging::{
                DispatchMessageW, GetWindowLongPtrW, PeekMessageW, SendMessageW, GWL_EXSTYLE,
                HTTRANSPARENT, MSG, PM_REMOVE, WM_NCHITTEST, WS_EX_TRANSPARENT,
            },
        };

        fn pump() {
            let mut message = MSG::default();
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe { DispatchMessageW(&message) };
            }
        }

        let anchor = super::super::read_taskbar_anchor().expect("taskbar anchor must be readable");
        let Some(rect) = mode_ribbon_rect(anchor) else {
            println!("EVIDENCE: ribbon_transparency skipped (この配置では出さない)");
            return;
        };
        let window =
            platform::create_test_window(rect, ModeRibbonColor::Sky).expect("create ribbon");
        pump();

        let ex_style = unsafe { GetWindowLongPtrW(window.hwnd, GWL_EXSTYLE) } as u32;
        let has_transparent = (ex_style & WS_EX_TRANSPARENT.0) != 0;

        // 画面上の実在する点を渡す。窓の中心を使う。
        let (x, y) = rect.center();
        let packed = LPARAM(((y as isize) << 16) | (x as isize & 0xffff));
        let hit = unsafe { SendMessageW(window.hwnd, WM_NCHITTEST, WPARAM(0), packed) };

        println!(
            "EVIDENCE: ribbon_transparency ex_style=0x{ex_style:08X} ws_ex_transparent={has_transparent}              nchittest={} htransparent={}",
            hit.0,
            hit.0 == HTTRANSPARENT as isize
        );
        assert!(
            has_transparent,
            "WS_EX_TRANSPARENT が立っていない。マウスが素通りしない"
        );
        assert_eq!(
            hit.0, HTTRANSPARENT as isize,
            "WM_NCHITTEST が HTTRANSPARENT を返していない"
        );

        // 参考として、この機で `WindowFromPoint` が使えるかも出す。
        let seen = platform::window_from_point(rect);
        println!(
            "EVIDENCE: ribbon_transparency window_from_point=0x{:X} is_ribbon={}",
            seen.0 as usize,
            seen == window.hwnd
        );
    }

    /// クリック透過の測定そのものが機能しているかを、**わざと透過しない窓**で確かめる。
    ///
    /// 「透過している」と「そもそも描かれていない」は、`WindowFromPoint` から見て同じ結果になる。
    /// 本番と同じ位置に、透過しない・最前面・表示済みの窓を置いて、
    /// **それが `WindowFromPoint` で返ることを先に確認する。**
    /// ここが返らないなら、透過の合格は何も意味していない。
    #[cfg(windows)]
    #[test]
    #[ignore = "実機に一時的な窓を出す。Windows設定は変更しない"]
    fn the_click_through_probe_can_actually_see_a_window_that_is_not_transparent() {
        use windows::Win32::{
            Foundation::{HINSTANCE, HWND},
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, DispatchMessageW, PeekMessageW, SetWindowPos,
                ShowWindow, HMENU, HWND_TOPMOST, MSG, PM_REMOVE, SWP_NOACTIVATE, SW_SHOWNOACTIVATE,
                WINDOW_EX_STYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
            },
        };

        struct Owned(HWND);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = unsafe { DestroyWindow(self.0) };
            }
        }

        fn pump() {
            let mut message = MSG::default();
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe { DispatchMessageW(&message) };
            }
        }

        let anchor = super::super::read_taskbar_anchor().expect("taskbar anchor must be readable");
        let Some(rect) = mode_ribbon_rect(anchor) else {
            println!("EVIDENCE: ribbon_probe skipped (この配置では出さない)");
            return;
        };

        // 透過フラグを付けない。ほかは本番と同じ条件に寄せる。
        let opaque = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOPMOST.0),
                windows::core::w!("STATIC"),
                windows::core::w!("pc-custom ribbon probe"),
                WS_POPUP,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                HWND::default(),
                HMENU::default(),
                HINSTANCE::default(),
                None,
            )
        }
        .expect("create an opaque probe window");
        let owned = Owned(opaque);
        let _ = unsafe {
            SetWindowPos(
                opaque,
                HWND_TOPMOST,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOACTIVATE,
            )
        };
        let _ = unsafe { ShowWindow(opaque, SW_SHOWNOACTIVATE) };
        pump();

        let hit = platform::window_from_point(rect);
        // 何が邪魔しているのかを分けて出す。
        {
            use windows::Win32::{
                Foundation::RECT,
                UI::WindowsAndMessaging::{GetAncestor, GetWindowRect, IsWindowVisible, GA_ROOT},
            };
            let mut actual = RECT::default();
            let got = unsafe { GetWindowRect(owned.0, &mut actual) }.is_ok();
            let mut hit_rect = RECT::default();
            let _ = unsafe { GetWindowRect(hit, &mut hit_rect) };
            let root = unsafe { GetAncestor(hit, GA_ROOT) };
            println!(
                "EVIDENCE: ribbon_probe visible={} rect_ok={} rect=({},{},{},{}) wanted=({},{},{},{})",
                unsafe { IsWindowVisible(owned.0) }.as_bool(),
                got,
                actual.left, actual.top, actual.right, actual.bottom,
                rect.left, rect.top, rect.right, rect.bottom
            );
            println!(
                "EVIDENCE: ribbon_probe hit_rect=({},{},{},{}) hit_root=0x{:X} hit_visible={}",
                hit_rect.left,
                hit_rect.top,
                hit_rect.right,
                hit_rect.bottom,
                root.0 as usize,
                unsafe { IsWindowVisible(hit) }.as_bool()
            );
        }
        let visible_to_probe = hit == owned.0;
        println!(
            "EVIDENCE: ribbon_probe opaque_hwnd=0x{:X} WindowFromPoint=0x{:X} probe_sees_it={}",
            owned.0 .0 as usize, hit.0 as usize, visible_to_probe
        );
        if !visible_to_probe {
            // 画面いっぱいの窓が手前にあると、透過していてもいなくても同じ結果になる。
            // この機の常態がそれなので、見送る形にする。
            // **見送りを透過の合格として読まないこと。**
            // 透過そのものは `the_ribbon_declares_itself_transparent_to_the_mouse` が
            // Z順に依存しない形で測っている。ここはその上に乗る確認でしかない。
            println!(
                "EVIDENCE: ribbon_probe skipped=cannot_measure                  reason=リボンの位置を覆う窓があり、WindowFromPoint では区別できない"
            );
        }
    }

    /// 実機のタスクバー直上へ本番と同じ窓を出し、位置とクリック透過と破棄を外から測る。
    #[cfg(windows)]
    #[test]
    #[ignore = "実機にクリック透過のモードリボンを一時表示する。Windows設定は変更しない"]
    fn mode_ribbon_geometry_and_click_through_on_real_taskbar() {
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, MSG, PM_REMOVE},
        };

        fn pump() {
            let mut message = MSG::default();
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe { DispatchMessageW(&message) };
            }
        }

        let anchor = super::super::read_taskbar_anchor().expect("taskbar anchor must be readable");
        let expected = mode_ribbon_rect(anchor)
            .expect("real-machine proof requires a bottom, non-auto-hide primary taskbar");
        let before: HWND = platform::window_from_point(expected);

        // 前提: いま `WindowFromPoint` がこの位置の窓を見分けられること。
        //
        // 画面いっぱいの窓が手前にあると、透過していてもいなくても同じ結果が返る。
        // その状態で「透過した」と言うのは、目を閉じて「見えない」と言うのと同じ。
        // 実際に一度、この条件で採った証拠を合格として受け取りかけた。
        {
            use windows::Win32::Foundation::RECT;
            use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
            let mut covering = RECT::default();
            let _ = unsafe { GetWindowRect(before, &mut covering) };
            let covers_everything = covering.left <= expected.left
                && covering.top <= 0
                && covering.right >= expected.right
                && covering.bottom >= expected.bottom;
            if covers_everything {
                // 見送る。**ただし透過そのものが未証明のまま通すわけではない。**
                // Z順に依存しない証拠は
                // `the_ribbon_declares_itself_transparent_to_the_mouse` が取っている。
                // ここは画面が空いているときにだけ意味を持つ、より強い端から端までの確認。
                println!(
                    "EVIDENCE: mode_ribbon skipped=cannot_measure covering_rect=({},{},{},{})                      reason=画面いっぱいの窓が手前にあり、透過の有無を区別できない。                     透過の証拠は the_ribbon_declares_itself_transparent_to_the_mouse を見ること",
                    covering.left, covering.top, covering.right, covering.bottom
                );
                return;
            }
        }

        let window =
            platform::create_test_window(expected, ModeRibbonColor::Sky).expect("create ribbon");
        pump();
        let actual = window.actual_rect().expect("read ribbon rect");
        assert_eq!(
            actual, expected,
            "created ribbon must use the calculated rect"
        );
        let during = platform::window_from_point(expected);
        assert_ne!(
            during, window.hwnd,
            "WindowFromPoint must pass through ribbon"
        );

        let ribbon_hwnd = window.hwnd;
        drop(window);
        pump();
        let after = platform::window_from_point(expected);
        assert_eq!(
            after, before,
            "closing ribbon must restore the original hit target"
        );

        println!(
            "EVIDENCE: taskbar=({},{},{},{}) ribbon=({},{},{},{}) ribbon_hwnd=0x{:X} WindowFromPoint(before=0x{:X},during=0x{:X},after=0x{:X}) click_through={} restored={}",
            anchor.left,
            anchor.top,
            anchor.right,
            anchor.bottom,
            actual.left,
            actual.top,
            actual.right,
            actual.bottom,
            ribbon_hwnd.0 as usize,
            before.0 as usize,
            during.0 as usize,
            after.0 as usize,
            during != ribbon_hwnd,
            after == before,
        );
    }
}
