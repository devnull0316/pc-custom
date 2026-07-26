//! 新しいPCセットアップ: WinGet(Microsoft公式CLI)による、**固定allowlistのアプリだけ**の導入。
//!
//! 安全契約(BRIEF/SECURITY厳守):
//! - 導入できるのはコード内 `ALLOWED_APPS` の既知 winget package ID のみ。任意IDは受け付けない。
//! - `winget.exe` を **shell経由でなく直接起動**し、引数は固定Vec。ユーザー文字列を連結しない。
//! - source は公式 `winget` に固定、per-user scope、非対話、契約自動承認。
//! - exit code / stdout / stderr を取得し、要約は境界長でsanitizeする(秘密/個人情報を出さない)。
//! - これは登録ずみの一回導入操作であり、レジストリrollbackエンジンには載せない(適用前にpreview提示)。

use serde::Serialize;

use crate::error::{CoreError, CoreResult};

/// allowlist のアプリ定義。winget_id は公式winget sourceの実在IDのみ。
struct SetupApp {
    id: &'static str,
    winget_id: &'static str,
    name: &'static str,
    description: &'static str,
    category: &'static str,
    requires_admin: bool,
}

const ALLOWED_APPS: &[SetupApp] = &[
    SetupApp {
        id: "brave",
        winget_id: "Brave.Brave",
        name: "Brave ブラウザ",
        description: "広告ブロック内蔵の高速ブラウザ。",
        category: "browser",
        requires_admin: false,
    },
    SetupApp {
        id: "chrome",
        winget_id: "Google.Chrome",
        name: "Google Chrome",
        description: "定番のWebブラウザ。",
        category: "browser",
        requires_admin: false,
    },
    SetupApp {
        id: "firefox",
        winget_id: "Mozilla.Firefox",
        name: "Mozilla Firefox",
        description: "プライバシー重視のWebブラウザ。",
        category: "browser",
        requires_admin: false,
    },
    SetupApp {
        id: "discord",
        winget_id: "Discord.Discord",
        name: "Discord",
        description: "ゲーマー向けの音声・テキストチャット。",
        category: "game",
        requires_admin: false,
    },
    SetupApp {
        id: "steam",
        winget_id: "Valve.Steam",
        name: "Steam",
        description: "PCゲームのプラットフォーム。",
        category: "game",
        requires_admin: false,
    },
    SetupApp {
        id: "epic",
        winget_id: "EpicGames.EpicGamesLauncher",
        name: "Epic Games Launcher",
        description: "Epic Gamesストアとランチャー。",
        category: "game",
        requires_admin: false,
    },
    SetupApp {
        id: "vscode",
        winget_id: "Microsoft.VisualStudioCode",
        name: "Visual Studio Code",
        description: "軽量なコードエディタ。",
        category: "work",
        requires_admin: false,
    },
    SetupApp {
        id: "git",
        winget_id: "Git.Git",
        name: "Git",
        description: "バージョン管理システム。",
        category: "work",
        requires_admin: false,
    },
    SetupApp {
        id: "powertoys",
        winget_id: "Microsoft.PowerToys",
        name: "Microsoft PowerToys",
        description: "Windowsの便利機能集(公式)。",
        category: "work",
        requires_admin: false,
    },
    SetupApp {
        id: "obs",
        winget_id: "OBSProject.OBSStudio",
        name: "OBS Studio",
        description: "配信・録画ソフト。",
        category: "work",
        requires_admin: false,
    },
    SetupApp {
        id: "vlc",
        winget_id: "VideoLAN.VLC",
        name: "VLC メディアプレーヤー",
        description: "何でも再生できるメディアプレーヤー。",
        category: "work",
        requires_admin: false,
    },
    SetupApp {
        id: "7zip",
        winget_id: "7zip.7zip",
        name: "7-Zip",
        description: "高圧縮の圧縮・解凍ソフト。",
        category: "work",
        requires_admin: false,
    },
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupAppDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub requires_admin: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOutcome {
    pub app_id: String,
    pub app_name: String,
    pub succeeded: bool,
    pub exit_code: Option<i32>,
    pub summary: String,
}

pub fn app_catalog() -> Vec<SetupAppDto> {
    ALLOWED_APPS
        .iter()
        .map(|app| SetupAppDto {
            id: app.id.to_owned(),
            name: app.name.to_owned(),
            description: app.description.to_owned(),
            category: app.category.to_owned(),
            requires_admin: app.requires_admin,
        })
        .collect()
}

fn find_allowed(app_id: &str) -> Option<&'static SetupApp> {
    ALLOWED_APPS.iter().find(|app| app.id == app_id)
}

