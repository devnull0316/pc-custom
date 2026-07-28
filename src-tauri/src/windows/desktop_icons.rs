//! デスクトップのアイコン配置の読み取り。
//!
//! **ここは動かさない。読むだけ。**
//!
//! # なぜ読み取りだけなのか
//!
//! `docs/RESEARCH_FEATURES.md` の「6. デスクトップアイコン配置の保存・復元」は
//! **次段**の判定で、いちばん危ないところを「再起動や OneDrive の移動をまたいだ
//! 項目の取り違え」と書いている。別のアイコンを動かしてしまえば、
//! 利用者から見れば配置が壊れたのと同じ。
//!
//! そこで最初にやることは「どのアイコンがどこにあるか」を読めるかどうかと、
//! **その識別子が本当に同じものを指し続けるか**を測れるようにすること。
//! 動かすのはそのあと。
//!
//! # 名前を持ち出さない
//!
//! デスクトップのアイコン名は個人情報を含む（「確定申告2026.xlsx」など）。
//! 位置と対応づける識別子は、Shell が返す PIDL のバイト列を
//! **ハッシュにしてから**持つ。ログにも画面にも名前は出さない。
//!
//! PIDL のバイト列が再起動をまたいで同じであるかは、**まだ分かっていない。**
//! 分かっていないので、ここでは「識別子」とだけ呼び、永続 ID とは呼ばない。
//!
//! # 使う手段
//!
//! 文書化された `IShellWindows` → `IServiceProvider` → `IShellBrowser` →
//! `IShellView` → `IFolderView` だけ。未文書の ListView メッセージは使わない。
//! Microsoft 自身が、非文書化 ListView message は Windows 10 1809 で壊れたと書いている。

use super::{WindowsError, WindowsErrorKind, WindowsResult};

/// アイコン1つ分。**名前は持たない。**
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopIcon {
    /// PIDL のバイト列から作った識別子。名前そのものではない。
    pub identity: String,
    pub x: i32,
    pub y: i32,
}

/// ある時点のデスクトップ。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopIconLayout {
    /// 自動整列が有効。**有効な間は位置を指定しても Windows が並べ直す。**
    pub auto_arrange: bool,
    /// アイコンの間隔。整列の判断材料。
    pub spacing_x: i32,
    pub spacing_y: i32,
    pub icons: Vec<DesktopIcon>,
    /// 位置を読めなかったアイコンの数。**0 件に混ぜない。**
    pub unreadable_count: usize,
}

impl DesktopIconLayout {
    /// 同じ識別子の集合か。並び順は問わない。
    pub fn same_identities(&self, other: &Self) -> bool {
        let mut mine: Vec<&str> = self
            .icons
            .iter()
            .map(|icon| icon.identity.as_str())
            .collect();
        let mut theirs: Vec<&str> = other
            .icons
            .iter()
            .map(|icon| icon.identity.as_str())
            .collect();
        mine.sort_unstable();
        theirs.sort_unstable();
        mine == theirs
    }

    /// 位置が変わったアイコンの識別子。識別子の集合が違うときは `None`。
    ///
    /// **集合が違うのに位置だけ比べない。** 増減があれば別の話として扱う。
    pub fn moved_since(&self, earlier: &Self) -> Option<Vec<String>> {
        if !self.same_identities(earlier) {
            return None;
        }
        let mut moved = Vec::new();
        for icon in &self.icons {
            let before = earlier
                .icons
                .iter()
                .find(|other| other.identity == icon.identity)?;
            if before.x != icon.x || before.y != icon.y {
                moved.push(icon.identity.clone());
            }
        }
        Some(moved)
    }
}

/// PIDL のバイト列から表示しても差し支えない識別子を作る。
///
/// 名前をそのまま持たないための一方向変換。**戻せない。**
fn identity_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    // 全部は要らない。取り違えを避けるのに十分な長さだけ持つ。
    digest.iter().take(8).fold(String::new(), |mut text, byte| {
        use std::fmt::Write;
        let _ = write!(text, "{byte:02x}");
        text
    })
}

