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
        CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants, UIA_NamePropertyId,
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
            WindowsError::new(WindowsErrorKind::InvalidData, "clock element not found", None)
        })
}

#[cfg(not(windows))]
pub fn observe_taskbar_clock_text() -> WindowsResult<String> {
    Err(WindowsError::unsupported("observe taskbar clock"))
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
        let _ = EnumWindows(
            Some(callback),
            LPARAM(&mut search as *mut Search as isize),
        );
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
    #[test]
    #[ignore = "実機のタスクバーを一時的に変更する証拠取得用"]
    fn taskbar_alignment_write_actually_moves_the_start_button() {
        use crate::backup::{
            prepare_registry_backup, read_registry_state, restore_registry_backup, RegistryTarget,
            RegistryRestoreOutcome,
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
        let original_value = current
            .value_existed
            .then(|| u32::from_le_bytes(current.raw_bytes[..4].try_into().unwrap_or([0; 4])))
            .unwrap_or(1);
        // 0 = 左寄せ, 1 = 中央寄せ。いまと反対側へ動かす。
        let flipped = if original_value == 0 { 1u32 } else { 0u32 };

        let backup = prepare_registry_backup(
            target,
            REG_DWORD,
            flipped.to_le_bytes().to_vec(),
            1,
            26_200,
        )
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
            moved.map(|m| m.start_center_ratio()).unwrap_or(baseline.start_center_ratio()),
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
        assert_eq!(final_state, backup.original, "値・型・有無まで元どおりに戻す");
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
        let probe: u32 = if before.red > 128 { 0xC4_20_60_A0 } else { 0xC4_C0_50_20 };
        let target = RegistryTarget::current_user_64(DWM_SUBKEY, VALUE);
        let backup = prepare_registry_backup(target, REG_DWORD, probe.to_le_bytes().to_vec(), 1, 26_200)
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
        assert_eq!(final_state, backup.original, "値・型・有無まで元どおりに戻す");
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
        println!("before: present={before_present} value_exists={}", current.value_existed);

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
            if changed { "反映される" } else { "反映されなかった" }
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
            let taskbar: HWND = FindWindowW(windows::core::w!("Shell_TrayWnd"), None).expect("taskbar");
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).expect("uia");
            let root: IUIAutomationElement = automation.ElementFromHandle(taskbar).expect("root");
            let condition = automation.CreateTrueCondition().expect("cond");
            let all = root.FindAll(TreeScope_Descendants, &condition).expect("all");
            let count = all.Length().unwrap_or(0);
            println!("要素数: {count}");
            for index in 0..count {
                if let Ok(element) = all.GetElement(index) {
                    let name = element.CurrentName().map(|n| n.to_string()).unwrap_or_default();
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
            let _ = std::fs::OpenOptions::new().read(true).share_mode(3).open(&hidden);
        }

        // Explorerで開く。
        let path = HSTRING::from(dir.as_os_str());
        unsafe {
            ShellExecuteW(None, windows::core::w!("open"), &path, None, None, SW_SHOWNORMAL);
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
        println!("before: hidden_file_listed={before_visible} items={}", before_names.len());

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
                ShellExecuteW(None, windows::core::w!("open"), &path, None, None, SW_SHOWNORMAL);
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
            (false, Some(true)) => println!(
                "EVIDENCE: 開いている窓には反映されないが、新しく開いた窓には反映される"
            ),
            (false, Some(false)) => println!("EVIDENCE: 新しく開いた窓にも反映されない"),
            (false, None) => println!("EVIDENCE: 開いている窓には反映されず、新窓の確認はできなかった"),
        }
    }
}