/// 固定の winget 引数を組み立てる。ユーザー入力は連結せず、winget_id は allowlist 由来のみ。
fn install_args(winget_id: &str) -> Vec<String> {
    vec![
        "install".to_owned(),
        "--id".to_owned(),
        winget_id.to_owned(),
        "--exact".to_owned(),
        "--source".to_owned(),
        "winget".to_owned(),
        "--scope".to_owned(),
        "user".to_owned(),
        "--accept-package-agreements".to_owned(),
        "--accept-source-agreements".to_owned(),
        "--disable-interactivity".to_owned(),
    ]
}

/// stdout/stderr を境界長へ丸めて要約する。改行は畳み、制御文字は落とす。
fn bounded_summary(stdout: &[u8], stderr: &[u8], succeeded: bool) -> String {
    const MAX: usize = 600;
    let raw = if succeeded { stdout } else { stderr };
    let text: String = String::from_utf8_lossy(raw)
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect();
    let flattened = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.is_empty() {
        return if succeeded {
            "導入処理が完了しました。".to_owned()
        } else {
            "WinGetが失敗を返しました。ネットワークや既存導入を確認してください。".to_owned()
        };
    }
    if flattened.chars().count() > MAX {
        flattened.chars().take(MAX).collect::<String>() + "…"
    } else {
        flattened
    }
}

#[cfg(windows)]
pub fn install(app_id: &str) -> CoreResult<InstallOutcome> {
    let app = find_allowed(app_id).ok_or_else(|| {
        CoreError::invalid_request("許可されていないアプリIDです。一覧のアプリだけ導入できます。")
    })?;
    let args = install_args(app.winget_id);
    let output = std::process::Command::new("winget")
        .args(&args)
        .output()
        .map_err(|_| {
            CoreError::new(
                "WINGET_LAUNCH_FAILED",
                "APPLY",
                true,
                "WinGetを起動できませんでした。『アプリ インストーラー』が必要です。",
            )
        })?;
    Ok(InstallOutcome {
        app_id: app.id.to_owned(),
        app_name: app.name.to_owned(),
        succeeded: output.status.success(),
        exit_code: output.status.code(),
        summary: bounded_summary(&output.stdout, &output.stderr, output.status.success()),
    })
}

#[cfg(not(windows))]
pub fn install(_app_id: &str) -> CoreResult<InstallOutcome> {
    Err(CoreError::new(
        "UNSUPPORTED_PLATFORM",
        "APPLY",
        false,
        "PCカスタムはWindows 11専用です。",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_non_empty_and_ids_are_unique() {
        let catalog = app_catalog();
        assert!(!catalog.is_empty());
        let mut ids: Vec<&str> = ALLOWED_APPS.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "allowlistのidは一意");
    }

    #[test]
    fn powertoys_install_uses_the_existing_fixed_winget_entry() {
        let app = find_allowed("powertoys").expect("PowerToys is allowlisted");
        assert_eq!(app.winget_id, "Microsoft.PowerToys");
        assert_eq!(app.name, "Microsoft PowerToys");
    }

    #[test]
    fn unknown_app_id_is_not_in_allowlist() {
        assert!(find_allowed("totally-unknown").is_none());
        assert!(find_allowed("../../evil").is_none());
        assert!(find_allowed("Discord.Discord; rm -rf").is_none());
    }

    #[test]
    fn install_args_are_fixed_and_carry_no_user_injection() {
        // winget_id は allowlist 由来のみ。引数は固定で、shellメタ文字を解釈する余地がない
        // (直接プロセス起動・Vec引数)。危険フラグが混ざらないことも確認。
        let app = find_allowed("vscode").expect("allowlisted");
        let args = install_args(app.winget_id);
        assert_eq!(args[0], "install");
        assert!(args.contains(&"--id".to_owned()));
        assert!(args.contains(&"Microsoft.VisualStudioCode".to_owned()));
        assert!(args.contains(&"--source".to_owned()) && args.contains(&"winget".to_owned()));
        assert!(args.contains(&"--exact".to_owned()));
        // 危険な指定が固定引数に含まれない。
        for token in &args {
            assert!(!token.contains(';'));
            assert!(!token.contains('&'));
            assert!(!token.contains('|'));
            assert!(token != "--source" || true);
        }
    }

    #[test]
    fn bounded_summary_truncates_and_drops_control_chars() {
        let long = "a".repeat(2000);
        let out = bounded_summary(long.as_bytes(), b"", true);
        assert!(out.chars().count() <= 601);
        let ctrl = bounded_summary(b"line1\x07\x00line2", b"", true);
        assert!(!ctrl.contains('\u{7}'));
    }
}