#[cfg(windows)]
mod imp {
    use super::{identity_of, DesktopIcon, DesktopIconLayout, WindowsError, WindowsErrorKind};
    use crate::windows::WindowsResult;
    use windows::{
        core::Interface,
        Win32::{
            Foundation::POINT,
            System::Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IServiceProvider,
                CLSCTX_ALL, COINIT_APARTMENTTHREADED,
            },
            UI::Shell::{
                Common::ITEMIDLIST, IFolderView, IShellBrowser, IShellView, IShellWindows,
                ShellWindows, SVGIO_ALLVIEW, SWC_DESKTOP, SWFO_NEEDDISPATCH,
            },
        },
    };

    /// `IShellBrowser` を取り出すためのサービス ID。文書化されている値。
    const SID_S_TOP_LEVEL_BROWSER: windows::core::GUID =
        windows::core::GUID::from_u128(0x4C96BE40_915C_11CF_99D3_00AA004AE837);

    fn com_error(operation: &'static str, error: windows::core::Error) -> WindowsError {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            operation,
            Some(i64::from(error.code().0)),
        )
    }

    /// Shell の COM は STA を要求する。専用スレッドで初期化して、そこで完結させる。
    pub fn read() -> WindowsResult<DesktopIconLayout> {
        std::thread::Builder::new()
            .name("pc-custom-desktop-icons".to_owned())
            .spawn(read_on_com_thread)
            .map_err(|error| WindowsError::io("spawn desktop icon read thread", &error))?
            .join()
            .map_err(|_| {
                WindowsError::new(
                    WindowsErrorKind::ApiFailure,
                    "join desktop icon read thread",
                    None,
                )
            })?
    }

    fn read_on_com_thread() -> WindowsResult<DesktopIconLayout> {
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if initialized.is_err() {
            return Err(WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "CoInitializeEx for desktop icons",
                Some(i64::from(initialized.0)),
            ));
        }
        let outcome = read_layout();
        unsafe { CoUninitialize() };
        outcome
    }

    fn folder_view() -> WindowsResult<IFolderView> {
        let shell_windows: IShellWindows =
            unsafe { CoCreateInstance(&ShellWindows, None, CLSCTX_ALL) }
                .map_err(|error| com_error("CoCreateInstance ShellWindows", error))?;

        // CSIDL_DESKTOP を VARIANT の I4 として渡す。
        let location = windows::core::VARIANT::from(0i32);
        let empty = windows::core::VARIANT::default();
        let mut hwnd = 0i32;
        let dispatch = unsafe {
            shell_windows.FindWindowSW(&location, &empty, SWC_DESKTOP, &mut hwnd, SWFO_NEEDDISPATCH)
        }
        .map_err(|error| com_error("IShellWindows::FindWindowSW", error))?;

        let provider: IServiceProvider = dispatch
            .cast()
            .map_err(|error| com_error("desktop dispatch to IServiceProvider", error))?;
        let browser: IShellBrowser = unsafe { provider.QueryService(&SID_S_TOP_LEVEL_BROWSER) }
            .map_err(|error| com_error("IServiceProvider::QueryService", error))?;
        let view: IShellView = unsafe { browser.QueryActiveShellView() }
            .map_err(|error| com_error("IShellBrowser::QueryActiveShellView", error))?;
        view.cast()
            .map_err(|error| com_error("IShellView to IFolderView", error))
    }

    /// PIDL の長さを数える。末尾は長さ 0 の要素で終わる。
    ///
    /// 識別子はこのバイト列から作るので、**長さを間違えると別物が同じ識別子になる。**
    fn pidl_len(pidl: *const ITEMIDLIST) -> usize {
        let mut cursor = pidl.cast::<u8>();
        let mut total = 0usize;
        loop {
            // 先頭 2 バイトがその要素の長さ（自身を含む）。
            let size = unsafe { u16::from_le_bytes([*cursor, *cursor.add(1)]) } as usize;
            if size == 0 {
                // 終端の 2 バイトも識別子に含める。
                return total + 2;
            }
            total += size;
            cursor = unsafe { cursor.add(size) };
            // 壊れた PIDL で無限に歩かない。
            if total > 64 * 1024 {
                return total;
            }
        }
    }

    fn read_layout() -> WindowsResult<DesktopIconLayout> {
        let view = folder_view()?;

        // `GetAutoArrange` は有効なら S_OK、無効なら S_FALSE。エラーではない。
        let auto_arrange = unsafe { view.GetAutoArrange() }.is_ok();

        let mut spacing = POINT::default();
        let _ = unsafe { view.GetSpacing(&mut spacing) };

        let count = unsafe { view.ItemCount(SVGIO_ALLVIEW) }
            .map_err(|error| com_error("IFolderView::ItemCount", error))?;

        let mut icons = Vec::new();
        let mut unreadable = 0usize;
        for index in 0..count {
            let Ok(pidl) = (unsafe { view.Item(index) }) else {
                unreadable += 1;
                continue;
            };
            if pidl.is_null() {
                unreadable += 1;
                continue;
            }
            let length = pidl_len(pidl);
            let bytes = unsafe { std::slice::from_raw_parts(pidl.cast::<u8>(), length) };
            let identity = identity_of(bytes);
            let position = unsafe { view.GetItemPosition(pidl) };
            // Shell が確保した PIDL は必ず解放する。
            unsafe { CoTaskMemFree(Some(pidl.cast())) };
            match position {
                Ok(point) => icons.push(DesktopIcon {
                    identity,
                    x: point.x,
                    y: point.y,
                }),
                // 位置が読めないものを (0,0) として並べない。数だけ残す。
                Err(_) => unreadable += 1,
            }
        }

        Ok(DesktopIconLayout {
            auto_arrange,
            spacing_x: spacing.x,
            spacing_y: spacing.y,
            icons,
            unreadable_count: unreadable,
        })
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{DesktopIconLayout, WindowsError, WindowsResult};

    pub fn read() -> WindowsResult<DesktopIconLayout> {
        Err(WindowsError::unsupported("read desktop icon layout"))
    }
}

