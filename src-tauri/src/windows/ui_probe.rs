//! Windows の実UI状態を、ピクセルを見ずに観測するための読み取り専用プローブ。
//!
//! 目的: 42件の候補Actionは「第三者アプリの書き込みがWindows UIへ反映されること」の
//! 証拠が無いため変更不能に据え置かれている。UI Automation で実UIの位置や状態を
//! 機械的に読めるなら、目視なしでその往復証拠を作れる。
//!
//! ここでは一切変更を行わない。観測のみ。

use super::{WindowsError, WindowsErrorKind, WindowsResult};

/// タスクバー上のスタートボタンの位置（画面座標）と、タスクバー全体の幅。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskbarLayoutObservation {
    pub taskbar_left: i32,
    pub taskbar_width: i32,
    pub start_button_left: i32,
    pub start_button_width: i32,
}

impl TaskbarLayoutObservation {
    /// スタートボタンの中心が、タスクバー幅のどのあたりか（0.0=左端, 1.0=右端）。
    /// 左寄せなら小さく、中央寄せなら 0.5 付近になる。
    pub fn start_center_ratio(&self) -> f64 {
        if self.taskbar_width <= 0 {
            return 0.0;
        }
        let center = f64::from(self.start_button_left) + f64::from(self.start_button_width) / 2.0;
        (center - f64::from(self.taskbar_left)) / f64::from(self.taskbar_width)
    }
}

#[cfg(windows)]
pub fn observe_taskbar_layout() -> WindowsResult<TaskbarLayoutObservation> {
    use windows::core::BSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
        UIA_NamePropertyId,
    };
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowRect};

    fn fail(operation: &'static str) -> WindowsError {
        WindowsError::new(WindowsErrorKind::ApiFailure, operation, None)
    }

    unsafe {
        // タスクバー本体のHWNDと矩形。ここはCOM不要。
        let taskbar: HWND = FindWindowW(windows::core::w!("Shell_TrayWnd"), None)
            .map_err(|_| fail("FindWindowW Shell_TrayWnd"))?;
        let mut bar = Default::default();
        GetWindowRect(taskbar, &mut bar).map_err(|_| fail("GetWindowRect taskbar"))?;

        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let owns_com = init.is_ok();
        let result = (|| -> WindowsResult<TaskbarLayoutObservation> {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .map_err(|_| fail("CoCreateInstance CUIAutomation"))?;
            let root: IUIAutomationElement = automation
                .ElementFromHandle(taskbar)
                .map_err(|_| fail("ElementFromHandle taskbar"))?;
            // 「スタート」ボタンは表示言語で名前が変わるため、日本語/英語の両方を試す。
            for name in ["スタート", "Start"] {
                let condition = automation
                    .CreatePropertyCondition(
                        UIA_NamePropertyId,
                        &windows::core::VARIANT::from(BSTR::from(name)),
                    )
                    .map_err(|_| fail("CreatePropertyCondition"))?;
                if let Ok(element) = root.FindFirst(TreeScope_Descendants, &condition) {
                    if let Ok(rect) = element.CurrentBoundingRectangle() {
                        if rect.right > rect.left {
                            return Ok(TaskbarLayoutObservation {
                                taskbar_left: bar.left,
                                taskbar_width: bar.right - bar.left,
                                start_button_left: rect.left,
                                start_button_width: rect.right - rect.left,
                            });
                        }
                    }
                }
            }
            Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "start button element not found",
                None,
            ))
        })();
        if owns_com {
            CoUninitialize();
        }
        result
    }
}

/// タスクバー上の全要素名。名前は「時計 11:15」のように付随情報を含むため、
/// 判定は完全一致ではなく**部分一致**で行うこと（完全一致は取りこぼす）。
#[cfg(windows)]
pub fn taskbar_element_names() -> WindowsResult<Vec<String>> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
    };
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    fn fail(operation: &'static str) -> WindowsError {
        WindowsError::new(WindowsErrorKind::ApiFailure, operation, None)
    }

    unsafe {
        let taskbar: HWND = FindWindowW(windows::core::w!("Shell_TrayWnd"), None)
            .map_err(|_| fail("FindWindowW Shell_TrayWnd"))?;
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let owns_com = init.is_ok();
        let result = (|| -> WindowsResult<Vec<String>> {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .map_err(|_| fail("CoCreateInstance"))?;
            let root: IUIAutomationElement = automation
                .ElementFromHandle(taskbar)
                .map_err(|_| fail("ElementFromHandle"))?;
            let condition = automation
                .CreateTrueCondition()
                .map_err(|_| fail("CreateTrueCondition"))?;
            let all = root
                .FindAll(TreeScope_Descendants, &condition)
                .map_err(|_| fail("FindAll"))?;
            let count = all.Length().map_err(|_| fail("Length"))?;
            let mut names = Vec::new();
            for index in 0..count {
                if let Ok(element) = all.GetElement(index) {
                    // UIA は**非表示の要素も列挙する**。存在するかではなく見えているかで判定したいので、
                    // 画面外扱いのものと、面積を持たないものを落とす。
                    // これを入れる前は「ステータスバーを消したのに要素は残る」ため、
                    // 反映されていないのか隠れているだけなのかを区別できなかった。
                    if element
                        .CurrentIsOffscreen()
                        .map(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    if let Ok(rect) = element.CurrentBoundingRectangle() {
                        if rect.right <= rect.left || rect.bottom <= rect.top {
                            continue;
                        }
                    }
                    if let Ok(name) = element.CurrentName() {
                        let text = name.to_string();
                        if !text.trim().is_empty() {
                            names.push(text);
                        }
                    }
                }
            }
            Ok(names)
        })();
        if owns_com {
            CoUninitialize();
        }
        result
    }
}

#[cfg(not(windows))]
pub fn taskbar_element_names() -> WindowsResult<Vec<String>> {
    Err(WindowsError::unsupported("taskbar element names"))
}

/// タスクバーの時計が表示している文字列（例: 「時計 11:15」）。
#[cfg(windows)]
pub fn observe_taskbar_clock_text() -> WindowsResult<String> {
    let names = taskbar_element_names()?;
    names
        .into_iter()
        .find(|name| name.starts_with("時計") || name.starts_with("Clock"))
        .ok_or_else(|| {
            WindowsError::new(
                WindowsErrorKind::InvalidData,
                "clock element not found",
                None,
            )
        })
}

#[cfg(not(windows))]
pub fn observe_taskbar_clock_text() -> WindowsResult<String> {
    Err(WindowsError::unsupported("observe taskbar clock"))
}

/// フォルダーオプションの文書化された設定API。
/// レジストリへ直接書くのと違い、Windows自身が使う経路なので必要な通知も内部で行われる。
/// `_bitfield1` のビット0が「すべてのファイルを表示」、ビット1が「拡張子を表示」。
#[cfg(windows)]
pub fn shell_state_show_hidden() -> WindowsResult<bool> {
    use windows::Win32::Foundation::FALSE;
    use windows::Win32::UI::Shell::{SHGetSetSettings, SHELLSTATEA, SSF_SHOWALLOBJECTS};

    let mut state = SHELLSTATEA::default();
    unsafe {
        SHGetSetSettings(Some(&mut state), SSF_SHOWALLOBJECTS, FALSE);
    }
    Ok(state._bitfield1 & 1 != 0)
}

/// 「すべてのファイルを表示」を設定する（文書化API経由）。
#[cfg(windows)]
pub fn set_shell_state_show_hidden(show: bool) -> WindowsResult<()> {
    use windows::Win32::Foundation::{FALSE, TRUE};
    use windows::Win32::UI::Shell::{SHGetSetSettings, SHELLSTATEA, SSF_SHOWALLOBJECTS};

    let mut state = SHELLSTATEA::default();
    unsafe {
        SHGetSetSettings(Some(&mut state), SSF_SHOWALLOBJECTS, FALSE);
        if show {
            state._bitfield1 |= 1;
        } else {
            state._bitfield1 &= !1;
        }
        SHGetSetSettings(Some(&mut state), SSF_SHOWALLOBJECTS, TRUE);
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn shell_state_show_hidden() -> WindowsResult<bool> {
    Err(WindowsError::unsupported("shell state"))
}

#[cfg(not(windows))]
pub fn set_shell_state_show_hidden(_show: bool) -> WindowsResult<()> {
    Err(WindowsError::unsupported("set shell state"))
}

/// ファイル／フォルダーの説明ポップアップ設定を、文書化された
/// `SHGetSetSettings` / `SSF_SHOWINFOTIP` 経由で読み取る。
#[cfg(windows)]
pub fn shell_state_show_info_tip() -> WindowsResult<bool> {
    use windows::Win32::Foundation::FALSE;
    use windows::Win32::UI::Shell::{SHGetSetSettings, SHELLSTATEA, SSF_SHOWINFOTIP};

    const SHOW_INFO_TIP_BIT: i32 = 1 << 11;

    let mut state = SHELLSTATEA::default();
    unsafe {
        SHGetSetSettings(Some(&mut state), SSF_SHOWINFOTIP, FALSE);
    }
    Ok(state._bitfield1 & SHOW_INFO_TIP_BIT != 0)
}

/// ファイル／フォルダーの説明ポップアップ設定を書き、同じ公開 API で必ず読み直す。
///
/// `SHGetSetSettings` の戻り値は `void` なので、呼び出せたことを成功とは扱わない。
#[cfg(windows)]
pub fn set_shell_state_show_info_tip(show: bool) -> WindowsResult<()> {
    use windows::Win32::Foundation::{FALSE, TRUE};
    use windows::Win32::UI::Shell::{SHGetSetSettings, SHELLSTATEA, SSF_SHOWINFOTIP};

    const SHOW_INFO_TIP_BIT: i32 = 1 << 11;

    let mut state = SHELLSTATEA::default();
    unsafe {
        SHGetSetSettings(Some(&mut state), SSF_SHOWINFOTIP, FALSE);
        if show {
            state._bitfield1 |= SHOW_INFO_TIP_BIT;
        } else {
            state._bitfield1 &= !SHOW_INFO_TIP_BIT;
        }
        SHGetSetSettings(Some(&mut state), SSF_SHOWINFOTIP, TRUE);
    }

    if shell_state_show_info_tip()? != show {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "SHGetSetSettings info-tip readback",
            None,
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn shell_state_show_info_tip() -> WindowsResult<bool> {
    Err(WindowsError::unsupported("read shell info-tip state"))
}

#[cfg(not(windows))]
pub fn set_shell_state_show_info_tip(_show: bool) -> WindowsResult<()> {
    Err(WindowsError::unsupported("set shell info-tip state"))
}

/// フォルダーウィンドウを別プロセスで開く設定を、文書化された
/// `SHGetSetSettings` / `SSF_SEPPROCESS` 経由で読み取る。
#[cfg(windows)]
pub fn shell_state_separate_process() -> WindowsResult<bool> {
    use windows::Win32::Foundation::FALSE;
    use windows::Win32::UI::Shell::{SHGetSetSettings, SHELLSTATEA, SSF_SEPPROCESS};

    let mut state = SHELLSTATEA::default();
    unsafe {
        SHGetSetSettings(Some(&mut state), SSF_SEPPROCESS, FALSE);
    }
    Ok(state._bitfield2 & 1 != 0)
}

/// フォルダーウィンドウを別プロセスで開く設定を書き、同じ公開 API で必ず読み直す。
///
/// `SHGetSetSettings` の戻り値は `void` なので、呼び出せたことを成功とは扱わない。
#[cfg(windows)]
pub fn set_shell_state_separate_process(enabled: bool) -> WindowsResult<()> {
    use windows::Win32::Foundation::{FALSE, TRUE};
    use windows::Win32::UI::Shell::{SHGetSetSettings, SHELLSTATEA, SSF_SEPPROCESS};

    let mut state = SHELLSTATEA::default();
    unsafe {
        SHGetSetSettings(Some(&mut state), SSF_SEPPROCESS, FALSE);
        if enabled {
            state._bitfield2 |= 1;
        } else {
            state._bitfield2 &= !1;
        }
        SHGetSetSettings(Some(&mut state), SSF_SEPPROCESS, TRUE);
    }

    if shell_state_separate_process()? != enabled {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "SHGetSetSettings separate process readback",
            None,
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn shell_state_separate_process() -> WindowsResult<bool> {
    Err(WindowsError::unsupported(
        "read shell separate process state",
    ))
}

#[cfg(not(windows))]
pub fn set_shell_state_separate_process(_enabled: bool) -> WindowsResult<()> {
    Err(WindowsError::unsupported(
        "set shell separate process state",
    ))
}

/// タイトルに `needle` を含む Explorer ウィンドウを1つ探す。
///
/// `FindWindowW` は最初に見つかったウィンドウを返すため、利用者が既に開いている
/// 別のウィンドウを掴んでしまう。列挙して**部分一致**で自分の対象だけを選ぶ。
#[cfg(windows)]
pub fn find_explorer_window_by_title(needle: &str) -> WindowsResult<isize> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextW, IsWindowVisible,
    };

    struct Search {
        needle: String,
        found: Option<isize>,
    }

    unsafe extern "system" fn callback(window: HWND, param: LPARAM) -> BOOL {
        let search = &mut *(param.0 as *mut Search);
        if search.found.is_some() {
            return TRUE;
        }
        if !IsWindowVisible(window).as_bool() {
            return TRUE;
        }
        let mut class_buffer = [0u16; 128];
        let class_len = GetClassNameW(window, &mut class_buffer);
        if class_len <= 0 {
            return TRUE;
        }
        let class_name = String::from_utf16_lossy(&class_buffer[..class_len as usize]);
        if class_name != "CabinetWClass" {
            return TRUE;
        }
        let mut title_buffer = [0u16; 512];
        let title_len = GetWindowTextW(window, &mut title_buffer);
        if title_len <= 0 {
            return TRUE;
        }
        let title = String::from_utf16_lossy(&title_buffer[..title_len as usize]);
        if title.contains(&search.needle) {
            search.found = Some(window.0 as isize);
        }
        TRUE
    }

    let mut search = Search {
        needle: needle.to_owned(),
        found: None,
    };
    unsafe {
        let _ = EnumWindows(Some(callback), LPARAM(&mut search as *mut Search as isize));
    }
    search.found.ok_or_else(|| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "explorer window with the requested title not found",
            None,
        )
    })
}

#[cfg(not(windows))]
pub fn find_explorer_window_by_title(_needle: &str) -> WindowsResult<isize> {
    Err(WindowsError::unsupported("find explorer window"))
}

/// 開いているExplorerウィンドウが一覧表示している項目名。
/// Explorerは別プロセスなので、自プロセスのキャッシュに影響されない観測点になる。
#[cfg(windows)]
pub fn explorer_window_item_names(window_handle: isize) -> WindowsResult<Vec<String>> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
    };
    fn fail(operation: &'static str) -> WindowsError {
        WindowsError::new(WindowsErrorKind::ApiFailure, operation, None)
    }

    unsafe {
        let window = HWND(window_handle as *mut core::ffi::c_void);
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let owns_com = init.is_ok();
        let result = (|| -> WindowsResult<Vec<String>> {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .map_err(|_| fail("CoCreateInstance"))?;
            let root: IUIAutomationElement = automation
                .ElementFromHandle(window)
                .map_err(|_| fail("ElementFromHandle explorer"))?;
            let condition = automation
                .CreateTrueCondition()
                .map_err(|_| fail("CreateTrueCondition"))?;
            let all = root
                .FindAll(TreeScope_Descendants, &condition)
                .map_err(|_| fail("FindAll"))?;
            let count = all.Length().map_err(|_| fail("Length"))?;
            let mut names = Vec::new();
            for index in 0..count {
                if let Ok(element) = all.GetElement(index) {
                    if let Ok(name) = element.CurrentName() {
                        let text = name.to_string();
                        if !text.trim().is_empty() {
                            names.push(text);
                        }
                    }
                }
            }
            Ok(names)
        })();
        if owns_com {
            CoUninitialize();
        }
        result
    }
}

/// エクスプローラーの窓の中で、名前が一致する要素の矩形を返す。
///
/// 「要素が在るか」では、ステータスバーの表示切替を判定できなかった。
/// UIA は非表示のものも列挙するためで、`CurrentIsOffscreen` も当てにならなかった。
/// 代わりに**面積で見る**。ステータスバーが出れば一覧の領域はその分縮む。
#[cfg(windows)]
pub fn explorer_element_rect(
    window_handle: isize,
    needle: &str,
) -> WindowsResult<Option<(i32, i32, i32, i32)>> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
    };

    fn fail(operation: &'static str) -> WindowsError {
        WindowsError::new(WindowsErrorKind::ApiFailure, operation, None)
    }

    unsafe {
        let window = HWND(window_handle as *mut core::ffi::c_void);
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let owns_com = init.is_ok();
        let result = (|| -> WindowsResult<Option<(i32, i32, i32, i32)>> {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .map_err(|_| fail("CoCreateInstance"))?;
            let root: IUIAutomationElement = automation
                .ElementFromHandle(window)
                .map_err(|_| fail("ElementFromHandle explorer"))?;
            let condition = automation
                .CreateTrueCondition()
                .map_err(|_| fail("CreateTrueCondition"))?;
            let all = root
                .FindAll(TreeScope_Descendants, &condition)
                .map_err(|_| fail("FindAll"))?;
            let count = all.Length().map_err(|_| fail("Length"))?;
            for index in 0..count {
                let Ok(element) = all.GetElement(index) else {
                    continue;
                };
                let Ok(name) = element.CurrentName() else {
                    continue;
                };
                if !name.to_string().contains(needle) {
                    continue;
                }
                if let Ok(rect) = element.CurrentBoundingRectangle() {
                    return Ok(Some((rect.left, rect.top, rect.right, rect.bottom)));
                }
            }
            Ok(None)
        })();
        if owns_com {
            CoUninitialize();
        }
        result
    }
}

#[cfg(not(windows))]
pub fn explorer_element_rect(
    _window_handle: isize,
    _needle: &str,
) -> WindowsResult<Option<(i32, i32, i32, i32)>> {
    Err(WindowsError::unsupported("explorer element rect"))
}

#[cfg(not(windows))]
pub fn explorer_window_item_names(window_handle: isize) -> WindowsResult<Vec<String>> {
    Err(WindowsError::unsupported("explorer window items"))
}

/// タスクバー上に、指定した名前の要素が存在するか。表示切替の反映確認に使う。
#[cfg(windows)]
pub fn observe_taskbar_element(needles: &[&str]) -> WindowsResult<bool> {
    let names = taskbar_element_names()?;
    Ok(names
        .iter()
        .any(|name| needles.iter().any(|needle| name.contains(needle))))
}

#[cfg(not(windows))]
pub fn observe_taskbar_element(_names: &[&str]) -> WindowsResult<bool> {
    Err(WindowsError::unsupported("observe taskbar element"))
}

#[cfg(not(windows))]
pub fn observe_taskbar_layout() -> WindowsResult<TaskbarLayoutObservation> {
    Err(WindowsError::unsupported("observe taskbar layout"))
}

