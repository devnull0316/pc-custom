//! Windows の設定アプリの該当ページを開く導線。
//!
//! 実測（`windows::ui_probe` の往復検証）で、第三者アプリからの書き込みは Explorer を
//! 再起動しない限りタスクバー等のUIへ反映されないことが分かった。そのため該当Actionは
//! 変更を行わない。ただし「表示するだけの行き止まり」にはせず、**Windows自身の設定画面**へ
//! 案内する。ユーザーはそこで確実に変更できる。
//!
//! 安全契約:
//! - 開けるのは、このファイル内の固定表にある `ms-settings:` URI だけ。
//! - 任意のURL・パス・コマンドは受け付けない。ユーザー入力を連結しない。
//! - 起動は ShellExecuteW に定数文字列を渡すだけで、シェルを経由しない。

use crate::{
    action::ActionId,
    error::{CoreError, CoreResult},
};

/// ActionId → Windows設定アプリのページ。存在しないものは None（UIはボタンを出さない）。
pub const fn settings_page_for(action: ActionId) -> Option<&'static str> {
    Some(match action {
        // タスクバー・スタート・検索
        ActionId::TaskbarSearchMode
        | ActionId::TaskbarAlignment
        | ActionId::TaskbarButtonGrouping
        | ActionId::TaskbarFlashing
        | ActionId::TaskbarShareWindow
        | ActionId::TaskbarShowDesktop
        | ActionId::TaskbarMultiMonitor
        | ActionId::TaskbarMultiMonitorMode
        | ActionId::TaskbarSecondaryButtonGrouping
        | ActionId::TaskbarTaskView
        | ActionId::TaskbarWidgets => "ms-settings:taskbar",
        ActionId::StartLayout
        | ActionId::StartRecommendations
        | ActionId::StartShowAllPins
        | ActionId::StartRecentApps => "ms-settings:personalization-start",
        // 色・見た目
        ActionId::AppearanceAccentStartTaskbar
        | ActionId::AppearanceAccentTitleBars
        | ActionId::AppearanceAutoAccent
        | ActionId::AppearanceAccentColorCheck
        | ActionId::AppearanceWindowColor
        | ActionId::ThemeColorMode
        | ActionId::AppearanceTransparency => "ms-settings:personalization-colors",
        ActionId::AppearanceTaskbarAnimations => "ms-settings:easeofaccess-visualeffects",
        // 通知
        ActionId::NotificationsToastBanners
        | ActionId::NotificationsUsbErrors
        | ActionId::NotificationsWeakCharger => "ms-settings:notifications",
        // 入力
        ActionId::InputAutocorrect
        | ActionId::InputDoubleSpacePeriod
        | ActionId::InputAutoShift
        | ActionId::InputMultilingualSuggestions => "ms-settings:devices-typing",
        ActionId::InputVoiceTypingKey => "ms-settings:privacy-speech",
        // ゲーム・デバイス
        ActionId::GamesGameMode => "ms-settings:gaming-gamemode",
        ActionId::GamesControllerGameBar => "ms-settings:gaming-gamebar",
        ActionId::DevicesAutoplay => "ms-settings:autoplay",
        // 電源・ストレージ
        ActionId::PowerActiveSchemeCheck
        | ActionId::PowerActiveSchemeSwitch
        | ActionId::SessionPreventSleep => "ms-settings:powersleep",
        ActionId::StorageFreeSpaceCheck | ActionId::StorageTempFilesCheck => {
            "ms-settings:storagesense"
        }
        ActionId::SetupStartupInventory => "ms-settings:startupapps",
        // Explorer の各設定は「フォルダーオプション」側にあり、設定アプリの対応ページがない。
        _ => return None,
    })
}

#[cfg(windows)]
pub fn open_settings_page(action: ActionId) -> CoreResult<&'static str> {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let page = settings_page_for(action).ok_or_else(|| {
        CoreError::invalid_request("このAction向けのWindows設定ページはありません。")
    })?;
    // page は上の固定表由来の定数のみ。ユーザー入力は一切混ざらない。
    let uri = HSTRING::from(page);
    let result = unsafe {
        ShellExecuteW(
            None,
            windows::core::w!("open"),
            &uri,
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW は成功時に 32 より大きい値を返す。
    if result.0 as isize <= 32 {
        return Err(CoreError::invalid_request(
            "Windowsの設定アプリを開けませんでした。",
        ));
    }
    Ok(page)
}

#[cfg(not(windows))]
pub fn open_settings_page(_action: ActionId) -> CoreResult<&'static str> {
    Err(CoreError::invalid_request("PCカスタムはWindows 11専用です。"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mapped_page_is_a_fixed_ms_settings_uri() {
        // 任意のURLやコマンドが混ざらないこと。スキームは ms-settings: のみ。
        let mut mapped = 0usize;
        for action in ActionId::ALL {
            if let Some(page) = settings_page_for(action) {
                mapped += 1;
                assert!(
                    page.starts_with("ms-settings:"),
                    "{page} は ms-settings スキームであること"
                );
                assert!(!page.contains(' '), "空白を含まない: {page}");
                assert!(!page.contains('&'), "引数連結を含まない: {page}");
                assert!(!page.contains('"'), "引用符を含まない: {page}");
                assert!(page.len() < 64, "境界内の長さ: {page}");
            }
        }
        assert!(mapped >= 20, "主要な設定へ案内できること: {mapped}件");
    }

    #[test]
    fn explorer_settings_have_no_settings_app_page() {
        // 設定アプリに該当ページが無いものは、案内を出さない（嘘の導線を作らない）。
        assert!(settings_page_for(ActionId::ExplorerShowExtensions).is_none());
        assert!(settings_page_for(ActionId::ExplorerLaunchTarget).is_none());
    }

    /// UIへ届く表示情報に settings_page が載ること（フロントのボタン表示条件）。
    #[test]
    fn presentation_exposes_the_settings_page_for_guided_candidates() {
        use crate::action::ACTION_REGISTRY;
        use crate::compatibility::{CompatibilityDecision, CompatibilityMode};
        let decision = CompatibilityDecision {
            mode: CompatibilityMode::TestedDetectOnly,
            rollback_across_unknown_build: false,
            evidence_id: "test",
        };
        let action = ACTION_REGISTRY
            .get(ActionId::TaskbarAlignment)
            .expect("registered");
        let view = crate::presentation::action_presentation(action.metadata(), decision, None);
        assert_eq!(view.settings_page.as_deref(), Some("ms-settings:taskbar"));
    }

    #[test]
    fn guided_taskbar_candidates_all_have_a_way_forward() {
        // 実測でUIへ反映されないと分かった候補ほど、Windows側の導線が要る。
        for action in [
            ActionId::TaskbarAlignment,
            ActionId::TaskbarSearchMode,
            ActionId::StartRecommendations,
            ActionId::NotificationsToastBanners,
            ActionId::GamesGameMode,
        ] {
            assert!(
                settings_page_for(action).is_some(),
                "{action:?} にWindows設定への導線があること"
            );
        }
    }
}