/// いまのデスクトップのアイコン配置を読む。**動かさない。**
pub fn read_desktop_icon_layout() -> WindowsResult<DesktopIconLayout> {
    imp::read()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon(identity: &str, x: i32, y: i32) -> DesktopIcon {
        DesktopIcon {
            identity: identity.to_owned(),
            x,
            y,
        }
    }

    fn layout(icons: Vec<DesktopIcon>) -> DesktopIconLayout {
        DesktopIconLayout {
            auto_arrange: false,
            spacing_x: 75,
            spacing_y: 100,
            icons,
            unreadable_count: 0,
        }
    }

    #[test]
    fn the_identity_never_contains_the_original_bytes() {
        // 名前を含むバイト列から作っても、元の文字が残らないこと。
        let raw = "確定申告2026.xlsx".as_bytes();
        let identity = identity_of(raw);
        assert!(!identity.contains("xlsx"));
        assert!(identity.chars().all(|c| c.is_ascii_hexdigit()));
        // 同じ入力なら同じ識別子。
        assert_eq!(identity, identity_of(raw));
        // 1バイト違えば別の識別子。
        let mut altered = raw.to_vec();
        altered[0] ^= 0x01;
        assert_ne!(identity, identity_of(&altered));
    }

    #[test]
    fn a_moved_icon_is_reported_and_a_still_one_is_not() {
        let before = layout(vec![icon("aa", 0, 0), icon("bb", 75, 0)]);
        let after = layout(vec![icon("aa", 0, 0), icon("bb", 150, 0)]);
        assert_eq!(after.moved_since(&before), Some(vec!["bb".to_owned()]));
    }

    #[test]
    fn nothing_moving_reports_an_empty_list_not_a_failure() {
        let one = layout(vec![icon("aa", 0, 0)]);
        assert_eq!(one.moved_since(&one), Some(Vec::new()));
    }

    #[test]
    fn an_added_or_removed_icon_stops_the_comparison_entirely() {
        // 増減があるのに「位置だけ変わった」とは言えない。
        let before = layout(vec![icon("aa", 0, 0)]);
        let after = layout(vec![icon("aa", 0, 0), icon("bb", 75, 0)]);
        assert_eq!(after.moved_since(&before), None);
        assert_eq!(before.moved_since(&after), None);
    }

    #[test]
    fn a_replaced_icon_stops_the_comparison_even_at_the_same_count() {
        let before = layout(vec![icon("aa", 0, 0)]);
        let after = layout(vec![icon("cc", 0, 0)]);
        assert_eq!(after.moved_since(&before), None);
    }

    #[test]
    fn ordering_alone_does_not_make_two_desktops_different() {
        let one = layout(vec![icon("aa", 0, 0), icon("bb", 75, 0)]);
        let other = layout(vec![icon("bb", 75, 0), icon("aa", 0, 0)]);
        assert!(one.same_identities(&other));
        assert_eq!(other.moved_since(&one), Some(Vec::new()));
    }

    /// 実機のデスクトップを読む。**何も動かさない。**
    ///
    /// 位置が読めたものと読めなかったものを分けて出す。
    /// 読めなかったものを 0 件扱いにすると「全部読めた」ように見えてしまう。
    #[test]
    #[ignore = "実機のデスクトップを読む"]
    fn read_the_real_desktop_layout() {
        let layout = match read_desktop_icon_layout() {
            Ok(layout) => layout,
            Err(error) => {
                // 読めなかったことを「アイコンが無い」と言わない。
                println!("EVIDENCE: desktop_icons unreadable error={error:?}");
                return;
            }
        };
        println!(
            "EVIDENCE: desktop_icons count={} unreadable={} auto_arrange={} spacing=({},{})",
            layout.icons.len(),
            layout.unreadable_count,
            layout.auto_arrange,
            layout.spacing_x,
            layout.spacing_y
        );
        for icon in layout.icons.iter().take(5) {
            println!(
                "EVIDENCE: desktop_icons item={} at ({},{})",
                icon.identity, icon.x, icon.y
            );
        }

        // 続けて2回読めば、識別子も位置も同じはず。
        // ここが揺れるなら、この識別子は配置の記録には使えない。
        let again = read_desktop_icon_layout().expect("read the desktop twice");
        println!(
            "EVIDENCE: desktop_icons same_identities={} moved={:?}",
            layout.same_identities(&again),
            again.moved_since(&layout)
        );
        assert!(
            layout.same_identities(&again),
            "同じ瞬間に集合が変わらないこと"
        );
        assert_eq!(again.moved_since(&layout), Some(Vec::new()));
    }
}
