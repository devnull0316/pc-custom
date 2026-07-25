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

#[cfg(not(windows))]
pub fn observe_taskbar_layout() -> WindowsResult<TaskbarLayoutObservation> {
    Err(WindowsError::unsupported("observe taskbar layout"))
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
}