/// シェルが利用者へ見せるファイル表示名。拡張子を隠す設定を反映するため、
/// 「拡張子表示」が実際に効いているかをウィンドウを開かずに判定できる。
#[cfg(windows)]
pub fn shell_display_name(path: &std::path::Path) -> WindowsResult<String> {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
    use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_DISPLAYNAME};

    let wide = HSTRING::from(path.as_os_str());
    let mut info = SHFILEINFOW::default();
    let ok = unsafe {
        SHGetFileInfoW(
            &wide,
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_DISPLAYNAME,
        )
    };
    if ok == 0 {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "SHGetFileInfoW display name",
            None,
        ));
    }
    let end = info
        .szDisplayName
        .iter()
        .position(|c| *c == 0)
        .unwrap_or(info.szDisplayName.len());
    Ok(String::from_utf16_lossy(&info.szDisplayName[..end]))
}

#[cfg(not(windows))]
pub fn shell_display_name(_path: &std::path::Path) -> WindowsResult<String> {
    Err(WindowsError::unsupported("shell display name"))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::backup::{
        prepare_registry_backup, read_registry_state, restore_registry_backup, RegistryBackup,
        RegistryRestoreOutcome, RegistryTarget,
    };
    use crate::windows::{notify_explorer_settings_changed, notify_theme_changed, write_raw_value};
    use std::{
        collections::HashSet,
        path::Path,
        thread::sleep,
        time::{Duration, Instant},
    };
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT, TRUE, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetPixel,
        GetWindowDC, ReleaseDC, SelectObject, CLR_INVALID, HDC,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
        UIA_ListItemControlTypeId,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowW, GetClassNameW, GetShellWindow, GetWindowRect, GetWindowTextW,
        GetWindowThreadProcessId, IsWindow, IsWindowVisible, PostMessageW, SetForegroundWindow,
        ShowWindow, SW_SHOWMAXIMIZED, WM_CLOSE,
    };

    const REG_DWORD: u32 = 4;

    #[link(name = "user32")]
    extern "system" {
        fn PrintWindow(window: HWND, target: HDC, flags: u32) -> BOOL;
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ProbeNotification {
        Explorer,
        Theme,
    }

    struct RegistryRestoreGuard {
        entries: Vec<RegistryBackup>,
        notification: ProbeNotification,
        restored: bool,
    }

    impl RegistryRestoreGuard {
        fn new(entries: Vec<RegistryBackup>, notification: ProbeNotification) -> Self {
            Self {
                entries,
                notification,
                restored: false,
            }
        }

        fn apply(&self) {
            for entry in &self.entries {
                write_raw_value(&entry.location, entry.intended_type, &entry.intended_raw)
                    .expect("write probe value");
                let applied =
                    read_registry_state(&entry.location).expect("read applied probe value");
                assert_eq!(
                    applied,
                    entry.applied_state(),
                    "probe value was applied exactly"
                );
            }
            self.notify();
        }

        fn restore_and_assert(&mut self) {
            for entry in self.entries.iter().rev() {
                let outcome = restore_registry_backup(entry).expect("restore probe value");
                assert!(matches!(
                    outcome,
                    RegistryRestoreOutcome::Restored | RegistryRestoreOutcome::AlreadyOriginal
                ));
            }
            self.notify();
            for entry in &self.entries {
                let restored =
                    read_registry_state(&entry.location).expect("read restored probe value");
                assert_eq!(
                    restored, entry.original,
                    "value, type, bytes, and absence must be restored exactly"
                );
            }
            self.restored = true;
        }

        fn notify(&self) {
            match self.notification {
                ProbeNotification::Explorer => {
                    let _ = notify_explorer_settings_changed();
                }
                ProbeNotification::Theme => {
                    let _ = notify_theme_changed();
                }
            }
        }
    }

    impl Drop for RegistryRestoreGuard {
        fn drop(&mut self) {
            if self.restored {
                return;
            }
            for entry in self.entries.iter().rev() {
                if let Err(error) = restore_registry_backup(entry) {
                    eprintln!("emergency probe restoration failed: {error}");
                }
            }
            self.notify();
        }
    }

    #[derive(Debug, Clone)]
    struct ExplorerWindowInfo {
        handle: isize,
        title: String,
    }

    fn explorer_windows() -> Vec<ExplorerWindowInfo> {
        unsafe extern "system" fn callback(window: HWND, param: LPARAM) -> BOOL {
            let result = &mut *(param.0 as *mut Vec<ExplorerWindowInfo>);
            if !IsWindowVisible(window).as_bool() {
                return TRUE;
            }
            let mut class_buffer = [0u16; 128];
            let class_len = GetClassNameW(window, &mut class_buffer);
            if class_len <= 0 {
                return TRUE;
            }
            if String::from_utf16_lossy(&class_buffer[..class_len as usize]) != "CabinetWClass" {
                return TRUE;
            }
            let mut title_buffer = [0u16; 512];
            let title_len = GetWindowTextW(window, &mut title_buffer);
            if title_len <= 0 {
                return TRUE;
            }
            result.push(ExplorerWindowInfo {
                handle: window.0 as isize,
                title: String::from_utf16_lossy(&title_buffer[..title_len as usize]),
            });
            TRUE
        }

        let mut result = Vec::new();
        unsafe {
            let _ = EnumWindows(
                Some(callback),
                LPARAM(&mut result as *mut Vec<ExplorerWindowInfo> as isize),
            );
        }
        result
    }

    struct OwnedExplorerWindow {
        handle: Option<isize>,
    }

    impl OwnedExplorerWindow {
        fn open(path: &Path, title_needle: &str) -> WindowsResult<Self> {
            let existing: HashSet<isize> = explorer_windows()
                .into_iter()
                .map(|window| window.handle)
                .collect();
            std::process::Command::new("explorer.exe")
                .arg(format!("/n,{}", path.display()))
                .spawn()
                .map_err(|error| WindowsError::io("launch owned Explorer", &error))?;
            let deadline = Instant::now() + Duration::from_secs(8);
            while Instant::now() < deadline {
                sleep(Duration::from_millis(200));
                if let Some(window) = explorer_windows().into_iter().find(|window| {
                    !existing.contains(&window.handle) && window.title.contains(title_needle)
                }) {
                    let handle = HWND(window.handle as *mut core::ffi::c_void);
                    unsafe {
                        let _ = ShowWindow(handle, SW_SHOWMAXIMIZED);
                        let _ = SetForegroundWindow(handle);
                    }
                    sleep(Duration::from_millis(700));
                    return Ok(Self {
                        handle: Some(window.handle),
                    });
                }
            }
            eprintln!("Explorer windows after launch: {:?}", explorer_windows());
            Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "new owned Explorer window not found",
                None,
            ))
        }

        fn handle(&self) -> isize {
            self.handle.expect("owned Explorer window is open")
        }

        fn process_id(&self) -> WindowsResult<u32> {
            let mut process_id = 0;
            unsafe {
                GetWindowThreadProcessId(
                    HWND(self.handle() as *mut core::ffi::c_void),
                    Some(&mut process_id),
                );
            }
            if process_id == 0 {
                return Err(WindowsError::new(
                    WindowsErrorKind::ApiFailure,
                    "GetWindowThreadProcessId owned Explorer",
                    None,
                ));
            }
            Ok(process_id)
        }

        fn close_and_assert(mut self) {
            let handle = self.handle.take().expect("owned Explorer window is open");
            close_owned_explorer_window(handle, true);
        }
    }

    impl Drop for OwnedExplorerWindow {
        fn drop(&mut self) {
            if let Some(handle) = self.handle.take() {
                close_owned_explorer_window(handle, false);
            }
        }
    }

    fn close_owned_explorer_window(handle: isize, assert_closed: bool) {
        let window = HWND(handle as *mut core::ffi::c_void);
        unsafe {
            let _ = PostMessageW(window, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if !unsafe { IsWindow(window) }.as_bool() {
                return;
            }
            sleep(Duration::from_millis(100));
        }
        if assert_closed {
            panic!("owned Explorer window did not close");
        }
        eprintln!("owned Explorer window did not close during emergency cleanup");
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ExplorerItemObservation {
        bounds: RECT,
    }

    fn explorer_item_observation(
        window_handle: isize,
        exact_name: &str,
    ) -> WindowsResult<ExplorerItemObservation> {
        fn fail(operation: &'static str) -> WindowsError {
            WindowsError::new(WindowsErrorKind::ApiFailure, operation, None)
        }

        unsafe {
            let window = HWND(window_handle as *mut core::ffi::c_void);
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let owns_com = init.is_ok();
            let result = (|| -> WindowsResult<ExplorerItemObservation> {
                let automation: IUIAutomation =
                    CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                        .map_err(|_| fail("CoCreateInstance"))?;
                let root: IUIAutomationElement = automation
                    .ElementFromHandle(window)
                    .map_err(|_| fail("ElementFromHandle explorer"))?;
                let condition = automation
                    .CreateTrueCondition()
                    .map_err(|_| fail("CreateTrueCondition"))?;
                let all = root
                    .FindAll(TreeScope_Descendants, &condition)
                    .map_err(|_| fail("FindAll"))?;
                let count = all.Length().map_err(|_| fail("Length"))?;
                for index in 0..count {
                    let Ok(element) = all.GetElement(index) else {
                        continue;
                    };
                    let Ok(name) = element.CurrentName() else {
                        continue;
                    };
                    if name != exact_name
                        || element.CurrentControlType().ok() != Some(UIA_ListItemControlTypeId)
                    {
                        continue;
                    }
                    let bounds = element
                        .CurrentBoundingRectangle()
                        .map_err(|_| fail("CurrentBoundingRectangle"))?;
                    return Ok(ExplorerItemObservation { bounds });
                }
                Err(WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "requested Explorer list item not found",
                    None,
                ))
            })();
            if owns_com {
                CoUninitialize();
            }
            result
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ExplorerRowLayout {
        item_height: i32,
        row_pitch: i32,
        list_height: i32,
    }

    fn explorer_row_layout(
        window_handle: isize,
        item_names: &[&str],
    ) -> WindowsResult<ExplorerRowLayout> {
        let mut bounds = Vec::with_capacity(item_names.len());
        for name in item_names {
            bounds.push(explorer_item_observation(window_handle, name)?.bounds);
        }
        bounds.sort_by_key(|rect| (rect.top, rect.left));
        let mut pitches = Vec::new();
        for pair in bounds.windows(2) {
            let pitch = pair[1].top - pair[0].top;
            if pitch > 0 && (pair[1].left - pair[0].left).abs() < 8 {
                pitches.push(pitch);
            }
        }
        pitches.sort_unstable();
        let Some(row_pitch) = pitches.get(pitches.len() / 2).copied() else {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "Explorer items are not in a measurable vertical list",
                None,
            ));
        };
        let mut heights: Vec<i32> = bounds.iter().map(|rect| rect.bottom - rect.top).collect();
        heights.sort_unstable();
        Ok(ExplorerRowLayout {
            item_height: heights[heights.len() / 2],
            row_pitch,
            list_height: bounds.last().expect("measured item bounds").bottom
                - bounds.first().expect("measured item bounds").top,
        })
    }

    fn explorer_item_lefts(window_handle: isize, item_names: &[&str]) -> WindowsResult<Vec<i32>> {
        item_names
            .iter()
            .map(|name| Ok(explorer_item_observation(window_handle, name)?.bounds.left))
            .collect()
    }

    #[derive(Debug, Clone, Copy)]
    struct PixelStats {
        samples: usize,
        luminance_mean: f64,
        luminance_variance: f64,
        saturation_mean: f64,
        saturation_variance: f64,
    }

    fn pixel_stats(colors: &[u32]) -> WindowsResult<PixelStats> {
        if colors.is_empty() {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "pixel sampling returned no colours",
                None,
            ));
        }

        let mut luminance_sum = 0.0;
        let mut luminance_square_sum = 0.0;
        let mut saturation_sum = 0.0;
        let mut saturation_square_sum = 0.0;
        for color in colors {
            let red = f64::from(color & 0xff);
            let green = f64::from((color >> 8) & 0xff);
            let blue = f64::from((color >> 16) & 0xff);
            let luminance = (red * 299.0 + green * 587.0 + blue * 114.0) / 1_000.0;
            let maximum = red.max(green).max(blue);
            let minimum = red.min(green).min(blue);
            let saturation = if maximum == 0.0 {
                0.0
            } else {
                (maximum - minimum) * 255.0 / maximum
            };
            luminance_sum += luminance;
            luminance_square_sum += luminance * luminance;
            saturation_sum += saturation;
            saturation_square_sum += saturation * saturation;
        }
        let count = colors.len() as f64;
        let luminance_mean = luminance_sum / count;
        let saturation_mean = saturation_sum / count;
        Ok(PixelStats {
            samples: colors.len(),
            luminance_mean,
            luminance_variance: (luminance_square_sum / count - luminance_mean * luminance_mean)
                .max(0.0),
            saturation_mean,
            saturation_variance: (saturation_square_sum / count
                - saturation_mean * saturation_mean)
                .max(0.0),
        })
    }

    fn taskbar_pixel_stats() -> WindowsResult<PixelStats> {
        fn fail(operation: &'static str) -> WindowsError {
            WindowsError::new(WindowsErrorKind::ApiFailure, operation, None)
        }

        let taskbar = unsafe { FindWindowW(windows::core::w!("Shell_TrayWnd"), None) }
            .map_err(|_| fail("FindWindowW Shell_TrayWnd"))?;
        if !unsafe { IsWindowVisible(taskbar) }.as_bool() {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "taskbar is not visible",
                None,
            ));
        }
        let mut rect = RECT::default();
        unsafe { GetWindowRect(taskbar, &mut rect) }.map_err(|_| fail("GetWindowRect taskbar"))?;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width < 20 || height < 20 {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "taskbar is too small for colour sampling",
                None,
            ));
        }

        let screen = unsafe { GetDC(HWND::default()) };
        if screen.0.is_null() {
            return Err(fail("GetDC screen"));
        }
        let mut colors = Vec::with_capacity(200);
        for x_step in 0..40 {
            for y_step in 0..5 {
                let x = rect.left + width * (5 + x_step * 90 / 39) / 100;
                let y = rect.top + height * (15 + y_step * 70 / 4) / 100;
                let color = unsafe { GetPixel(screen, x, y) }.0;
                if color != CLR_INVALID {
                    colors.push(color);
                }
            }
        }
        unsafe {
            let _ = ReleaseDC(HWND::default(), screen);
        }
        pixel_stats(&colors)
    }

    fn explorer_window_luminance(window_handle: isize) -> WindowsResult<u32> {
        fn fail(operation: &'static str) -> WindowsError {
            WindowsError::new(WindowsErrorKind::ApiFailure, operation, None)
        }

        let window = HWND(window_handle as *mut core::ffi::c_void);
        let mut rect = RECT::default();
        unsafe { GetWindowRect(window, &mut rect) }.map_err(|_| fail("GetWindowRect Explorer"))?;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width < 400 || height < 300 {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "owned Explorer window is too small for colour sampling",
                None,
            ));
        }

        let source = unsafe { GetWindowDC(window) };
        if source.0.is_null() {
            return Err(fail("GetWindowDC Explorer"));
        }
        let memory = unsafe { CreateCompatibleDC(source) };
        if memory.0.is_null() {
            unsafe {
                let _ = ReleaseDC(window, source);
            }
            return Err(fail("CreateCompatibleDC Explorer"));
        }
        let bitmap = unsafe { CreateCompatibleBitmap(source, width, height) };
        if bitmap.0.is_null() {
            unsafe {
                let _ = DeleteDC(memory);
                let _ = ReleaseDC(window, source);
            }
            return Err(fail("CreateCompatibleBitmap Explorer"));
        }
        let previous = unsafe { SelectObject(memory, bitmap) };

        let result = (|| -> WindowsResult<u32> {
            // PW_RENDERFULLCONTENT=2 renders Explorer into our bitmap even when another
            // window is foreground or overlaps it.
            if !unsafe { PrintWindow(window, memory, 2) }.as_bool() {
                return Err(fail("PrintWindow Explorer"));
            }
            let mut total = 0u64;
            let mut samples = 0u64;
            let mut non_black = 0u64;
            for x_step in 0..8 {
                for y_step in 0..8 {
                    let x = width * (60 + x_step * 4) / 100;
                    let y = height * (58 + y_step * 4) / 100;
                    let color = unsafe { GetPixel(memory, x, y) }.0;
                    if color == CLR_INVALID {
                        continue;
                    }
                    let red = color & 0xff;
                    let green = (color >> 8) & 0xff;
                    let blue = (color >> 16) & 0xff;
                    non_black += u64::from(red != 0 || green != 0 || blue != 0);
                    total += u64::from((red * 299 + green * 587 + blue * 114) / 1_000);
                    samples += 1;
                }
            }
            if samples == 0 || non_black == 0 {
                return Err(WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "PrintWindow returned no usable Explorer pixels",
                    None,
                ));
            }
            Ok((total / samples) as u32)
        })();

        unsafe {
            let _ = SelectObject(memory, previous);
            let _ = DeleteObject(bitmap);
            let _ = DeleteDC(memory);
            let _ = ReleaseDC(window, source);
        }
        result
    }

    fn current_boolean_value(target: RegistryTarget) -> u32 {
        let state = read_registry_state(&target.location()).expect("read current probe value");
        if state.value_existed && state.value_type == Some(REG_DWORD) && state.raw_bytes.len() == 4
        {
            let value = u32::from_le_bytes(state.raw_bytes[..4].try_into().expect("DWORD bytes"));
            if value <= 1 {
                return value;
            }
        }
        0
    }

    fn prepare_boolean_probe(target: RegistryTarget, desired: u32) -> RegistryBackup {
        prepare_registry_backup(target, REG_DWORD, desired.to_le_bytes().to_vec(), 1, 26_200)
            .expect("prepare typed probe backup")
    }

    fn probe_folder(prefix: &str, item_names: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .expect("create unique probe directory");
        for name in item_names {
            std::fs::write(dir.path().join(name), b"probe").expect("create probe item");
        }
        dir
    }

    fn probe_title(dir: &tempfile::TempDir) -> String {
        dir.path()
            .file_name()
            .expect("probe directory name")
            .to_string_lossy()
            .into_owned()
    }

    struct SeparateProcessRestoreGuard {
        original: bool,
        restored: bool,
    }

    impl SeparateProcessRestoreGuard {
        fn new() -> WindowsResult<Self> {
            Ok(Self {
                original: shell_state_separate_process()?,
                restored: false,
            })
        }

        fn restore_and_assert(&mut self) {
            set_shell_state_separate_process(self.original)
                .expect("restore separate-process setting through documented API");
            assert_eq!(
                shell_state_separate_process().expect("read restored separate-process setting"),
                self.original,
                "separate-process setting must be restored exactly"
            );
            self.restored = true;
        }
    }

    impl Drop for SeparateProcessRestoreGuard {
        fn drop(&mut self) {
            if self.restored {
                return;
            }
            if let Err(error) = set_shell_state_separate_process(self.original) {
                eprintln!("emergency separate-process restoration failed: {error}");
            }
        }
    }

    struct InfoTipRestoreGuard {
        original: bool,
        restored: bool,
    }

    impl InfoTipRestoreGuard {
        fn new() -> WindowsResult<Self> {
            Ok(Self {
                original: shell_state_show_info_tip()?,
                restored: false,
            })
        }

        fn restore_and_assert(&mut self) {
            set_shell_state_show_info_tip(self.original)
                .expect("restore info-tip setting through documented API");
            assert_eq!(
                shell_state_show_info_tip().expect("read restored info-tip setting"),
                self.original,
                "info-tip setting must be restored exactly"
            );
            self.restored = true;
        }
    }

    impl Drop for InfoTipRestoreGuard {
        fn drop(&mut self) {
            if self.restored {
                return;
            }
            if let Err(error) = set_shell_state_show_info_tip(self.original) {
                eprintln!("emergency info-tip restoration failed: {error}");
            }
        }
    }

    struct CursorRestoreGuard {
        original: windows::Win32::Foundation::POINT,
        moved: bool,
    }

    impl CursorRestoreGuard {
        fn new() -> WindowsResult<Self> {
            use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

            let mut original = windows::Win32::Foundation::POINT::default();
            unsafe { GetCursorPos(&mut original) }.map_err(|_| {
                WindowsError::new(WindowsErrorKind::ApiFailure, "GetCursorPos", None)
            })?;
            Ok(Self {
                original,
                moved: false,
            })
        }

        fn move_to(&mut self, x: i32, y: i32) -> WindowsResult<()> {
            use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;

            unsafe { SetCursorPos(x, y) }.map_err(|_| {
                WindowsError::new(WindowsErrorKind::ApiFailure, "SetCursorPos", None)
            })?;
            self.moved = true;
            Ok(())
        }
    }

    impl Drop for CursorRestoreGuard {
        fn drop(&mut self) {
            use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;

            if !self.moved {
                return;
            }
            if unsafe { SetCursorPos(self.original.x, self.original.y) }.is_err() {
                eprintln!("emergency cursor restoration failed");
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct InfoTipUiObservation {
        readback: bool,
        samples: usize,
        samples_with_owned_or_near_tooltip: usize,
        maximum_visible_tooltips: i32,
        maximum_owned_process_tooltips: i32,
        maximum_near_item_tooltips: i32,
        names: Vec<String>,
        cursor_moved: bool,
        unavailable_reason: Option<String>,
    }

    fn rects_are_near(left: RECT, right: RECT) -> bool {
        let left_center_x = i64::from(left.left) + i64::from(left.right - left.left) / 2;
        let left_center_y = i64::from(left.top) + i64::from(left.bottom - left.top) / 2;
        let right_center_x = i64::from(right.left) + i64::from(right.right - right.left) / 2;
        let right_center_y = i64::from(right.top) + i64::from(right.bottom - right.top) / 2;
        let delta_x = left_center_x - right_center_x;
        let delta_y = left_center_y - right_center_y;
        delta_x * delta_x + delta_y * delta_y <= 800 * 800
    }

    fn visible_tooltip_counts(
        owned_process_id: u32,
        item_bounds: RECT,
    ) -> WindowsResult<(i32, i32, i32, Vec<String>)> {
        use windows::Win32::UI::Accessibility::{
            UIA_ControlTypePropertyId, UIA_ToolTipControlTypeId,
        };

        fn fail(operation: &'static str) -> WindowsError {
            WindowsError::new(WindowsErrorKind::ApiFailure, operation, None)
        }

        unsafe {
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let owns_com = init.is_ok();
            let result = (|| -> WindowsResult<(i32, i32, i32, Vec<String>)> {
                let automation: IUIAutomation =
                    CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                        .map_err(|_| fail("CoCreateInstance tooltip probe"))?;
                let root = automation
                    .GetRootElement()
                    .map_err(|_| fail("GetRootElement tooltip probe"))?;
                let condition = automation
                    .CreatePropertyCondition(
                        UIA_ControlTypePropertyId,
                        &windows::core::VARIANT::from(UIA_ToolTipControlTypeId.0),
                    )
                    .map_err(|_| fail("CreatePropertyCondition tooltip probe"))?;
                let tooltips = root
                    .FindAll(TreeScope_Descendants, &condition)
                    .map_err(|_| fail("FindAll tooltip probe"))?;
                let count = tooltips
                    .Length()
                    .map_err(|_| fail("Length tooltip probe"))?;
                let mut visible = 0;
                let mut owned = 0;
                let mut near = 0;
                let mut names = Vec::new();
                for index in 0..count {
                    let Ok(element) = tooltips.GetElement(index) else {
                        continue;
                    };
                    if element
                        .CurrentIsOffscreen()
                        .map(|value| value.as_bool())
                        .unwrap_or(true)
                    {
                        continue;
                    }
                    let Ok(bounds) = element.CurrentBoundingRectangle() else {
                        continue;
                    };
                    if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
                        continue;
                    }
                    visible += 1;
                    if element.CurrentProcessId().ok() == Some(owned_process_id as i32) {
                        owned += 1;
                    }
                    if rects_are_near(bounds, item_bounds) {
                        near += 1;
                    }
                    if let Ok(name) = element.CurrentName() {
                        let name = name.to_string();
                        if !name.trim().is_empty() && !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
                Ok((visible, owned, near, names))
            })();
            if owns_com {
                CoUninitialize();
            }
            result
        }
    }

    fn observe_owned_explorer_info_tip(
        show: bool,
        cursor_guard: &mut CursorRestoreGuard,
    ) -> WindowsResult<InfoTipUiObservation> {
        const ITEM_NAME: &str = "infotip-target-6b8e.txt";

        set_shell_state_show_info_tip(show)?;
        let readback = shell_state_show_info_tip()?;
        if readback != show {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "info-tip setting changed after readback",
                None,
            ));
        }

        let dir = probe_folder("pcc-infotip-probe-", &[ITEM_NAME]);
        std::fs::write(dir.path().join(ITEM_NAME), vec![b'x'; 12 * 1024])
            .map_err(|error| WindowsError::io("write info-tip probe file", &error))?;
        let title = probe_title(&dir);
        let window = OwnedExplorerWindow::open(dir.path(), &title)?;
        let item = explorer_item_observation(window.handle(), ITEM_NAME)?;
        let owned_process_id = window.process_id()?;
        let center_x = item.bounds.left + (item.bounds.right - item.bounds.left) / 2;
        let center_y = item.bounds.top + (item.bounds.bottom - item.bounds.top) / 2;

        let mut observation = InfoTipUiObservation {
            readback,
            samples: 0,
            samples_with_owned_or_near_tooltip: 0,
            maximum_visible_tooltips: 0,
            maximum_owned_process_tooltips: 0,
            maximum_near_item_tooltips: 0,
            names: Vec::new(),
            cursor_moved: false,
            unavailable_reason: None,
        };
        if let Err(error) = cursor_guard.move_to(center_x, center_y) {
            observation.unavailable_reason = Some(error.to_string());
            window.close_and_assert();
            return Ok(observation);
        }
        observation.cursor_moved = true;

        for _ in 0..40 {
            sleep(Duration::from_millis(200));
            let (visible, owned, near, names) =
                visible_tooltip_counts(owned_process_id, item.bounds)?;
            observation.samples += 1;
            observation.maximum_visible_tooltips =
                observation.maximum_visible_tooltips.max(visible);
            observation.maximum_owned_process_tooltips =
                observation.maximum_owned_process_tooltips.max(owned);
            observation.maximum_near_item_tooltips =
                observation.maximum_near_item_tooltips.max(near);
            if owned > 0 || near > 0 {
                observation.samples_with_owned_or_near_tooltip += 1;
            }
            for name in names {
                if !observation.names.contains(&name) {
                    observation.names.push(name);
                }
            }
        }

        window.close_and_assert();
        Ok(observation)
    }

    #[test]
    #[ignore = "文書化APIで設定を一時変更し、自己所有Explorerの説明ポップアップをUIAで測る"]
    fn info_tip_setting_changes_owned_explorer_tooltip_visibility() {
        let mut setting_guard = match InfoTipRestoreGuard::new() {
            Ok(guard) => guard,
            Err(error) => {
                println!("EVIDENCE: info_tip measurement unavailable before mutation: {error:?}");
                return;
            }
        };
        let mut cursor_guard = match CursorRestoreGuard::new() {
            Ok(guard) => guard,
            Err(error) => {
                println!("EVIDENCE: info_tip measurement unavailable before cursor move: original={} error={error:?}", setting_guard.original);
                return;
            }
        };
        let original = setting_guard.original;

        let off = match observe_owned_explorer_info_tip(false, &mut cursor_guard) {
            Ok(observation) => observation,
            Err(error) => {
                setting_guard.restore_and_assert();
                println!(
                    "EVIDENCE: info_tip measurement unavailable for off state: \
                     original={original} restored=true error={error:?}"
                );
                return;
            }
        };
        let on = match observe_owned_explorer_info_tip(true, &mut cursor_guard) {
            Ok(observation) => observation,
            Err(error) => {
                setting_guard.restore_and_assert();
                println!(
                    "EVIDENCE: info_tip measurement unavailable for on state: \
                     original={original} restored=true off={off:?} error={error:?}"
                );
                return;
            }
        };

        setting_guard.restore_and_assert();
        let off_detected = off.cursor_moved
            && (off.maximum_owned_process_tooltips > 0 || off.maximum_near_item_tooltips > 0);
        let on_detected = on.cursor_moved
            && (on.maximum_owned_process_tooltips > 0 || on.maximum_near_item_tooltips > 0);
        let outward_difference = off.samples > 0 && on.samples > 0 && !off_detected && on_detected;
        println!(
            "EVIDENCE: info_tip original={original} restored=true \
             off_readback={} off_cursor_moved={} off_samples={} off_samples_with_tooltip={} \
             off_max_visible={} off_max_owned={} off_max_near={} off_names={:?} \
             off_unavailable={:?} \
             on_readback={} on_cursor_moved={} on_samples={} on_samples_with_tooltip={} \
             on_max_visible={} on_max_owned={} on_max_near={} on_names={:?} \
             on_unavailable={:?} \
             outward_difference={outward_difference}",
            off.readback,
            off.cursor_moved,
            off.samples,
            off.samples_with_owned_or_near_tooltip,
            off.maximum_visible_tooltips,
            off.maximum_owned_process_tooltips,
            off.maximum_near_item_tooltips,
            off.names,
            off.unavailable_reason,
            on.readback,
            on.cursor_moved,
            on.samples,
            on.samples_with_owned_or_near_tooltip,
            on.maximum_visible_tooltips,
            on.maximum_owned_process_tooltips,
            on.maximum_near_item_tooltips,
            on.names,
            on.unavailable_reason,
        );
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct SeparateProcessPidObservation {
        readback: bool,
        shell_pid: u32,
        window_pids: [u32; 2],
        window_matches_shell: [bool; 2],
    }

    fn shell_process_id() -> WindowsResult<u32> {
        let shell_window = unsafe { GetShellWindow() };
        if shell_window.0.is_null() {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "GetShellWindow",
                None,
            ));
        }
        let mut process_id = 0;
        unsafe {
            GetWindowThreadProcessId(shell_window, Some(&mut process_id));
        }
        if process_id == 0 {
            return Err(WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "GetWindowThreadProcessId shell",
                None,
            ));
        }
        Ok(process_id)
    }

    fn observe_owned_explorer_processes(
        enabled: bool,
    ) -> WindowsResult<SeparateProcessPidObservation> {
        set_shell_state_separate_process(enabled)?;
        let readback = shell_state_separate_process()?;
        if readback != enabled {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "separate-process setting changed after readback",
                None,
            ));
        }

        let first_dir = probe_folder("pcc-separate-a-", &["owned-a.txt"]);
        let second_dir = probe_folder("pcc-separate-b-", &["owned-b.txt"]);
        let first_title = probe_title(&first_dir);
        let second_title = probe_title(&second_dir);
        let first = OwnedExplorerWindow::open(first_dir.path(), &first_title)?;
        let second = OwnedExplorerWindow::open(second_dir.path(), &second_title)?;

        let shell_pid = shell_process_id()?;
        let window_pids = [first.process_id()?, second.process_id()?];
        let window_matches_shell = [window_pids[0] == shell_pid, window_pids[1] == shell_pid];

        second.close_and_assert();
        first.close_and_assert();
        Ok(SeparateProcessPidObservation {
            readback,
            shell_pid,
            window_pids,
            window_matches_shell,
        })
    }

    #[test]
    #[ignore = "文書化APIで設定を一時変更し、自己所有のExplorer窓だけを開閉してPIDを測る"]
    fn separate_process_setting_changes_owned_explorer_window_process_pattern() {
        let mut guard = match SeparateProcessRestoreGuard::new() {
            Ok(guard) => guard,
            Err(error) => {
                println!(
                    "EVIDENCE: separate_process measurement unavailable before mutation: {error:?}"
                );
                return;
            }
        };
        let original = guard.original;

        let off = match observe_owned_explorer_processes(false) {
            Ok(observation) => observation,
            Err(error) => {
                guard.restore_and_assert();
                println!(
                    "EVIDENCE: separate_process measurement unavailable for off state: \
                     original={original} restored=true error={error:?}"
                );
                return;
            }
        };
        let on = match observe_owned_explorer_processes(true) {
            Ok(observation) => observation,
            Err(error) => {
                guard.restore_and_assert();
                println!(
                    "EVIDENCE: separate_process measurement unavailable for on state: \
                     original={original} restored=true \
                     off_readback={} off_shell_pid={} off_window_pids={:?} \
                     off_matches_shell={:?} error={error:?}",
                    off.readback, off.shell_pid, off.window_pids, off.window_matches_shell
                );
                return;
            }
        };

        guard.restore_and_assert();
        let expected_pattern =
            off.window_matches_shell == [true, true] && on.window_matches_shell == [false, false];
        println!(
            "EVIDENCE: separate_process original={original} restored=true \
             off_readback={} off_shell_pid={} off_window_pids={:?} off_matches_shell={:?} \
             on_readback={} on_shell_pid={} on_window_pids={:?} on_matches_shell={:?} \
             expected_pattern={expected_pattern}",
            off.readback,
            off.shell_pid,
            off.window_pids,
            off.window_matches_shell,
            on.readback,
            on.shell_pid,
            on.window_pids,
            on.window_matches_shell
        );
    }

    /// **この観測は判定に使えない**（記録として残す）。
    ///
    /// UIAが返す項目名は、拡張子の表示設定に**鈍感**で、常に正式なファイル名を返す。
    /// 同一の `HideFileExt=1` の下で、別プロセスのシェル表示名は「拡張子なし」を返したのに、
    /// Explorer UIA は「.txt 付き」を返した。信号が食い違う以上、この観測で
    /// 「反映されない」と結論づけてはならない。
    /// `show_extensions` の判定根拠は、別プロセスのシェル表示名のほうである。
    #[test]
    #[ignore = "記録用: UIA名は拡張子表示設定に鈍感なため判定に使わないこと"]
    fn show_extensions_write_changes_the_fresh_explorer_listing() {
        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        const STEM: &str = "ext-row-9b41";

        let dir = probe_folder("totonoe-ext-probe-", &["ext-row-9b41.txt"]);
        let title = probe_title(&dir);
        let with_ext = format!("{STEM}.txt");

        let observe = |handle: isize| -> Option<bool> {
            // 拡張子つきで見えるなら true、拡張子なしで見えるなら false、どちらでもなければ None。
            if explorer_item_observation(handle, &with_ext).is_ok() {
                Some(true)
            } else if explorer_item_observation(handle, STEM).is_ok() {
                Some(false)
            } else {
                None
            }
        };

        let before_window = match OwnedExplorerWindow::open(dir.path(), &title) {
            Ok(window) => window,
            Err(error) => {
                println!("OBSERVATION_UNAVAILABLE: owned Explorer window unavailable: {error:?}");
                return;
            }
        };
        let before = observe(before_window.handle());
        before_window.close_and_assert();
        let Some(before) = before else {
            println!("OBSERVATION_UNAVAILABLE: probe item was not observable in Explorer");
            return;
        };
        println!("before: extension_shown={before}");

        let target = RegistryTarget::current_user_64(SUBKEY, "HideFileExt");
        let original = current_boolean_value(target);
        // HideFileExt: 0 = 拡張子を表示 / 1 = 隠す
        let desired: u32 = if before { 1 } else { 0 };
        let mut guard = RegistryRestoreGuard::new(
            vec![prepare_boolean_probe(target, desired)],
            ProbeNotification::Explorer,
        );
        guard.apply();
        sleep(Duration::from_millis(500));

        let applied = match OwnedExplorerWindow::open(dir.path(), &title) {
            Ok(window) => {
                let seen = observe(window.handle());
                window.close_and_assert();
                seen
            }
            Err(error) => {
                println!("OBSERVATION_UNAVAILABLE: owned Explorer window unavailable after change: {error:?}");
                None
            }
        };
        guard.restore_and_assert();
        assert_eq!(
            current_boolean_value(target),
            original,
            "元の値へ正確に戻す"
        );

        match applied {
            Some(seen) => {
                println!("applied: extension_shown={seen} (desired HideFileExt={desired})");
                if seen != before {
                    println!(
                        "EVIDENCE: show_extensions changed what a fresh Explorer window lists"
                    );
                } else {
                    println!(
                        "EVIDENCE: show_extensions did not change a fresh Explorer window listing"
                    );
                }
            }
            None => println!("EVIDENCE: inconclusive"),
        }
    }

    /// 最後の未測定Action。既存窓では反映されないと既に分かっているので、
    /// **新しく開いた窓**に隠しファイルが現れるかで判定する。
    #[test]
    #[ignore = "temporarily changes hidden-file visibility and opens only an owned Explorer window"]
    fn show_hidden_write_changes_the_fresh_explorer_listing() {
        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        const VISIBLE: &str = "plain-row-5c2e";
        const SECRET: &str = "secret-row-5c2e";

        let dir = probe_folder("totonoe-hidden-probe-", &[VISIBLE, SECRET]);
        let title = probe_title(&dir);
        // 片方だけ隠し属性にする。属性が付かなければ判定できないので確認する。
        let secret_path = dir.path().join(SECRET);
        let _ = std::process::Command::new("attrib")
            .arg("+h")
            .arg(&secret_path)
            .output();
        let hidden_attribute_set = {
            use std::os::windows::fs::MetadataExt;
            std::fs::metadata(&secret_path)
                .map(|m| m.file_attributes() & 2 != 0)
                .unwrap_or(false)
        };
        if !hidden_attribute_set {
            println!("OBSERVATION_UNAVAILABLE: could not mark the probe item hidden");
            return;
        }

        let sees_secret = |handle: isize| explorer_item_observation(handle, SECRET).is_ok();

        let before_window = match OwnedExplorerWindow::open(dir.path(), &title) {
            Ok(window) => window,
            Err(error) => {
                println!("OBSERVATION_UNAVAILABLE: owned Explorer window unavailable: {error:?}");
                return;
            }
        };
        // 対照として、隠しでない項目は必ず見えるはず。見えないなら観測自体が壊れている。
        let control_visible = explorer_item_observation(before_window.handle(), VISIBLE).is_ok();
        let before = sees_secret(before_window.handle());
        before_window.close_and_assert();
        println!("before: control_item_visible={control_visible} secret_visible={before}");
        if !control_visible {
            println!("OBSERVATION_UNAVAILABLE: control item was not observable");
            return;
        }

        let target = RegistryTarget::current_user_64(SUBKEY, "Hidden");
        let original = current_boolean_value(target);
        // 1 = 隠しファイルを表示 / 2 = 表示しない
        let desired: u32 = if before { 2 } else { 1 };
        let mut guard = RegistryRestoreGuard::new(
            vec![prepare_boolean_probe(target, desired)],
            ProbeNotification::Explorer,
        );
        guard.apply();
        sleep(Duration::from_millis(500));

        let applied = match OwnedExplorerWindow::open(dir.path(), &title) {
            Ok(window) => {
                let seen = sees_secret(window.handle());
                window.close_and_assert();
                Some(seen)
            }
            Err(error) => {
                println!("OBSERVATION_UNAVAILABLE: owned Explorer window unavailable after change: {error:?}");
                None
            }
        };
        guard.restore_and_assert();

        let restored = current_boolean_value(target);
        assert_eq!(restored, original, "元の値へ正確に戻す");

        match applied {
            Some(seen) => {
                println!("applied: secret_visible={seen} (desired={desired})");
                if seen != before {
                    println!("EVIDENCE: show_hidden changed what a fresh Explorer window lists");
                } else {
                    println!(
                        "EVIDENCE: show_hidden did not change a fresh Explorer window listing"
                    );
                }
            }
            None => println!("EVIDENCE: inconclusive"),
        }
    }

    #[test]
    #[ignore = "temporarily changes item checkboxes and opens only an owned Explorer window"]
    fn explorer_item_checkboxes_write_changes_the_fresh_explorer_ui() {
        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        const ITEMS: &[&str] = &[
            "checkbox-row-a-7f3a",
            "checkbox-row-b-7f3a",
            "checkbox-row-c-7f3a",
            "checkbox-row-d-7f3a",
        ];
        let target = RegistryTarget::current_user_64(SUBKEY, "AutoCheckSelect");
        let dir = probe_folder("totonoe-checkbox-probe-", ITEMS);
        let title = probe_title(&dir);

        let before_window = match OwnedExplorerWindow::open(dir.path(), &title) {
            Ok(window) => window,
            Err(error) => {
                println!("OBSERVATION_UNAVAILABLE: owned Explorer window unavailable: {error:?}");
                return;
            }
        };
        let before = match explorer_item_lefts(before_window.handle(), ITEMS) {
            Ok(lefts) => lefts,
            Err(error) => {
                before_window.close_and_assert();
                println!(
                    "OBSERVATION_UNAVAILABLE: Explorer item bounds unavailable before change: {error:?}"
                );
                return;
            }
        };
        before_window.close_and_assert();
        println!("before: bounds_left={before:?}");

        let original = current_boolean_value(target);
        let desired = u32::from(original == 0);
        let mut guard = RegistryRestoreGuard::new(
            vec![prepare_boolean_probe(target, desired)],
            ProbeNotification::Explorer,
        );
        guard.apply();
        sleep(Duration::from_millis(500));

        let applied_window = match OwnedExplorerWindow::open(dir.path(), &title) {
            Ok(window) => window,
            Err(error) => {
                guard.restore_and_assert();
                println!(
                    "OBSERVATION_UNAVAILABLE: owned Explorer unavailable after checkbox change: {error:?}"
                );
                return;
            }
        };
        let applied = match explorer_item_lefts(applied_window.handle(), ITEMS) {
            Ok(lefts) => lefts,
            Err(error) => {
                applied_window.close_and_assert();
                guard.restore_and_assert();
                println!(
                    "OBSERVATION_UNAVAILABLE: Explorer item bounds unavailable after change: {error:?}"
                );
                return;
            }
        };
        applied_window.close_and_assert();
        let deltas: Vec<i32> = applied
            .iter()
            .zip(&before)
            .map(|(applied, before)| applied - before)
            .collect();
        println!("applied: bounds_left={applied:?} delta={deltas:?}");

        guard.restore_and_assert();
        sleep(Duration::from_millis(500));
        let restored_window = match OwnedExplorerWindow::open(dir.path(), &title) {
            Ok(window) => window,
            Err(error) => {
                println!(
                    "OBSERVATION_UNAVAILABLE: owned Explorer unavailable after checkbox restore: {error:?}"
                );
                return;
            }
        };
        let restored = match explorer_item_lefts(restored_window.handle(), ITEMS) {
            Ok(lefts) => lefts,
            Err(error) => {
                restored_window.close_and_assert();
                println!(
                    "OBSERVATION_UNAVAILABLE: Explorer item bounds unavailable after restore: {error:?}"
                );
                return;
            }
        };
        restored_window.close_and_assert();
        assert_eq!(
            restored, before,
            "restored Explorer item left edges must equal baseline"
        );

        if deltas.iter().any(|delta| *delta != 0) {
            println!("EVIDENCE: item checkbox setting shifted Explorer item left edges");
        } else {
            println!("EVIDENCE: item checkbox setting did not shift Explorer item left edges");
        }
    }

    #[test]
    #[ignore = "temporarily changes compact view and opens only an owned Explorer window"]
    fn explorer_compact_view_write_changes_the_fresh_explorer_row_spacing() {
        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        const ITEMS: &[&str] = &[
            "compact-row-a-71c2",
            "compact-row-b-71c2",
            "compact-row-c-71c2",
            "compact-row-d-71c2",
        ];
        let target = RegistryTarget::current_user_64(SUBKEY, "UseCompactMode");
        let dir = probe_folder("totonoe-compact-probe-", ITEMS);
        let title = probe_title(&dir);

        let before_window = match OwnedExplorerWindow::open(dir.path(), &title) {
            Ok(window) => window,
            Err(error) => {
                println!("OBSERVATION_UNAVAILABLE: owned Explorer window unavailable: {error:?}");
                return;
            }
        };
        let before = explorer_row_layout(before_window.handle(), ITEMS)
            .expect("observe measurable Explorer row spacing");
        before_window.close_and_assert();
        println!("before: {before:?}");

        let original = current_boolean_value(target);
        let desired = u32::from(original == 0);
        let mut guard = RegistryRestoreGuard::new(
            vec![prepare_boolean_probe(target, desired)],
            ProbeNotification::Explorer,
        );
        guard.apply();
        sleep(Duration::from_millis(500));

        let applied_window = OwnedExplorerWindow::open(dir.path(), &title)
            .expect("open owned Explorer after compact change");
        let applied = explorer_row_layout(applied_window.handle(), ITEMS)
            .expect("observe applied Explorer row spacing");
        applied_window.close_and_assert();
        println!("applied: {applied:?}");

        guard.restore_and_assert();
        sleep(Duration::from_millis(500));
        let restored_window = OwnedExplorerWindow::open(dir.path(), &title)
            .expect("open owned Explorer after compact restore");
        let restored = explorer_row_layout(restored_window.handle(), ITEMS)
            .expect("observe restored Explorer row spacing");
        restored_window.close_and_assert();
        assert_eq!(
            restored, before,
            "restored Explorer row spacing must equal baseline"
        );

        if applied.list_height != before.list_height {
            println!(
                "EVIDENCE: compact view changed four-item list height in a fresh Explorer window"
            );
        } else {
            println!("EVIDENCE: compact view did not change measurable four-item list height");
        }
    }

    #[test]
    #[ignore = "temporarily changes transparency and samples taskbar pixels"]
    fn appearance_transparency_write_changes_taskbar_pixel_variance() {
        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
        let target = RegistryTarget::current_user_64(SUBKEY, "EnableTransparency");
        let before = match taskbar_pixel_stats() {
            Ok(observation) => observation,
            Err(error) => {
                println!("OBSERVATION_UNAVAILABLE: taskbar pixels unavailable: {error:?}");
                return;
            }
        };
        let original = current_boolean_value(target);
        let desired = u32::from(original == 0);
        println!(
            "before: samples={} luminance_mean={:.2} luminance_variance={:.2} saturation_mean={:.2} saturation_variance={:.2} desired_transparency={desired}",
            before.samples,
            before.luminance_mean,
            before.luminance_variance,
            before.saturation_mean,
            before.saturation_variance,
        );

        let mut guard = RegistryRestoreGuard::new(
            vec![prepare_boolean_probe(target, desired)],
            ProbeNotification::Theme,
        );
        guard.apply();
        sleep(Duration::from_millis(1_500));
        let applied = match taskbar_pixel_stats() {
            Ok(observation) => observation,
            Err(error) => {
                guard.restore_and_assert();
                println!(
                    "OBSERVATION_UNAVAILABLE: taskbar pixels unavailable after change: {error:?}"
                );
                return;
            }
        };
        println!(
            "applied: samples={} luminance_mean={:.2} luminance_variance={:.2} saturation_mean={:.2} saturation_variance={:.2} luminance_variance_delta={:.2} saturation_variance_delta={:.2}",
            applied.samples,
            applied.luminance_mean,
            applied.luminance_variance,
            applied.saturation_mean,
            applied.saturation_variance,
            applied.luminance_variance - before.luminance_variance,
            applied.saturation_variance - before.saturation_variance,
        );

        guard.restore_and_assert();
        sleep(Duration::from_millis(1_500));
        match taskbar_pixel_stats() {
            Ok(restored) => {
                println!(
                    "restored: samples={} luminance_mean={:.2} luminance_variance={:.2} saturation_mean={:.2} saturation_variance={:.2}",
                    restored.samples,
                    restored.luminance_mean,
                    restored.luminance_variance,
                    restored.saturation_mean,
                    restored.saturation_variance,
                );
            }
            Err(error) => {
                println!(
                    "OBSERVATION_UNAVAILABLE: taskbar pixels unavailable after restore: {error:?}"
                );
                return;
            }
        }
        println!(
            "EVIDENCE: taskbar pixel statistics captured before and after transparency change"
        );
    }

    #[test]
    #[ignore = "temporarily changes color mode and samples only an owned Explorer window"]
    fn theme_color_mode_write_changes_a_fresh_explorer_window_luminance() {
        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
        let apps = RegistryTarget::current_user_64(SUBKEY, "AppsUseLightTheme");
        let system = RegistryTarget::current_user_64(SUBKEY, "SystemUsesLightTheme");
        let dir = probe_folder("totonoe-theme-probe-", &["theme-sample-82d1"]);
        let title = probe_title(&dir);

        let before_window = match OwnedExplorerWindow::open(dir.path(), &title) {
            Ok(window) => window,
            Err(error) => {
                println!("OBSERVATION_UNAVAILABLE: owned Explorer window unavailable: {error:?}");
                return;
            }
        };
        // 他3件と同じく、観測できない環境では失敗ではなく安全終了にする。
        // expect にすると、環境要因がテスト失敗として残り、本当の退行を隠してしまう。
        let before = match explorer_window_luminance(before_window.handle()) {
            Ok(value) => value,
            Err(error) => {
                println!("OBSERVATION_UNAVAILABLE: Explorer luminance unavailable: {error:?}");
                return;
            }
        };
        before_window.close_and_assert();
        let desired = u32::from(before < 128);
        println!("before: luminance={before} desired_light={desired}");

        let mut guard = RegistryRestoreGuard::new(
            vec![
                prepare_boolean_probe(apps, desired),
                prepare_boolean_probe(system, desired),
            ],
            ProbeNotification::Theme,
        );
        guard.apply();
        sleep(Duration::from_millis(700));

        let applied_window = match OwnedExplorerWindow::open(dir.path(), &title) {
            Ok(window) => window,
            Err(error) => {
                guard.restore_and_assert();
                println!(
                    "OBSERVATION_UNAVAILABLE: owned Explorer unavailable after theme change: {error:?}"
                );
                return;
            }
        };
        let applied = match explorer_window_luminance(applied_window.handle()) {
            Ok(value) => value,
            Err(error) => {
                applied_window.close_and_assert();
                guard.restore_and_assert();
                println!(
                    "OBSERVATION_UNAVAILABLE: Explorer bitmap luminance unavailable after change: {error:?}"
                );
                return;
            }
        };
        applied_window.close_and_assert();
        println!("applied: luminance={applied}");

        guard.restore_and_assert();
        sleep(Duration::from_millis(700));
        let restored_window = match OwnedExplorerWindow::open(dir.path(), &title) {
            Ok(window) => window,
            Err(error) => {
                println!(
                    "OBSERVATION_UNAVAILABLE: owned Explorer unavailable after theme restore: {error:?}"
                );
                return;
            }
        };
        let restored = match explorer_window_luminance(restored_window.handle()) {
            Ok(value) => value,
            Err(error) => {
                restored_window.close_and_assert();
                println!(
                    "OBSERVATION_UNAVAILABLE: Explorer bitmap luminance unavailable after restore: {error:?}"
                );
                return;
            }
        };
        restored_window.close_and_assert();
        assert!(
            restored.abs_diff(before) <= 25,
            "restored Explorer luminance must return near baseline: {before} -> {restored}"
        );

        if applied.abs_diff(before) >= 60 {
            println!("EVIDENCE: fresh Explorer luminance changed from {before} to {applied}");
        } else {
            println!(
                "EVIDENCE: fresh Explorer luminance stayed near baseline: {before} -> {applied}"
            );
        }
    }

    /// この環境でWindowsの実UIを機械的に観測できるかの調査。
    /// 環境によって取得できないことがあるため、失敗しても診断を出して落とさない。
    #[test]
    #[ignore = "実機のUI構造に依存する調査用"]
    fn taskbar_layout_can_be_observed_without_pixels() {
        match observe_taskbar_layout() {
            Ok(observation) => {
                println!(
                    "taskbar_left={} width={} start_left={} start_width={} center_ratio={:.3}",
                    observation.taskbar_left,
                    observation.taskbar_width,
                    observation.start_button_left,
                    observation.start_button_width,
                    observation.start_center_ratio()
                );
                assert!(observation.taskbar_width > 0);
                assert!(observation.start_button_width > 0);
            }
            Err(error) => {
                println!("観測できず: {error:?}");
            }
        }
    }

    /// 42候補を解禁するために必要な「第三者アプリの書き込みがWindows UIへ反映される」証拠を、
    /// 目視ではなくUI Automationの実測で取る。
    ///
    /// taskbar.alignment（HKCU Advanced\TaskbarAl）を対象に、
    /// 元値を型付きで退避 → 反対の値へ変更 → スタートボタンの位置が実際に動いたか実測 →
    /// 元の値・型・有無へ正確に復元 → 位置が戻ったか実測、までを1往復で確認する。
    ///
    /// ユーザーの実環境を一時的に変更するため、通常のテスト実行では走らせない。
    /// オーバーレイは「置けた」だけでは足りない。**留まり続けるか**を測る。
    ///
    /// いちばん危ないのは、このアプリ自身が持つシェル再起動との相互作用である。
    /// シェルを作り直すとタスクバーの HWND ごと変わる。そのため、古い HWND を握ったままの
    /// オーバーレイが位置を見失うか、新しいタスクバーが自分より手前に来るかのどちらかが
    /// 起きうる。単一ファイルのレビューでは見つからない類の問題。
    ///
    /// **実機のタスクバー上に帯が出たまま、シェルが1回再起動される。**
    #[cfg(windows)]
    #[test]
    #[ignore = "オーバーレイを出したままシェルを再起動し、前面維持を測る"]
    fn overlay_survives_a_shell_restart() {
        use std::{thread::sleep, time::Duration};
        use windows::{
            core::{w, PCWSTR},
            Win32::{
                Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
                Graphics::Gdi::HBRUSH,
                UI::WindowsAndMessaging::{
                    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW,
                    GetTopWindow, GetWindow, GetWindowRect, IsWindow, PeekMessageW, RegisterClassW,
                    SetLayeredWindowAttributes, SetWindowPos, ShowWindow, GW_HWNDNEXT,
                    HWND_TOPMOST, LWA_ALPHA, MSG, PM_REMOVE, SWP_NOACTIVATE, SW_SHOWNOACTIVATE,
                    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
                    WS_EX_TRANSPARENT, WS_POPUP,
                },
            },
        };

        unsafe extern "system" fn wnd_proc(
            window: HWND,
            message: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            DefWindowProcW(window, message, wparam, lparam)
        }

        fn taskbar_now() -> Option<HWND> {
            match unsafe { FindWindowW(w!("Shell_TrayWnd"), None) } {
                Ok(handle) if !handle.is_invalid() => Some(handle),
                _ => None,
            }
        }
        fn z_index(target: HWND) -> Option<usize> {
            let mut current = unsafe { GetTopWindow(None) }.ok()?;
            let mut index = 0usize;
            loop {
                if current == target {
                    return Some(index);
                }
                match unsafe { GetWindow(current, GW_HWNDNEXT) } {
                    Ok(next) if !next.is_invalid() => {
                        current = next;
                        index += 1;
                        if index > 5000 {
                            return None;
                        }
                    }
                    _ => return None,
                }
            }
        }
        fn pump(rounds: usize) {
            for _ in 0..rounds {
                let mut message = MSG::default();
                while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                    unsafe { DispatchMessageW(&message) };
                }
                sleep(Duration::from_millis(100));
            }
        }

        let Some(taskbar_before) = taskbar_now() else {
            println!("タスクバーが無いためスキップ");
            return;
        };
        let mut bar = RECT::default();
        if unsafe { GetWindowRect(taskbar_before, &mut bar) }.is_err() {
            println!("矩形を取れないためスキップ");
            return;
        }

        let class_name = w!("PcCustomOverlayPersist");
        let class = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            ..Default::default()
        };
        unsafe { RegisterClassW(&class) };
        let width = bar.right - bar.left;
        let height = (bar.bottom - bar.top).min(48);
        let overlay = match unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST
                    | WS_EX_NOACTIVATE,
                class_name,
                w!("pc-custom overlay persist"),
                WS_POPUP,
                bar.left,
                bar.top,
                width,
                height,
                None,
                None,
                None,
                None,
            )
        } {
            Ok(handle) if !handle.is_invalid() => handle,
            other => {
                println!("作成できなかった: {other:?}");
                return;
            }
        };
        struct Owned(HWND);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = unsafe { DestroyWindow(self.0) };
            }
        }
        let _owned = Owned(overlay);

        let _ = unsafe { SetLayeredWindowAttributes(overlay, COLORREF(0), 140, LWA_ALPHA) };
        let _ = unsafe { ShowWindow(overlay, SW_SHOWNOACTIVATE) };
        let _ = unsafe {
            SetWindowPos(
                overlay,
                HWND_TOPMOST,
                bar.left,
                bar.top,
                width,
                height,
                SWP_NOACTIVATE,
            )
        };
        pump(10);
        println!(
            "再起動前: overlay_z={:?} taskbar_z={:?}",
            z_index(overlay),
            z_index(taskbar_before)
        );

        // ここでシェルを作り直す。タスクバーの HWND は変わるはず。
        let outcome = crate::windows::restart_shell().expect("restart shell");
        println!("restart: {outcome:?}");
        pump(30);

        let alive = unsafe { IsWindow(overlay) }.as_bool();
        let taskbar_after = taskbar_now();
        let handle_changed = match taskbar_after {
            Some(after) => after != taskbar_before,
            None => false,
        };
        println!(
            "再起動後: overlay生存={alive} タスクバーHWND変化={handle_changed} overlay_z={:?} taskbar_z={:?}",
            z_index(overlay),
            taskbar_after.and_then(z_index)
        );

        match (z_index(overlay), taskbar_after.and_then(z_index)) {
            (Some(o), Some(t)) if o < t => {
                println!("EVIDENCE: シェル再起動後もオーバーレイは手前を保った");
            }
            (Some(o), Some(t)) => {
                println!("EVIDENCE: シェル再起動でオーバーレイが奥へ落ちた (overlay={o} taskbar={t})。前面を取り直す仕組みが要る");
            }
            other => println!("EVIDENCE: 再起動後にZ順を判定できなかった {other:?}"),
        }
        assert!(alive, "オーバーレイ自体はシェル再起動で消えないこと");
    }

    /// エクスプローラー系の候補を、**新しく開いた自分の窓**で一括判定する。
    ///
    /// エクスプローラーの表示設定は、既に開いている窓には効かない。
    /// 設定を変えたあとに新しい窓を開いて読む必要がある。
    /// （同一プロセス内のキャッシュで誤判定した過去があるため、必ず新しい窓で見る）
    #[cfg(windows)]
    #[test]
    #[ignore = "設定を変えて自分の窓を開き、戻す。既存の窓には触れない"]
    fn batch_measure_explorer_candidates() {
        use crate::backup::{prepare_registry_backup, restore_registry_backup, RegistryTarget};
        use crate::windows::write_raw_value;
        use std::fs;

        const REG_DWORD: u32 = 4;
        const ADVANCED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";

        struct Candidate {
            // 判定できるようになったら使う。いまは観測手段が無く、id は記録のためだけに持つ。
            #[allow(dead_code)]
            id: &'static str,
            value_name: &'static str,
            flipped: u32,
        }
        let candidates = [
            Candidate {
                id: "explorer.status_bar",
                value_name: "ShowStatusBar",
                flipped: 0,
            },
            Candidate {
                id: "explorer.always_show_menus",
                value_name: "AlwaysShowMenus",
                flipped: 1,
            },
        ];

        /// 一覧の矩形を返す。「要素が在るか」では判定できなかったので、**面積**で見る。
        /// ステータスバーやメニューバーが出れば、その分だけ一覧は縮む。
        fn read_list_rect(label: &str) -> Option<(i32, i32, i32, i32)> {
            let Ok(dir) = tempfile::tempdir() else {
                return None;
            };
            let title = format!("pcc-x-{}", uuid::Uuid::new_v4().simple());
            let target = dir.path().join(&title);
            if fs::create_dir(&target).is_err() {
                return None;
            }
            let _ = fs::write(target.join("a.txt"), b"a");
            match OwnedExplorerWindow::open(&target, &title) {
                Ok(window) => {
                    let rect = window
                        .handle
                        .and_then(|h| explorer_element_rect(h, "個の項目").ok().flatten());
                    println!("  ({label}) 一覧の矩形={rect:?}");
                    rect
                }
                Err(error) => {
                    println!("  ({label}) 窓を開けなかった: {error:?}");
                    None
                }
            }
        }

        let before = read_list_rect("変更前");
        let Some(before_rect) = before else {
            println!("一覧の矩形を読めないためスキップ");
            return;
        };

        let mut backups = Vec::new();
        for candidate in &candidates {
            let target = RegistryTarget::current_user_64(ADVANCED, candidate.value_name);
            match prepare_registry_backup(
                target,
                REG_DWORD,
                candidate.flipped.to_le_bytes().to_vec(),
                1,
                26_200,
            ) {
                Ok(backup) => backups.push((candidate, backup)),
                Err(error) => println!("{}: backup不可 {error:?}", candidate.id),
            }
        }
        struct Guard(Vec<crate::backup::RegistryBackup>, bool);
        impl Drop for Guard {
            fn drop(&mut self) {
                if !self.1 {
                    for b in &self.0 {
                        let _ = restore_registry_backup(b);
                    }
                }
            }
        }
        let mut guard = Guard(backups.iter().map(|(_, b)| b.clone()).collect(), false);

        for (candidate, backup) in &backups {
            let _ = write_raw_value(
                &backup.location,
                REG_DWORD,
                &candidate.flipped.to_le_bytes(),
            );
        }
        let after = read_list_rect("変更後");

        for (_, backup) in &backups {
            let _ = restore_registry_backup(backup);
        }
        guard.1 = true;
        drop(guard);
        let restored = read_list_rect("復元後");

        let height = |r: Option<(i32, i32, i32, i32)>| r.map(|v| v.3 - v.1);
        println!(
            "一覧の高さ: 前={:?} 後={:?} 戻し後={:?}",
            height(before),
            height(after),
            height(restored)
        );
        let changed = match (height(before), height(after)) {
            (Some(a), Some(b)) => (a - b).abs() > 4,
            _ => false,
        };
        let _ = before_rect;

        println!("---- 判定 ----");
        // 3回とも観測に失敗している。ここで出る「変化なし」は
        // **「反映されない」ではなく「測れていない」** である。結論を書かない。
        //   1回目: 要素名の有無 → UIA は非表示のものも列挙するので区別できない
        //   2回目: CurrentIsOffscreen と境界矩形で絞る → 要素数が変わらず、効いていない
        //   3回目: 一覧の高さ → 掴んだのは幅299pxの詳細ウィンドウで、一覧ではなかった
        // 必要なのは、一覧そのものを取り違えずに掴む手段。
        println!("この経路では判定できない（変化={changed}）。観測手段を作り直すこと");
    }

    /// 自分で開いたエクスプローラーの窓に、UIA から何が見えるかを出す。
    /// エクスプローラー系の候補で、何を判定材料に使えるかの下調べ。
    #[cfg(windows)]
    #[test]
    #[ignore = "自分の窓を1枚開いて要素名を出す。設定は変更しない"]
    fn dump_owned_explorer_element_names() {
        use std::fs;
        let dir = match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(_) => {
                println!("作業フォルダーを作れないためスキップ");
                return;
            }
        };
        let _ = fs::write(dir.path().join("sample-note.txt"), b"probe");
        let title = format!("pcc-dump-{}", uuid::Uuid::new_v4().simple());
        let target = dir.path().join(&title);
        if fs::create_dir(&target).is_err() {
            println!("フォルダーを作れないためスキップ");
            return;
        }
        let _ = fs::write(target.join("a.txt"), b"a");
        let window = match OwnedExplorerWindow::open(&target, &title) {
            Ok(window) => window,
            Err(error) => {
                println!("窓を開けないためスキップ: {error:?}");
                return;
            }
        };
        match window
            .handle
            .and_then(|h| explorer_window_item_names(h).ok())
        {
            Some(names) => {
                println!("要素数={}", names.len());
                for name in names.iter().take(60) {
                    println!("  [{name}]");
                }
            }
            None => println!("要素を読めなかった"),
        }
    }

    /// オーバーレイが**他の最前面ウィンドウに前へ出られたとき**どうなるか。
    ///
    /// 実運用では、別の topmost ウィンドウ（通知、ゲームのオーバーレイ、他のツール）が
    /// いつでも前に出てくる。出られたまま黙っていると、絵が半分隠れた状態が続く。
    /// ここでは、前へ出られたことを**検出できるか**と、
    /// `SetWindowPos(HWND_TOPMOST)` で**取り返せるか**を測る。
    ///
    /// 自分で作った窓しか使わない。利用者の窓には触れない。
    #[cfg(windows)]
    #[test]
    #[ignore = "自作の窓2枚でZ順の奪い合いを測る"]
    fn overlay_can_retake_the_front_after_another_topmost_window() {
        use std::{thread::sleep, time::Duration};
        use windows::{
            core::{w, PCWSTR},
            Win32::{
                Foundation::{HWND, LPARAM, LRESULT, WPARAM},
                Graphics::Gdi::HBRUSH,
                UI::WindowsAndMessaging::{
                    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetTopWindow,
                    GetWindow, PeekMessageW, RegisterClassW, SetWindowPos, ShowWindow, GW_HWNDNEXT,
                    HWND_TOPMOST, MSG, PM_REMOVE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                    SW_SHOWNOACTIVATE, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
                    WS_EX_TOPMOST, WS_POPUP,
                },
            },
        };

        unsafe extern "system" fn proc_fn(
            window: HWND,
            message: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            DefWindowProcW(window, message, wparam, lparam)
        }
        fn make(class: PCWSTR, title: PCWSTR) -> Option<HWND> {
            let registration = WNDCLASSW {
                lpfnWndProc: Some(proc_fn),
                lpszClassName: class,
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            unsafe { RegisterClassW(&registration) };
            match unsafe {
                CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    class,
                    title,
                    WS_POPUP,
                    0,
                    0,
                    240,
                    60,
                    None,
                    None,
                    None,
                    None,
                )
            } {
                Ok(handle) if !handle.is_invalid() => {
                    let _ = unsafe { ShowWindow(handle, SW_SHOWNOACTIVATE) };
                    Some(handle)
                }
                _ => None,
            }
        }
        fn z(target: HWND) -> Option<usize> {
            let mut current = unsafe { GetTopWindow(None) }.ok()?;
            let mut index = 0usize;
            loop {
                if current == target {
                    return Some(index);
                }
                match unsafe { GetWindow(current, GW_HWNDNEXT) } {
                    Ok(next) if !next.is_invalid() => {
                        current = next;
                        index += 1;
                        if index > 5000 {
                            return None;
                        }
                    }
                    _ => return None,
                }
            }
        }
        fn pump() {
            for _ in 0..6 {
                let mut message = MSG::default();
                while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                    unsafe { DispatchMessageW(&message) };
                }
                sleep(Duration::from_millis(120));
            }
        }

        let Some(overlay) = make(w!("PcCustomZOverlay"), w!("overlay")) else {
            println!("窓を作れないためスキップ");
            return;
        };
        struct Owned(HWND);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = unsafe { DestroyWindow(self.0) };
            }
        }
        let _own_overlay = Owned(overlay);
        pump();
        let before = z(overlay);

        // あとから作った別の topmost が前に出るはず。
        let Some(rival) = make(w!("PcCustomZRival"), w!("rival")) else {
            println!("相手の窓を作れないためスキップ");
            return;
        };
        let _own_rival = Owned(rival);
        pump();
        let (overlay_z, rival_z) = (z(overlay), z(rival));
        println!("割り込み後: overlay={overlay_z:?} rival={rival_z:?}");
        let taken = match (overlay_z, rival_z) {
            (Some(o), Some(r)) => r < o,
            _ => false,
        };
        println!("前を取られた: {taken}");

        // 取り返す。
        let _ = unsafe {
            SetWindowPos(
                overlay,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            )
        };
        pump();
        let (after_o, after_r) = (z(overlay), z(rival));
        println!("取り返し後: overlay={after_o:?} rival={after_r:?}");

        match (before, after_o, after_r) {
            (_, Some(o), Some(r)) if o < r => {
                println!("EVIDENCE: 割り込まれても SetWindowPos(HWND_TOPMOST) で前を取り返せる");
            }
            _ => {
                println!("EVIDENCE: 取り返せなかった。前面維持には別の手立てが要る");
            }
        }
    }

    /// Phase 1 A の決定実験。
    ///
    /// 「タスクバーを着せ替えたように見せる」を Safe 方式（Explorer へ手を入れず、
    /// 別ウィンドウを重ねるだけ）で成立させられるかは、次の一点にかかっている。
    ///
    ///   **自分のウィンドウを Shell_TrayWnd より手前の Z 順に置けるか。**
    ///
    /// タスクバーは topmost である。ここが偽なら、どれだけ絵を作り込んでも
    /// タスクバーの下に隠れるだけで、企画そのものを見直す必要がある。
    ///
    /// 目視では判定しない。Z 順を `GetTopWindow` / `GetWindow` で辿って
    /// 両者の順位を数えて比べる。
    ///
    /// **実機のタスクバー上に、半透明の帯が数秒表示される。クリックは透過する。**
    #[cfg(windows)]
    #[test]
    #[ignore = "実機のタスクバー上に一時的にオーバーレイを出してZ順を測る"]
    fn overlay_window_can_sit_above_the_taskbar() {
        use std::{thread::sleep, time::Duration};
        use windows::{
            core::{w, PCWSTR},
            Win32::{
                Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
                Graphics::Gdi::HBRUSH,
                UI::WindowsAndMessaging::{
                    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW,
                    GetTopWindow, GetWindow, GetWindowRect, PeekMessageW, RegisterClassW,
                    SetLayeredWindowAttributes, SetWindowPos, ShowWindow, GW_HWNDNEXT,
                    HWND_TOPMOST, LWA_ALPHA, MSG, PM_REMOVE, SWP_NOACTIVATE, SW_SHOWNOACTIVATE,
                    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
                    WS_EX_TRANSPARENT, WS_POPUP,
                },
            },
        };

        unsafe extern "system" fn wnd_proc(
            window: HWND,
            message: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            DefWindowProcW(window, message, wparam, lparam)
        }

        let taskbar = match unsafe { FindWindowW(w!("Shell_TrayWnd"), None) } {
            Ok(handle) if !handle.is_invalid() => handle,
            _ => {
                println!("タスクバーを見つけられないためスキップ");
                return;
            }
        };
        let mut bar = RECT::default();
        if unsafe { GetWindowRect(taskbar, &mut bar) }.is_err() {
            println!("タスクバーの矩形を取得できないためスキップ");
            return;
        }
        println!(
            "taskbar rect: left={} top={} right={} bottom={}",
            bar.left, bar.top, bar.right, bar.bottom
        );

        let class_name = w!("PcCustomOverlayProbe");
        let class = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            hbrBackground: HBRUSH(std::ptr::null_mut()),
            ..Default::default()
        };
        unsafe { RegisterClassW(&class) };

        // クリック透過(WS_EX_TRANSPARENT)、フォーカスを奪わない(WS_EX_NOACTIVATE)、
        // タスクバーに出ない(WS_EX_TOOLWINDOW)、最前面(WS_EX_TOPMOST)。
        let overlay = match unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST
                    | WS_EX_NOACTIVATE,
                class_name,
                w!("pc-custom overlay probe"),
                WS_POPUP,
                bar.left,
                bar.top,
                bar.right - bar.left,
                (bar.bottom - bar.top).min(48),
                None,
                None,
                None,
                None,
            )
        } {
            Ok(handle) if !handle.is_invalid() => handle,
            other => {
                println!("オーバーレイを作成できなかった: {other:?}");
                return;
            }
        };

        struct OwnedOverlay(HWND);
        impl Drop for OwnedOverlay {
            fn drop(&mut self) {
                let _ = unsafe { DestroyWindow(self.0) };
            }
        }
        let _owned = OwnedOverlay(overlay);

        let _ = unsafe { SetLayeredWindowAttributes(overlay, COLORREF(0), 140, LWA_ALPHA) };
        let _ = unsafe { ShowWindow(overlay, SW_SHOWNOACTIVATE) };
        let _ = unsafe {
            SetWindowPos(
                overlay,
                HWND_TOPMOST,
                bar.left,
                bar.top,
                bar.right - bar.left,
                (bar.bottom - bar.top).min(48),
                SWP_NOACTIVATE,
            )
        };

        // メッセージを少し回して落ち着かせる。
        for _ in 0..20 {
            let mut message = MSG::default();
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                unsafe { DispatchMessageW(&message) };
            }
            sleep(Duration::from_millis(100));
        }

        /// トップレベルの Z 順を先頭から辿り、対象が何番目かを返す。小さいほど手前。
        fn z_index(target: HWND) -> Option<usize> {
            let mut current = unsafe { GetTopWindow(None) }.ok()?;
            let mut index = 0usize;
            loop {
                if current == target {
                    return Some(index);
                }
                match unsafe { GetWindow(current, GW_HWNDNEXT) } {
                    Ok(next) if !next.is_invalid() => {
                        current = next;
                        index += 1;
                        if index > 5000 {
                            return None;
                        }
                    }
                    _ => return None,
                }
            }
        }

        let overlay_z = z_index(overlay);
        let taskbar_z = z_index(taskbar);
        println!("z-order: overlay={overlay_z:?} taskbar={taskbar_z:?} (小さいほど手前)");

        match (overlay_z, taskbar_z) {
            (Some(o), Some(t)) if o < t => {
                println!(
                    "EVIDENCE: オーバーレイはタスクバーより手前に置けた。Safe方式が成立しうる"
                );
            }
            (Some(o), Some(t)) => {
                println!("EVIDENCE: オーバーレイはタスクバーより奥だった (overlay={o} taskbar={t})。Safe方式は不成立");
            }
            other => {
                println!("EVIDENCE: Z順を判定できなかった {other:?}");
            }
        }
    }

    /// 候補をまとめて適用し、シェル再起動 1 回で複数の項目を同時に判定する。
    ///
    /// 1 件ずつ測ると再起動が件数×2 回になり、画面が何十回も点滅する。
    /// 観測できる信号が互いに独立している項目は、まとめて測ってよい。
    ///
    /// **実機のタスクバーが 2 回消えて戻る。**
    #[test]
    #[ignore = "実機のシェルを2回再起動して複数候補をまとめて判定"]
    fn batch_measure_taskbar_candidates_after_shell_restart() {
        use crate::backup::{
            prepare_registry_backup, restore_registry_backup, RegistryBackup, RegistryTarget,
        };
        use crate::windows::{restart_shell, write_raw_value};
        use std::{thread::sleep, time::Duration};

        const REG_DWORD: u32 = 4;
        const ADVANCED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        const SEARCH: &str = r"Software\Microsoft\Windows\CurrentVersion\Search";

        /// 測る対象。`marker` がタスクバー要素名に含まれるかで有無を判定する。
        struct Candidate {
            id: &'static str,
            subkey: &'static str,
            value_name: &'static str,
            off_value: u32,
            marker: &'static str,
        }

        let candidates = [
            Candidate {
                id: "taskbar.search_mode",
                subkey: SEARCH,
                value_name: "SearchboxTaskbarMode",
                off_value: 0, // 0 = 検索を隠す
                marker: "検索",
            },
            Candidate {
                id: "taskbar.show_desktop",
                subkey: ADVANCED,
                value_name: "TaskbarSd",
                off_value: 0, // 0 = 右端の「デスクトップの表示」を出さない
                marker: "デスクトップを表示する",
            },
            // 以前「反映されない」として Guided へ降格した2件。
            // あのときは設定変更通知だけで判定していた。シェル再起動込みで測り直す。
            Candidate {
                id: "taskbar.task_view",
                subkey: ADVANCED,
                value_name: "ShowTaskViewButton",
                off_value: 0,
                marker: "タスク ビュー",
            },
            Candidate {
                id: "taskbar.widgets",
                subkey: ADVANCED,
                value_name: "TaskbarDa",
                off_value: 0,
                marker: "ウィジェット",
            },
        ];

        fn names_or_empty() -> Vec<String> {
            taskbar_element_names().unwrap_or_default()
        }
        fn contains(names: &[String], marker: &str) -> bool {
            names.iter().any(|name| name.contains(marker))
        }
        /// シェルが戻り、要素が読めるようになるまで待つ。
        fn settle() -> Vec<String> {
            for _ in 0..40 {
                sleep(Duration::from_millis(300));
                let names = names_or_empty();
                if names.len() > 5 {
                    return names;
                }
            }
            names_or_empty()
        }

        let before = names_or_empty();
        if before.len() <= 5 {
            println!("タスクバーを観測できないためスキップ");
            return;
        }
        for candidate in &candidates {
            println!(
                "before: {} marker={:?} present={}",
                candidate.id,
                candidate.marker,
                contains(&before, candidate.marker)
            );
        }

        // 全件のバックアップを取ってから、まとめて書く。
        let mut backups: Vec<(&Candidate, RegistryBackup)> = Vec::new();
        for candidate in &candidates {
            let target = RegistryTarget::current_user_64(candidate.subkey, candidate.value_name);
            match prepare_registry_backup(
                target,
                REG_DWORD,
                candidate.off_value.to_le_bytes().to_vec(),
                1,
                26_200,
            ) {
                Ok(backup) => backups.push((candidate, backup)),
                Err(error) => println!("{}: backup 不可のため除外 {error:?}", candidate.id),
            }
        }

        struct Guard(Vec<RegistryBackup>, bool);
        impl Drop for Guard {
            fn drop(&mut self) {
                if self.1 {
                    return;
                }
                for backup in &self.0 {
                    let _ = restore_registry_backup(backup);
                }
                // panic で巻き戻ってきた場合もここを通る。
                // **タスクバーが無いまま終わらせない。** 実際に一度そうなった。
                let _ = crate::windows::restart_shell();
                for _ in 0..40 {
                    if crate::windows::taskbar_is_present() {
                        return;
                    }
                    sleep(Duration::from_millis(250));
                }
                let _ = crate::windows::restart_shell();
            }
        }
        let mut guard = Guard(backups.iter().map(|(_, b)| b.clone()).collect(), false);

        for (candidate, backup) in &backups {
            if let Err(error) = write_raw_value(
                &backup.location,
                REG_DWORD,
                &candidate.off_value.to_le_bytes(),
            ) {
                println!("{}: 書き込み失敗 {error:?}", candidate.id);
            }
        }

        println!(
            "restart#1: {:?}",
            restart_shell().expect("restart after write")
        );
        let after = settle();

        let mut verdicts = Vec::new();
        for (candidate, _) in &backups {
            let was = contains(&before, candidate.marker);
            let now = contains(&after, candidate.marker);
            let changed = was != now;
            println!(
                "applied: {} present {was} -> {now}  changed={changed}",
                candidate.id
            );
            verdicts.push((candidate.id, changed));
        }

        for (_, backup) in &backups {
            let _ = restore_registry_backup(backup);
        }
        println!(
            "restart#2: {:?}",
            restart_shell().expect("restart after restore")
        );
        let restored = settle();
        guard.1 = true;
        drop(guard);

        // 反映には時間がかかる。1回読んで違ったら失敗、では早すぎる。
        for (candidate, _) in &backups {
            let originally = contains(&before, candidate.marker);
            let mut back = contains(&restored, candidate.marker);
            for _ in 0..20 {
                if back == originally {
                    break;
                }
                sleep(Duration::from_millis(500));
                back = contains(&names_or_empty(), candidate.marker);
            }
            println!(
                "restored: {} present={back} (元は {originally})",
                candidate.id
            );
            assert_eq!(back, originally, "{} が元へ戻ること", candidate.id);
        }

        println!("---- 判定 ----");
        for (id, changed) in verdicts {
            if changed {
                println!("EVIDENCE: {id} はシェル再起動で実UIへ反映される。昇格可能");
            } else {
                println!("EVIDENCE: {id} は反映を確認できなかった。昇格しない");
            }
        }
    }

    /// 42 件の候補を「使えるようにする」ための決定的な実験。
    ///
    /// 設定変更通知だけでは反映されないことは既に実測済み。ここではシェルを再起動して
    /// 反映されるかを見る。ここが偽なら、シェル再起動を足しても候補は昇格できない。
    ///
    /// **実機のタスクバーが 2 回消えて戻る。開いているエクスプローラーの窓は閉じる。**
    #[test]
    #[ignore = "実機のシェルを2回再起動する。反映可否の決定実験"]
    fn shell_restart_makes_taskbar_alignment_actually_apply() {
        use crate::backup::{
            prepare_registry_backup, read_registry_state, restore_registry_backup, RegistryTarget,
        };
        use crate::windows::{restart_shell, write_raw_value};
        use std::{thread::sleep, time::Duration};

        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        const VALUE: &str = "TaskbarAl";
        const REG_DWORD: u32 = 4;

        /// シェル再起動後、タスクバーが観測できるようになるまで待つ。
        fn settle() -> Option<TaskbarLayoutObservation> {
            for _ in 0..40 {
                sleep(Duration::from_millis(300));
                if let Ok(now) = observe_taskbar_layout() {
                    if now.taskbar_width > 0 && now.start_button_width > 0 {
                        return Some(now);
                    }
                }
            }
            None
        }

        let Ok(baseline) = observe_taskbar_layout() else {
            println!("タスクバーを観測できないためスキップ");
            return;
        };
        let target = RegistryTarget::current_user_64(SUBKEY, VALUE);
        let current = read_registry_state(&target.location()).expect("read TaskbarAl");
        let original_value = if current.value_existed {
            u32::from_le_bytes(current.raw_bytes[..4].try_into().unwrap_or([0; 4]))
        } else {
            1
        };
        let flipped = if original_value == 0 { 1u32 } else { 0u32 };

        let backup =
            prepare_registry_backup(target, REG_DWORD, flipped.to_le_bytes().to_vec(), 1, 26_200)
                .expect("prepare backup");

        // panic しても必ず元へ戻し、シェルも戻す。
        struct Guard(crate::backup::RegistryBackup, bool);
        impl Drop for Guard {
            fn drop(&mut self) {
                if !self.1 {
                    let _ = restore_registry_backup(&self.0);
                    let _ = crate::windows::restart_shell();
                }
            }
        }
        let mut guard = Guard(backup.clone(), false);

        println!(
            "before: TaskbarAl={original_value} start_center_ratio={:.3}",
            baseline.start_center_ratio()
        );

        write_raw_value(&backup.location, REG_DWORD, &flipped.to_le_bytes())
            .expect("write flipped");
        let restart1 = restart_shell().expect("restart shell after write");
        println!("restart#1: {restart1:?}");
        let after = settle();
        let moved = match after {
            Some(observation) => {
                let delta =
                    (observation.start_center_ratio() - baseline.start_center_ratio()).abs();
                println!(
                    "applied: start_center_ratio={:.3} delta={delta:.3}",
                    observation.start_center_ratio()
                );
                delta > 0.05
            }
            None => {
                println!("再起動後にタスクバーを観測できなかった");
                false
            }
        };

        // 元へ戻す。
        let restored = restore_registry_backup(&backup).expect("restore original");
        let restart2 = restart_shell().expect("restart shell after restore");
        println!("restart#2: {restart2:?} restore={restored:?}");
        let back = settle();
        guard.1 = true;
        drop(guard);

        if let Some(observation) = back {
            let delta = (observation.start_center_ratio() - baseline.start_center_ratio()).abs();
            println!(
                "restored: start_center_ratio={:.3} delta_from_baseline={delta:.3}",
                observation.start_center_ratio()
            );
            assert!(delta <= 0.05, "元の配置へ戻ること");
        }

        if moved {
            println!("EVIDENCE: シェル再起動で TaskbarAl は実UIへ反映される。候補の昇格が可能");
        } else {
            println!("EVIDENCE: シェル再起動でも反映されなかった。昇格の根拠にならない");
        }
    }

    #[test]
    #[ignore = "実機のタスクバーを一時的に変更する証拠取得用"]
    fn taskbar_alignment_write_actually_moves_the_start_button() {
        use crate::backup::{
            prepare_registry_backup, read_registry_state, restore_registry_backup,
            RegistryRestoreOutcome, RegistryTarget,
        };
        use crate::windows::{notify_explorer_settings_changed, write_raw_value};
        use std::{thread::sleep, time::Duration};

        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        const VALUE: &str = "TaskbarAl";
        const REG_DWORD: u32 = 4;

        fn wait_for_layout_change(before: f64) -> Option<TaskbarLayoutObservation> {
            // Explorer が設定変更通知を反映するまで待つ。反映は即時ではない。
            for _ in 0..30 {
                sleep(Duration::from_millis(200));
                if let Ok(now) = observe_taskbar_layout() {
                    if (now.start_center_ratio() - before).abs() > 0.05 {
                        return Some(now);
                    }
                }
            }
            None
        }

        let target = RegistryTarget::current_user_64(SUBKEY, VALUE);
        let baseline = match observe_taskbar_layout() {
            Ok(observation) => observation,
            Err(error) => {
                println!("タスクバーを観測できないため証拠取得をスキップ: {error:?}");
                return;
            }
        };
        let current = read_registry_state(&target.location()).expect("read TaskbarAl");
        let original_value = if current.value_existed {
            u32::from_le_bytes(current.raw_bytes[..4].try_into().unwrap_or([0; 4]))
        } else {
            1
        };
        // 0 = 左寄せ, 1 = 中央寄せ。いまと反対側へ動かす。
        let flipped = if original_value == 0 { 1u32 } else { 0u32 };

        let backup =
            prepare_registry_backup(target, REG_DWORD, flipped.to_le_bytes().to_vec(), 1, 26_200)
                .expect("prepare backup before evidence run");

        println!(
            "before: TaskbarAl={original_value} start_center_ratio={:.3}",
            baseline.start_center_ratio()
        );

        write_raw_value(&backup.location, REG_DWORD, &flipped.to_le_bytes())
            .expect("apply flipped alignment");
        let _ = notify_explorer_settings_changed();
        let moved = wait_for_layout_change(baseline.start_center_ratio());

        // 何があっても必ず元へ戻す。
        let restored = restore_registry_backup(&backup).expect("restore original alignment");
        let _ = notify_explorer_settings_changed();
        assert!(
            matches!(
                restored,
                RegistryRestoreOutcome::Restored | RegistryRestoreOutcome::AlreadyOriginal
            ),
            "元の状態へ戻せること: {restored:?}"
        );
        let after_restore = wait_for_layout_change(
            moved
                .map(|m| m.start_center_ratio())
                .unwrap_or(baseline.start_center_ratio()),
        );

        match moved {
            Some(observation) => {
                println!(
                    "moved:  TaskbarAl={flipped} start_center_ratio={:.3}",
                    observation.start_center_ratio()
                );
                println!(
                    "after restore: start_center_ratio={:.3}",
                    after_restore
                        .map(|o| o.start_center_ratio())
                        .unwrap_or(baseline.start_center_ratio())
                );
                println!("EVIDENCE: 第三者書き込みがタスクバーUIへ反映されることを実測で確認");
            }
            None => {
                println!(
                    "EVIDENCE: 反映を検出できなかった。設定値は書けてもUIへ反映されない可能性がある"
                );
            }
        }

        // 復元の正確性は既存の検証で担保する。
        let final_state = read_registry_state(&backup.location).expect("read back");
        assert_eq!(
            final_state, backup.original,
            "値・型・有無まで元どおりに戻す"
        );
    }

    /// アクセントカラーは DwmGetColorizationColor で**実効色**を読めるため、
    /// タスクバーと同じ枠組みで往復検証できる。反映されるなら変更機能を解禁できるし、
    /// 反映されないならタスクバーと同じく案内に留める根拠になる。
    #[test]
    #[ignore = "実機のアクセントカラーを一時的に変更する証拠取得用"]
    fn accent_colour_write_actually_changes_the_effective_dwm_colour() {
        use crate::backup::{
            prepare_registry_backup, read_registry_state, restore_registry_backup,
            RegistryRestoreOutcome, RegistryTarget,
        };
        use crate::windows::{notify_theme_changed, system_accent_color, write_raw_value};
        use std::{thread::sleep, time::Duration};

        const DWM_SUBKEY: &str = r"Software\Microsoft\Windows\DWM";
        const VALUE: &str = "ColorizationColor";
        const REG_DWORD: u32 = 4;

        let before = match system_accent_color() {
            Ok(colour) => colour,
            Err(error) => {
                println!("実効色を読めないため証拠取得をスキップ: {error:?}");
                return;
            }
        };
        println!(
            "before: effective #{:02X}{:02X}{:02X}",
            before.red, before.green, before.blue
        );

        // 元の色から十分離れた色を選ぶ（判定を確実にするため）。
        let probe: u32 = if before.red > 128 {
            0xC4_20_60_A0
        } else {
            0xC4_C0_50_20
        };
        let target = RegistryTarget::current_user_64(DWM_SUBKEY, VALUE);
        let backup =
            prepare_registry_backup(target, REG_DWORD, probe.to_le_bytes().to_vec(), 1, 26_200)
                .expect("prepare accent backup");

        write_raw_value(&backup.location, REG_DWORD, &probe.to_le_bytes())
            .expect("write probe colour");
        let _ = notify_theme_changed();

        let mut changed = None;
        for _ in 0..25 {
            sleep(Duration::from_millis(200));
            if let Ok(now) = system_accent_color() {
                if now.red != before.red || now.green != before.green || now.blue != before.blue {
                    changed = Some(now);
                    break;
                }
            }
        }

        // 何があっても元へ戻す。
        let restored = restore_registry_backup(&backup).expect("restore accent colour");
        let _ = notify_theme_changed();
        assert!(
            matches!(
                restored,
                RegistryRestoreOutcome::Restored | RegistryRestoreOutcome::AlreadyOriginal
            ),
            "元の状態へ戻せること: {restored:?}"
        );

        match changed {
            Some(now) => println!(
                "EVIDENCE: 実効色が #{:02X}{:02X}{:02X} へ変化。アクセントカラー変更は反映される",
                now.red, now.green, now.blue
            ),
            None => println!(
                "EVIDENCE: 実効色は変化せず。保存値を書いてもDWMへは反映されない（案内に留めるべき）"
            ),
        }

        let final_state = read_registry_state(&backup.location).expect("read back");
        assert_eq!(
            final_state, backup.original,
            "値・型・有無まで元どおりに戻す"
        );
    }

    /// すでに「変更可能」として出荷しているタスクバー系Actionが、本当に効いているか。
    /// 効いていなければ、利用者は「適用した／戻した」と表示されるのに何も変わらない。
    #[test]
    #[ignore = "実機のタスクバー表示を一時的に変更する"]
    fn shipped_task_view_toggle_actually_changes_the_taskbar() {
        use crate::backup::{
            prepare_registry_backup, read_registry_state, restore_registry_backup, RegistryTarget,
        };
        use crate::windows::{notify_explorer_settings_changed, write_raw_value};
        use std::{thread::sleep, time::Duration};

        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        const NAMES: &[&str] = &["タスク ビュー", "タスクビュー", "Task View"];

        let target = RegistryTarget::current_user_64(SUBKEY, "ShowTaskViewButton");
        let before_present = match observe_taskbar_element(NAMES) {
            Ok(present) => present,
            Err(error) => {
                println!("観測できないためスキップ: {error:?}");
                return;
            }
        };
        let current = read_registry_state(&target.location()).expect("read ShowTaskViewButton");
        println!(
            "before: present={before_present} value_exists={}",
            current.value_existed
        );

        let flipped: u32 = if before_present { 0 } else { 1 };
        let backup = prepare_registry_backup(target, 4, flipped.to_le_bytes().to_vec(), 1, 26_200)
            .expect("prepare backup");
        write_raw_value(&backup.location, 4, &flipped.to_le_bytes()).expect("write");
        let _ = notify_explorer_settings_changed();

        let mut changed = false;
        for _ in 0..25 {
            sleep(Duration::from_millis(200));
            if let Ok(now) = observe_taskbar_element(NAMES) {
                if now != before_present {
                    changed = true;
                    break;
                }
            }
        }

        restore_registry_backup(&backup).expect("restore");
        let _ = notify_explorer_settings_changed();
        sleep(Duration::from_millis(400));

        let after = read_registry_state(&backup.location).expect("read back");
        assert_eq!(after, backup.original, "元どおりに戻す");

        if changed {
            println!("EVIDENCE: 出荷中のタスクビュー切替は実UIへ反映される");
        } else {
            println!(
                "EVIDENCE: 出荷中のタスクビュー切替は実UIへ反映されなかった。可変扱いを見直す必要がある"
            );
        }
    }

    #[test]
    #[ignore = "実機のタスクバー表示を一時的に変更する"]
    fn shipped_widgets_toggle_actually_changes_the_taskbar() {
        use crate::backup::{
            prepare_registry_backup, read_registry_state, restore_registry_backup, RegistryTarget,
        };
        use crate::windows::{notify_explorer_settings_changed, write_raw_value};
        use std::{thread::sleep, time::Duration};

        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        const NAMES: &[&str] = &["ウィジェット", "Widgets"];

        let target = RegistryTarget::current_user_64(SUBKEY, "TaskbarDa");
        let before = match observe_taskbar_element(NAMES) {
            Ok(present) => present,
            Err(error) => {
                println!("観測できないためスキップ: {error:?}");
                return;
            }
        };
        println!("before: widgets_present={before}");
        let flipped: u32 = if before { 0 } else { 1 };
        let backup = prepare_registry_backup(target, 4, flipped.to_le_bytes().to_vec(), 1, 26_200)
            .expect("prepare");
        if let Err(error) = write_raw_value(&backup.location, 4, &flipped.to_le_bytes()) {
            // 書き込み自体が拒否されるなら、利用者にはエラーとして見える（無言の無反応ではない）。
            println!("EVIDENCE: ウィジェット値への書き込みが拒否された: {error:?}");
            return;
        }
        let _ = notify_explorer_settings_changed();
        let mut changed = false;
        for _ in 0..25 {
            sleep(Duration::from_millis(200));
            if let Ok(now) = observe_taskbar_element(NAMES) {
                if now != before {
                    changed = true;
                    break;
                }
            }
        }
        restore_registry_backup(&backup).expect("restore");
        let _ = notify_explorer_settings_changed();
        sleep(Duration::from_millis(400));
        let after = read_registry_state(&backup.location).expect("read back");
        assert_eq!(after, backup.original, "元どおりに戻す");
        println!(
            "EVIDENCE: 出荷中のウィジェット切替は実UIへ{}",
            if changed {
                "反映される"
            } else {
                "反映されなかった"
            }
        );
    }

    /// 注意: シェルの表示名はプロセスごとにキャッシュされる。
    /// 同一プロセス内で設定を変えても表示名は変わらないため、
    /// 「拡張子表示」の反映は**別プロセス**で確認しなければならない。
    ///
    /// 実測（別プロセス起動で確認済み）:
    ///   HideFileExt=0 -> "totonoe-extension-probe2.txt"
    ///   HideFileExt=1 -> "totonoe-extension-probe2"
    /// つまり `explorer.show_extensions` は実際に効いている。
    /// この事実を、同一プロセス内の素朴な往復テストで否定してはいけない。
    #[test]
    #[ignore = "同一プロセスではキャッシュされるため、判定に使わないこと"]
    fn show_extensions_needs_a_fresh_process_to_observe() {
        let probe = std::env::temp_dir().join("totonoe-cache-note.txt");
        std::fs::write(&probe, b"probe").expect("create");
        let name = shell_display_name(&probe).expect("display name");
        println!("このプロセスでの表示名: {name}（設定を変えてもここでは変わらない）");
        let _ = std::fs::remove_file(&probe);
    }

    /// 読み取り専用。新しいプロセスでシェル表示名を1回だけ報告する（キャッシュ検証用）。
    #[test]
    #[ignore = "調査用の読み取り専用プローブ"]
    fn reports_display_name_only() {
        let probe = std::env::temp_dir().join("totonoe-extension-probe2.txt");
        std::fs::write(&probe, b"probe").expect("create");
        match shell_display_name(&probe) {
            Ok(name) => println!("DISPLAY={name}"),
            Err(error) => println!("DISPLAY_ERR={error:?}"),
        }
        let _ = std::fs::remove_file(&probe);
    }

    /// 出荷中の `explorer.clock_seconds` が、実行中Explorerの時計へ実際に効いているか。
    /// 観測点は自プロセスの外（Explorerが描いている文字列）。
    #[test]
    #[ignore = "実機の時計表示を一時的に変更する"]
    fn shipped_clock_seconds_actually_changes_the_taskbar_clock() {
        use crate::backup::{
            prepare_registry_backup, read_registry_state, restore_registry_backup, RegistryTarget,
        };
        use crate::windows::{notify_explorer_settings_changed, write_raw_value};
        use std::{thread::sleep, time::Duration};

        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";

        fn separator_count(text: &str) -> usize {
            text.chars().filter(|c| *c == ':' || *c == '：').count()
        }

        let before = match observe_taskbar_clock_text() {
            Ok(text) => text,
            Err(error) => {
                println!("時計を観測できないためスキップ: {error:?}");
                return;
            }
        };
        let before_has_seconds = separator_count(&before) >= 2;
        println!("before: clock=\"{before}\" has_seconds={before_has_seconds}");

        let target = RegistryTarget::current_user_64(SUBKEY, "ShowSecondsInSystemClock");
        let flipped: u32 = if before_has_seconds { 0 } else { 1 };
        let backup = prepare_registry_backup(target, 4, flipped.to_le_bytes().to_vec(), 1, 26_200)
            .expect("prepare backup");
        if let Err(error) = write_raw_value(&backup.location, 4, &flipped.to_le_bytes()) {
            println!("EVIDENCE: 秒表示の値への書き込みが拒否された: {error:?}");
            return;
        }
        let _ = notify_explorer_settings_changed();

        let mut changed = None;
        for _ in 0..30 {
            sleep(Duration::from_millis(300));
            if let Ok(now) = observe_taskbar_clock_text() {
                if (separator_count(&now) >= 2) != before_has_seconds {
                    changed = Some(now);
                    break;
                }
            }
        }

        restore_registry_backup(&backup).expect("restore");
        let _ = notify_explorer_settings_changed();
        sleep(Duration::from_millis(500));
        let after = read_registry_state(&backup.location).expect("read back");
        assert_eq!(after, backup.original, "元どおりに戻す");

        match changed {
            Some(text) => println!("EVIDENCE: 時計が \"{text}\" へ変化。秒表示は実際に効く"),
            None => println!("EVIDENCE: 時計は変化せず。秒表示は実UIへ反映されない"),
        }
    }

    /// 読み取り専用の診断。タスクバー上の要素名を列挙する。
    #[test]
    #[ignore = "調査用"]
    fn dump_taskbar_element_names() {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Accessibility::{
            CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
        };
        use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
        unsafe {
            let taskbar: HWND =
                FindWindowW(windows::core::w!("Shell_TrayWnd"), None).expect("taskbar");
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).expect("uia");
            let root: IUIAutomationElement = automation.ElementFromHandle(taskbar).expect("root");
            let condition = automation.CreateTrueCondition().expect("cond");
            let all = root
                .FindAll(TreeScope_Descendants, &condition)
                .expect("all");
            let count = all.Length().unwrap_or(0);
            println!("要素数: {count}");
            for index in 0..count {
                if let Ok(element) = all.GetElement(index) {
                    let name = element
                        .CurrentName()
                        .map(|n| n.to_string())
                        .unwrap_or_default();
                    if !name.trim().is_empty() {
                        println!("[{index}] {name}");
                    }
                }
            }
            CoUninitialize();
        }
    }

    /// 出荷中の `explorer.show_hidden` の検証（**未完成**）。
    ///
    /// 現状の問題:
    /// 1. `FindWindowW` は最初に見つかった Explorer ウィンドウを返すため、
    ///    利用者が既に開いている別のウィンドウを誤って観測・操作しうる。
    ///    実際に最初の実行で、対象でないウィンドウを観測し（項目数101）、
    ///    後片付けでそのウィンドウへ WM_CLOSE を送ってしまった。
    /// 2. タイトル完全一致に変えたら、今度はウィンドウを見つけられなくなった。
    ///
    /// 正しくやるには `EnumWindows` で CabinetWClass を列挙し、
    /// タイトルの**部分一致**で自分が開いたウィンドウだけを選び、
    /// そのウィンドウにだけ操作を限定する必要がある。
    /// それまでは走らせない。誤った結論は結論が無いより悪い。
    #[test]
    #[ignore = "実機のExplorerを一時的に開いて設定を変更する"]
    fn shipped_show_hidden_actually_changes_the_explorer_listing() {
        use crate::backup::{
            prepare_registry_backup, read_registry_state, restore_registry_backup, RegistryTarget,
        };
        use crate::windows::{notify_explorer_settings_changed, write_raw_value};
        use std::{thread::sleep, time::Duration};
        use windows::core::HSTRING;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SW_SHOWNORMAL, WM_CLOSE};

        const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
        const HIDDEN_NAME: &str = "zz-secret-item.txt";

        // 隠し属性つきの検査用ファイルを作る。
        let dir = std::env::temp_dir().join("totonoe-probe-folder");
        let _ = std::fs::create_dir_all(&dir);
        let hidden = dir.join(HIDDEN_NAME);
        std::fs::write(&hidden, b"probe").expect("create probe");
        {
            use std::os::windows::fs::OpenOptionsExt;
            let _ = std::process::Command::new("attrib")
                .arg("+h")
                .arg(&hidden)
                .output();
            let _ = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .open(&hidden);
        }

        // Explorerで開く。
        let path = HSTRING::from(dir.as_os_str());
        unsafe {
            ShellExecuteW(
                None,
                windows::core::w!("open"),
                &path,
                None,
                None,
                SW_SHOWNORMAL,
            );
        }
        sleep(Duration::from_millis(2500));

        let listed = |names: &[String]| names.iter().any(|n| n.contains("zz-secret-item"));
        const PROBE_DIR: &str = "totonoe-probe-folder";
        // 自分が開いたウィンドウだけを対象にする。見つからなければ何もしない。
        let window = match find_explorer_window_by_title(PROBE_DIR) {
            Ok(handle) => handle,
            Err(error) => {
                println!("自分のExplorerウィンドウを特定できないためスキップ: {error:?}");
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
        };
        let before_names = match explorer_window_item_names(window) {
            Ok(names) => names,
            Err(error) => {
                println!("Explorerを観測できないためスキップ: {error:?}");
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
        };
        let before_visible = listed(&before_names);
        println!(
            "before: hidden_file_listed={before_visible} items={}",
            before_names.len()
        );

        let target = RegistryTarget::current_user_64(SUBKEY, "Hidden");
        // 1 = 隠しファイルを表示, 2 = 表示しない。
        let flipped: u32 = if before_visible { 2 } else { 1 };
        let backup = prepare_registry_backup(target, 4, flipped.to_le_bytes().to_vec(), 1, 26_200)
            .expect("prepare backup");
        if let Err(error) = write_raw_value(&backup.location, 4, &flipped.to_le_bytes()) {
            println!("EVIDENCE: 隠しファイル設定への書き込みが拒否された: {error:?}");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let _ = notify_explorer_settings_changed();

        let mut changed = false;
        for _ in 0..30 {
            sleep(Duration::from_millis(300));
            if let Ok(names) = explorer_window_item_names(window) {
                if listed(&names) != before_visible {
                    changed = true;
                    break;
                }
            }
        }

        // 開いている窓に効かなくても、**新しく開いた窓**には効くかもしれない。
        // 利用者にとって意味が違う（壊れている vs 窓を開き直せば反映される）ので切り分ける。
        let mut fresh_window_reflects = None;
        if !changed {
            unsafe {
                ShellExecuteW(
                    None,
                    windows::core::w!("open"),
                    &path,
                    None,
                    None,
                    SW_SHOWNORMAL,
                );
            }
            sleep(Duration::from_millis(2500));
            if let Ok(handle) = find_explorer_window_by_title(PROBE_DIR) {
                if handle != window {
                    if let Ok(names) = explorer_window_item_names(handle) {
                        fresh_window_reflects = Some(listed(&names) != before_visible);
                    }
                    unsafe {
                        let target =
                            windows::Win32::Foundation::HWND(handle as *mut core::ffi::c_void);
                        let _ = PostMessageW(target, WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                }
            }
        }

        restore_registry_backup(&backup).expect("restore");
        let _ = notify_explorer_settings_changed();
        sleep(Duration::from_millis(500));

        // Explorerウィンドウを閉じ、検査用ファイルを片付ける。
        unsafe {
            // 閉じるのは、自分が開いたウィンドウだけ。
            let target = windows::Win32::Foundation::HWND(window as *mut core::ffi::c_void);
            let _ = PostMessageW(target, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        sleep(Duration::from_millis(600));
        let _ = std::fs::remove_dir_all(&dir);

        let after = read_registry_state(&backup.location).expect("read back");
        assert_eq!(after, backup.original, "元どおりに戻す");
        match (changed, fresh_window_reflects) {
            (true, _) => println!("EVIDENCE: 隠しファイル表示は開いている窓へ即座に反映される"),
            (false, Some(true)) => {
                println!("EVIDENCE: 開いている窓には反映されないが、新しく開いた窓には反映される")
            }
            (false, Some(false)) => println!("EVIDENCE: 新しく開いた窓にも反映されない"),
            (false, None) => {
                println!("EVIDENCE: 開いている窓には反映されず、新窓の確認はできなかった")
            }
        }
    }

    /// 文書化API `SHGetSetSettings` なら、開いているExplorerへ反映されるか。
    /// レジストリ直書きでは反映されなかったので、その差を確かめる。
    #[test]
    #[ignore = "実機のExplorerを一時的に開いて設定を変更する"]
    fn documented_shell_api_reaches_the_open_explorer_window() {
        use std::{thread::sleep, time::Duration};
        use windows::core::HSTRING;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SW_SHOWNORMAL, WM_CLOSE};

        const PROBE_DIR: &str = "totonoe-api-probe-folder";
        let dir = std::env::temp_dir().join(PROBE_DIR);
        let _ = std::fs::create_dir_all(&dir);
        let hidden = dir.join("zz-secret-item.txt");
        std::fs::write(&hidden, b"probe").expect("create probe");
        let _ = std::process::Command::new("attrib")
            .arg("+h")
            .arg(&hidden)
            .output();

        let original = shell_state_show_hidden().expect("read shell state");
        println!("before: shell_state_show_hidden={original}");

        let path = HSTRING::from(dir.as_os_str());
        unsafe {
            ShellExecuteW(
                None,
                windows::core::w!("open"),
                &path,
                None,
                None,
                SW_SHOWNORMAL,
            );
        }
        sleep(Duration::from_millis(2500));

        let window = match find_explorer_window_by_title(PROBE_DIR) {
            Ok(handle) => handle,
            Err(error) => {
                println!("窓を特定できないためスキップ: {error:?}");
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
        };
        let listed = |names: &[String]| names.iter().any(|n| n.contains("zz-secret-item"));
        let before_listed = explorer_window_item_names(window)
            .map(|n| listed(&n))
            .unwrap_or(false);
        println!("before: hidden_listed={before_listed}");

        set_shell_state_show_hidden(!original).expect("set via documented API");

        let mut changed = false;
        for _ in 0..30 {
            sleep(Duration::from_millis(300));
            if let Ok(names) = explorer_window_item_names(window) {
                if listed(&names) != before_listed {
                    changed = true;
                    break;
                }
            }
        }

        set_shell_state_show_hidden(original).expect("restore via documented API");
        sleep(Duration::from_millis(500));
        unsafe {
            let target = windows::Win32::Foundation::HWND(window as *mut core::ffi::c_void);
            let _ = PostMessageW(target, WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        sleep(Duration::from_millis(600));
        let _ = std::fs::remove_dir_all(&dir);

        let restored = shell_state_show_hidden().expect("read back");
        assert_eq!(restored, original, "元の設定へ戻す");
        println!(
            "EVIDENCE: 文書化APIでの変更は開いているExplorerへ{}",
            if changed {
                "反映される"
            } else {
                "反映されない"
            }
        );
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct HighContrastSnapshot {
        structure_size: u32,
        flags: u32,
        scheme_name: Option<String>,
    }

    fn read_high_contrast_snapshot() -> Result<HighContrastSnapshot, String> {
        use windows::Win32::UI::{
            Accessibility::HIGHCONTRASTW,
            WindowsAndMessaging::{
                SystemParametersInfoW, SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
            },
        };

        let structure_size = std::mem::size_of::<HIGHCONTRASTW>() as u32;
        let mut value = HIGHCONTRASTW {
            cbSize: structure_size,
            ..Default::default()
        };
        unsafe {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                structure_size,
                Some((&mut value as *mut HIGHCONTRASTW).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .map_err(|error| format!("SPI_GETHIGHCONTRAST failed: {error}"))?;
        if value.cbSize != structure_size {
            return Err(format!(
                "SPI_GETHIGHCONTRAST returned cbSize={} expected={structure_size}",
                value.cbSize
            ));
        }
        let scheme_name = if value.lpszDefaultScheme.is_null() {
            None
        } else {
            Some(
                unsafe { value.lpszDefaultScheme.to_string() }
                    .map_err(|error| format!("HIGHCONTRASTW scheme was invalid: {error}"))?,
            )
        };
        Ok(HighContrastSnapshot {
            structure_size,
            flags: value.dwFlags.0,
            scheme_name,
        })
    }

    fn write_high_contrast_snapshot(snapshot: &HighContrastSnapshot) -> Result<(), String> {
        use windows::{
            core::PWSTR,
            Win32::UI::{
                Accessibility::{HIGHCONTRASTW, HIGHCONTRASTW_FLAGS},
                WindowsAndMessaging::{
                    SystemParametersInfoW, SPIF_SENDCHANGE, SPI_SETHIGHCONTRAST,
                },
            },
        };

        let mut scheme_name = snapshot.scheme_name.as_ref().map(|name| {
            name.encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>()
        });
        let mut value = HIGHCONTRASTW {
            cbSize: snapshot.structure_size,
            dwFlags: HIGHCONTRASTW_FLAGS(snapshot.flags),
            lpszDefaultScheme: scheme_name
                .as_mut()
                .map_or(PWSTR::null(), |name| PWSTR(name.as_mut_ptr())),
        };
        unsafe {
            SystemParametersInfoW(
                SPI_SETHIGHCONTRAST,
                snapshot.structure_size,
                Some((&mut value as *mut HIGHCONTRASTW).cast()),
                SPIF_SENDCHANGE,
            )
        }
        .map_err(|error| format!("SPI_SETHIGHCONTRAST failed: {error}"))
    }

    struct HighContrastRestoreGuard {
        original: HighContrastSnapshot,
        applied: Option<HighContrastSnapshot>,
        restored: bool,
    }

    impl HighContrastRestoreGuard {
        fn new(original: HighContrastSnapshot) -> Self {
            Self {
                original,
                applied: None,
                restored: false,
            }
        }

        fn restore_if_unchanged(&mut self) -> Result<HighContrastSnapshot, String> {
            let applied = self
                .applied
                .as_ref()
                .ok_or_else(|| "applied high-contrast state was not captured".to_owned())?;
            let current = read_high_contrast_snapshot()?;
            if current != *applied {
                return Err(format!(
                    "external high-contrast change detected; current={current:?} applied={applied:?}"
                ));
            }
            write_high_contrast_snapshot(&self.original)?;
            let restored = read_high_contrast_snapshot()?;
            self.restored = true;
            Ok(restored)
        }
    }

    impl Drop for HighContrastRestoreGuard {
        fn drop(&mut self) {
            if self.restored {
                return;
            }
            let Some(applied) = self.applied.as_ref() else {
                return;
            };
            match read_high_contrast_snapshot() {
                Ok(current) if current == *applied => {
                    if let Err(error) = write_high_contrast_snapshot(&self.original) {
                        eprintln!("emergency high-contrast restoration failed: {error}");
                    }
                }
                Ok(current) => eprintln!(
                    "emergency high-contrast restoration skipped after an external change: \
                     current={current:?} applied={applied:?}"
                ),
                Err(error) => eprintln!(
                    "emergency high-contrast restoration skipped because current state is unknown: \
                     {error}"
                ),
            }
        }
    }

    /// 別プロセスで標準 Win32 コントロールを描く、実機コントラスト測定専用の子テスト。
    #[test]
    #[ignore = "helper process for the separate-process high-contrast pixel probe"]
    fn contrast_probe_child_process() {
        if std::env::var_os("TOTONOE_CONTRAST_PROBE_CHILD").is_none() {
            return;
        }
        use std::io::Write;
        use windows::{
            core::{w, HSTRING, PCWSTR},
            Win32::{
                Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
                Graphics::Gdi::{
                    GetStockObject, GetSysColorBrush, UpdateWindow, COLOR_WINDOW, DEFAULT_GUI_FONT,
                },
                UI::{
                    Controls::BS_COMMANDLINK,
                    WindowsAndMessaging::{
                        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
                        PostQuitMessage, RegisterClassW, SendMessageW, SetForegroundWindow,
                        ShowWindow, TranslateMessage, ES_AUTOHSCROLL, HMENU, MSG, SW_SHOW,
                        WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROY, WM_SETFONT, WNDCLASSW,
                        WS_BORDER, WS_CHILD, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
                    },
                },
            },
        };

        unsafe extern "system" fn contrast_probe_window_proc(
            window: HWND,
            message: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            if message == WM_DESTROY {
                unsafe { PostQuitMessage(0) };
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }

        let class_name = w!("TotonoeContrastPixelProbe");
        let class = WNDCLASSW {
            lpfnWndProc: Some(contrast_probe_window_proc),
            hbrBackground: unsafe { GetSysColorBrush(COLOR_WINDOW) },
            hInstance: HINSTANCE::default(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        unsafe { RegisterClassW(&class) };

        let title = HSTRING::from(
            std::env::var("TOTONOE_CONTRAST_PROBE_TITLE")
                .expect("contrast probe title environment variable"),
        );
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST,
                class_name,
                &title,
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                180,
                140,
                640,
                360,
                HWND::default(),
                HMENU::default(),
                HINSTANCE::default(),
                None,
            )
        }
        .expect("create contrast probe window");

        let controls = [
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("BUTTON"),
                    w!("Continue"),
                    WS_CHILD | WS_VISIBLE,
                    32,
                    44,
                    180,
                    42,
                    window,
                    HMENU::default(),
                    HINSTANCE::default(),
                    None,
                )
            }
            .expect("create standard button"),
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("EDIT"),
                    w!("Sample input"),
                    WS_CHILD | WS_VISIBLE | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                    32,
                    112,
                    280,
                    36,
                    window,
                    HMENU::default(),
                    HINSTANCE::default(),
                    None,
                )
            }
            .expect("create standard input"),
            unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    w!("BUTTON"),
                    w!("Open contrast settings"),
                    WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_COMMANDLINK as u32),
                    32,
                    178,
                    300,
                    42,
                    window,
                    HMENU::default(),
                    HINSTANCE::default(),
                    None,
                )
            }
            .expect("create standard link"),
        ];
        let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
        for control in controls {
            unsafe {
                SendMessageW(control, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
            }
        }
        unsafe {
            let _ = ShowWindow(window, SW_SHOW);
            let _ = SetForegroundWindow(window);
            assert!(UpdateWindow(window).as_bool(), "draw contrast probe window");
        }
        writeln!(
            std::io::stderr().lock(),
            "CONTRAST_WINDOW_READY:{}",
            window.0 as isize
        )
        .expect("publish contrast probe HWND");

        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, HWND::default(), 0, 0) }.0 > 0 {
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    struct ContrastProbeProcess {
        child: std::process::Child,
        window: HWND,
    }

    impl ContrastProbeProcess {
        fn start() -> Result<Self, String> {
            use std::{
                io::BufRead,
                process::{Command, Stdio},
                time::{Duration, Instant},
            };

            let title = format!("totonoe-contrast-probe-{}", uuid::Uuid::new_v4().simple());
            let mut child =
                Command::new(std::env::current_exe().map_err(|error| error.to_string())?)
                    .args([
                        "--ignored",
                        "--exact",
                        "windows::ui_probe::tests::contrast_probe_child_process",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env("TOTONOE_CONTRAST_PROBE_CHILD", "1")
                    .env("TOTONOE_CONTRAST_PROBE_TITLE", title)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|error| format!("spawn contrast probe child: {error}"))?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| "contrast probe stderr was not piped".to_owned())?;
            let mut lines = std::io::BufReader::new(stderr).lines();
            let deadline = Instant::now() + Duration::from_secs(8);
            while Instant::now() < deadline {
                let line = lines
                    .next()
                    .ok_or_else(|| {
                        "contrast probe child exited before creating a window".to_owned()
                    })?
                    .map_err(|error| format!("read contrast probe child output: {error}"))?;
                if let Some(value) = line.trim().strip_prefix("CONTRAST_WINDOW_READY:") {
                    let handle = value
                        .parse::<isize>()
                        .map_err(|error| format!("parse contrast probe HWND: {error}"))?;
                    return Ok(Self {
                        child,
                        window: HWND(handle as *mut core::ffi::c_void),
                    });
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            Err("contrast probe child did not become ready".to_owned())
        }
    }

    impl Drop for ContrastProbeProcess {
        fn drop(&mut self) {
            use std::time::{Duration, Instant};

            unsafe {
                let _ = PostMessageW(self.window, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                match self.child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) if Instant::now() < deadline => {
                        sleep(Duration::from_millis(25));
                    }
                    _ => {
                        let _ = self.child.kill();
                        let _ = self.child.wait();
                        return;
                    }
                }
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct ContrastPixelObservation {
        window_background: u32,
        button_surface: u32,
        input_background: u32,
        foreground_pixel: u32,
        contrast_ratio: f64,
    }

    fn wcag_luminance(color: u32) -> f64 {
        let linear = |value: u32| {
            let channel = f64::from(value) / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        let red = linear(color & 0xff);
        let green = linear((color >> 8) & 0xff);
        let blue = linear((color >> 16) & 0xff);
        red * 0.2126 + green * 0.7152 + blue * 0.0722
    }

    fn contrast_ratio(first: u32, second: u32) -> f64 {
        let first = wcag_luminance(first);
        let second = wcag_luminance(second);
        (first.max(second) + 0.05) / (first.min(second) + 0.05)
    }

    fn observe_contrast_probe_pixels(window: HWND) -> Result<ContrastPixelObservation, String> {
        use windows::Win32::{
            Foundation::{POINT, RECT},
            Graphics::Gdi::{
                BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC,
                DeleteObject, GetDC, RedrawWindow, ReleaseDC, SelectObject, RDW_ALLCHILDREN,
                RDW_INVALIDATE, RDW_UPDATENOW, SRCCOPY,
            },
            UI::WindowsAndMessaging::{
                FindWindowExW, GetClientRect, GetWindowRect, SetForegroundWindow, ShowWindow,
                SW_RESTORE,
            },
        };

        unsafe {
            let _ = ShowWindow(window, SW_RESTORE);
            let _ = SetForegroundWindow(window);
        }
        if !unsafe {
            RedrawWindow(
                window,
                None,
                None,
                RDW_INVALIDATE | RDW_UPDATENOW | RDW_ALLCHILDREN,
            )
        }
        .as_bool()
        {
            return Err("redraw contrast probe failed".to_owned());
        }
        sleep(Duration::from_millis(450));

        let mut window_rect = RECT::default();
        let mut client_rect = RECT::default();
        unsafe { GetWindowRect(window, &mut window_rect) }
            .map_err(|error| format!("read contrast probe window rect: {error}"))?;
        unsafe { GetClientRect(window, &mut client_rect) }
            .map_err(|error| format!("read contrast probe client rect: {error}"))?;
        let width = window_rect.right - window_rect.left;
        let height = window_rect.bottom - window_rect.top;
        if width < 600 || height < 300 {
            return Err(format!(
                "contrast probe window was too small: {width}x{height}"
            ));
        }
        let mut client_origin = POINT::default();
        if !unsafe { ClientToScreen(window, &mut client_origin) }.as_bool() {
            return Err("map contrast probe client origin failed".to_owned());
        }
        let offset_x = client_origin.x - window_rect.left;
        let offset_y = client_origin.y - window_rect.top;
        let button = unsafe {
            FindWindowExW(
                window,
                HWND::default(),
                windows::core::w!("BUTTON"),
                windows::core::w!("Continue"),
            )
        }
        .map_err(|error| format!("find standard button: {error}"))?;
        let input = unsafe {
            FindWindowExW(
                window,
                HWND::default(),
                windows::core::w!("EDIT"),
                windows::core::w!("Sample input"),
            )
        }
        .map_err(|error| format!("find standard input: {error}"))?;
        let mut button_rect = RECT::default();
        let mut input_rect = RECT::default();
        unsafe { GetWindowRect(button, &mut button_rect) }
            .map_err(|error| format!("read standard button rect: {error}"))?;
        unsafe { GetWindowRect(input, &mut input_rect) }
            .map_err(|error| format!("read standard input rect: {error}"))?;

        let source = unsafe { GetDC(HWND::default()) };
        if source.0.is_null() {
            return Err("GetDC screen for contrast probe failed".to_owned());
        }
        let memory = unsafe { CreateCompatibleDC(source) };
        if memory.0.is_null() {
            unsafe {
                let _ = ReleaseDC(HWND::default(), source);
            }
            return Err("CreateCompatibleDC contrast probe failed".to_owned());
        }
        let bitmap = unsafe { CreateCompatibleBitmap(source, width, height) };
        if bitmap.0.is_null() {
            unsafe {
                let _ = DeleteDC(memory);
                let _ = ReleaseDC(HWND::default(), source);
            }
            return Err("CreateCompatibleBitmap contrast probe failed".to_owned());
        }
        let previous = unsafe { SelectObject(memory, bitmap) };

        let result = (|| -> Result<ContrastPixelObservation, String> {
            if unsafe {
                BitBlt(
                    memory,
                    0,
                    0,
                    width,
                    height,
                    source,
                    window_rect.left,
                    window_rect.top,
                    SRCCOPY,
                )
            }
            .is_err()
            {
                return Err("BitBlt contrast probe screenshot failed".to_owned());
            }
            let pixel = |screen_x: i32, screen_y: i32| -> Result<u32, String> {
                let color = unsafe {
                    GetPixel(
                        memory,
                        screen_x - window_rect.left,
                        screen_y - window_rect.top,
                    )
                }
                .0;
                if color == CLR_INVALID {
                    Err(format!(
                        "GetPixel contrast probe failed at ({screen_x},{screen_y})"
                    ))
                } else {
                    Ok(color)
                }
            };

            let window_background = pixel(
                window_rect.left + offset_x + client_rect.right * 4 / 5,
                window_rect.top + offset_y + client_rect.bottom * 4 / 5,
            )?;
            let button_width = button_rect.right - button_rect.left;
            let button_height = button_rect.bottom - button_rect.top;
            let mut button_colors = std::collections::HashMap::<u32, usize>::new();
            for x in (button_rect.left + 2)..(button_rect.right - 2) {
                for y in (button_rect.top + 2)..(button_rect.bottom - 2) {
                    *button_colors.entry(pixel(x, y)?).or_default() += 1;
                }
            }
            let button_surface = button_colors
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(color, _)| color)
                .ok_or_else(|| "standard button screenshot was empty".to_owned())?;
            let input_background = pixel(
                input_rect.left + (input_rect.right - input_rect.left) * 9 / 10,
                input_rect.top + (input_rect.bottom - input_rect.top) / 2,
            )?;
            let mut foreground_pixel = button_surface;
            let mut measured_contrast_ratio = 1.0;
            for x in (button_rect.left + button_width / 10)..(button_rect.right - button_width / 10)
            {
                for y in
                    (button_rect.top + button_height / 8)..(button_rect.bottom - button_height / 8)
                {
                    let candidate = pixel(x, y)?;
                    let candidate_ratio = contrast_ratio(candidate, button_surface);
                    if candidate_ratio > measured_contrast_ratio {
                        foreground_pixel = candidate;
                        measured_contrast_ratio = candidate_ratio;
                    }
                }
            }
            if measured_contrast_ratio < 2.0 {
                return Err(format!(
                    "button foreground was not visible in screenshot: \
                     ratio={measured_contrast_ratio:.3} surface={} foreground={} \
                     window_rect={window_rect:?} button_rect={button_rect:?}",
                    rgb_text(button_surface),
                    rgb_text(foreground_pixel)
                ));
            }
            Ok(ContrastPixelObservation {
                window_background,
                button_surface,
                input_background,
                foreground_pixel,
                contrast_ratio: measured_contrast_ratio,
            })
        })();

        unsafe {
            let _ = SelectObject(memory, previous);
            let _ = DeleteObject(bitmap);
            let _ = DeleteDC(memory);
            let _ = ReleaseDC(HWND::default(), source);
        }
        result
    }

    fn rgb_text(color: u32) -> String {
        format!(
            "#{:02X}{:02X}{:02X}",
            color & 0xff,
            (color >> 8) & 0xff,
            (color >> 16) & 0xff
        )
    }

    fn observation_text(observation: ContrastPixelObservation) -> String {
        format!(
            "background={} button={} input={} foreground={} contrast_ratio={:.3}",
            rgb_text(observation.window_background),
            rgb_text(observation.button_surface),
            rgb_text(observation.input_background),
            rgb_text(observation.foreground_pixel),
            observation.contrast_ratio
        )
    }

    fn pixel_channels_near(first: u32, second: u32, tolerance: u32) -> bool {
        (first & 0xff).abs_diff(second & 0xff) <= tolerance
            && ((first >> 8) & 0xff).abs_diff((second >> 8) & 0xff) <= tolerance
            && ((first >> 16) & 0xff).abs_diff((second >> 16) & 0xff) <= tolerance
    }

    /// コントラストテーマを製品登録する前の実機ゲート。
    ///
    /// 設定の readback では合格にしない。別プロセスの標準 Win32 コントロール窓を
    /// PrintWindow で撮り、同じ HWND のピクセル差とコントラスト比を測る。
    /// 復元後はさらに別の新規プロセス窓で開始前のピクセルへ戻ったかを確認する。
    #[test]
    #[ignore = "temporarily enables high contrast and proves separate-process pixels plus exact restore"]
    fn high_contrast_changes_separate_process_pixels_and_restores() {
        use windows::Win32::UI::Accessibility::HCF_HIGHCONTRASTON;

        let original = match read_high_contrast_snapshot() {
            Ok(value) => value,
            Err(error) => {
                println!("EVIDENCE: contrast_trial measured=false reason=initial_state_unavailable detail={error}");
                panic!("high-contrast measurement could not start");
            }
        };
        if original.flags & HCF_HIGHCONTRASTON.0 != 0 {
            println!(
                "EVIDENCE: contrast_trial measured=false reason=already_enabled flags={} scheme={:?}",
                original.flags, original.scheme_name
            );
            return;
        }
        let before_window = match ContrastProbeProcess::start() {
            Ok(child) => child,
            Err(error) => {
                println!(
                    "EVIDENCE: contrast_trial measured=false reason=child_window_unavailable detail={error}"
                );
                panic!("separate-process contrast probe was unavailable");
            }
        };
        let before = match observe_contrast_probe_pixels(before_window.window) {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "EVIDENCE: contrast_trial measured=false reason=baseline_pixels_unavailable detail={error}"
                );
                panic!("baseline contrast pixels were unavailable");
            }
        };

        let mut guard = HighContrastRestoreGuard::new(original.clone());
        let mut requested = original.clone();
        requested.flags |= HCF_HIGHCONTRASTON.0;
        if let Err(error) = write_high_contrast_snapshot(&requested) {
            println!(
                "EVIDENCE: contrast_trial measured=false reason=apply_rejected before=\"{}\" detail={error}",
                observation_text(before)
            );
            panic!("high-contrast apply was rejected");
        }
        let immediate_applied_state = match read_high_contrast_snapshot() {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "EVIDENCE: contrast_trial measured=false reason=applied_state_unavailable before=\"{}\" detail={error}",
                    observation_text(before)
                );
                panic!("applied high-contrast state could not be guarded");
            }
        };
        guard.applied = Some(immediate_applied_state);
        sleep(Duration::from_millis(1_500));
        let applied_state = match read_high_contrast_snapshot() {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "EVIDENCE: contrast_trial measured=false reason=settled_applied_state_unavailable before=\"{}\" detail={error}",
                    observation_text(before)
                );
                panic!("settled applied high-contrast state could not be guarded");
            }
        };
        guard.applied = Some(applied_state.clone());

        let mut applied_result = Err("applied screenshot was not attempted".to_owned());
        for _ in 0..8 {
            applied_result = observe_contrast_probe_pixels(before_window.window);
            if applied_result.is_ok() {
                break;
            }
            sleep(Duration::from_millis(250));
        }
        let applied = match applied_result {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "EVIDENCE: contrast_trial measured=false reason=applied_pixels_unavailable before=\"{}\" spi_enabled={} detail={error}",
                    observation_text(before),
                    applied_state.flags & HCF_HIGHCONTRASTON.0 != 0
                );
                panic!("applied contrast pixels were unavailable");
            }
        };

        let restored_state = match guard.restore_if_unchanged() {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "EVIDENCE: contrast_trial measured=false reason=restore_failed before=\"{}\" applied=\"{}\" spi_enabled={} detail={error}",
                    observation_text(before),
                    observation_text(applied),
                    applied_state.flags & HCF_HIGHCONTRASTON.0 != 0
                );
                panic!("high-contrast state could not be restored safely");
            }
        };
        // 有効化側と同じく、Windows のテーマ遷移が落ち着いてから別プロセスを描く。
        // 判定値は緩めず、遷移中の一時色を復元後の色として採らないための待機だけを置く。
        sleep(Duration::from_millis(1_500));
        drop(before_window);

        let restored_window = match ContrastProbeProcess::start() {
            Ok(child) => child,
            Err(error) => {
                println!(
                    "EVIDENCE: contrast_trial measured=false reason=restored_child_unavailable before=\"{}\" applied=\"{}\" detail={error}",
                    observation_text(before),
                    observation_text(applied)
                );
                panic!("restored separate-process contrast probe was unavailable");
            }
        };
        let restored = match observe_contrast_probe_pixels(restored_window.window) {
            Ok(value) => value,
            Err(error) => {
                println!(
                    "EVIDENCE: contrast_trial measured=false reason=restored_pixels_unavailable before=\"{}\" applied=\"{}\" detail={error}",
                    observation_text(before),
                    observation_text(applied)
                );
                panic!("restored contrast pixels were unavailable");
            }
        };

        let pixels_changed = before.window_background != applied.window_background
            || before.button_surface != applied.button_surface
            || before.input_background != applied.input_background
            || before.foreground_pixel != applied.foreground_pixel;
        let restored_pixels =
            pixel_channels_near(before.window_background, restored.window_background, 3)
                && pixel_channels_near(before.button_surface, restored.button_surface, 3)
                && pixel_channels_near(before.input_background, restored.input_background, 3)
                && pixel_channels_near(before.foreground_pixel, restored.foreground_pixel, 3)
                && (before.contrast_ratio - restored.contrast_ratio).abs() <= 0.15;
        let spi_enabled = applied_state.flags & HCF_HIGHCONTRASTON.0 != 0;
        let snapshot_restored = restored_state == original;
        // **コントラスト比が上がることを合格条件にしない。**
        //
        // 実測すると、高コントラスト（黒）は既定の白背景より比が「下がる」。
        //   before  #FFFFFF / #000000  ratio=18.427
        //   applied #202020 / #FFFFFF  ratio=16.293
        // 黒地に白は、白地に黒より数値上のコントラストが低い。
        // 「高コントラスト」という名前から上がると思い込んでいた。**測ったら逆だった。**
        //
        // この機能が約束できるのは「見え方が変わること」と「元へ正確に戻ること」まで。
        // 読みやすくなるかは本人にしか分からない。比の向きを合格条件にすると、
        // 正しく動いているものを不合格にする。比は数値として出すだけにする。
        let measured = pixels_changed && restored_pixels && spi_enabled && snapshot_restored;
        let reason = if !spi_enabled {
            "spi_did_not_enable"
        } else if !pixels_changed {
            "pixels_unchanged"
        } else if !restored_pixels {
            "restored_pixels_differ"
        } else if !snapshot_restored {
            "snapshot_not_restored"
        } else {
            "separate_process_pixels_changed_and_restored"
        };
        println!(
            "EVIDENCE: contrast_trial measured={measured} reason={reason} before=\"{}\" applied=\"{}\" restored=\"{}\" spi_before={original:?} spi_applied={applied_state:?} spi_restored={restored_state:?}",
            observation_text(before),
            observation_text(applied),
            observation_text(restored)
        );
        assert!(
            measured,
            "high-contrast implementation gate failed: {reason}"
        );
    }
}
