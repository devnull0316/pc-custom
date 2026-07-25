use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BroadcastReport {
    pub shell_change_notified: bool,
    pub setting_change_acknowledged: bool,
    pub setting_change_error_code: Option<i64>,
}

/// Non-destructive Explorer notification. It never terminates or restarts Explorer.
#[cfg(windows)]
pub fn notify_explorer_settings_changed() -> BroadcastReport {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    // 関連付け変更の通知だけでは、開いているExplorerはフォルダーオプションを読み直さない。
    // 「ShellState」を伴う WM_SETTINGCHANGE が、その再読み込みを促す文書化された合図。
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
    let acknowledged = unsafe {
        let payload = HSTRING::from("ShellState");
        let result = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(payload.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            2_000,
            None,
        );
        let _: HWND = HWND_BROADCAST;
        result.0 != 0
    };
    BroadcastReport {
        shell_change_notified: true,
        setting_change_acknowledged: acknowledged,
        setting_change_error_code: None,
    }
}

#[cfg(not(windows))]
pub fn notify_explorer_settings_changed() -> BroadcastReport {
    BroadcastReport {
        shell_change_notified: false,
        setting_change_acknowledged: false,
        setting_change_error_code: None,
    }
}

/// Bounded theme notification plus the same non-destructive shell broadcast.
#[cfg(windows)]
pub fn notify_theme_changed() -> BroadcastReport {
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_SETTINGCHANGE,
    };

    let section: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
    let mut message_result = 0usize;
    let sent = unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(section.as_ptr() as isize),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            2_000,
            Some(&mut message_result),
        )
    };
    let acknowledged = sent.0 != 0;
    let error_code = if acknowledged {
        None
    } else {
        std::io::Error::last_os_error()
            .raw_os_error()
            .map(i64::from)
    };
    let mut report = notify_explorer_settings_changed();
    report.setting_change_acknowledged = acknowledged;
    report.setting_change_error_code = error_code;
    report
}

#[cfg(not(windows))]
pub fn notify_theme_changed() -> BroadcastReport {
    notify_explorer_settings_changed()
}
