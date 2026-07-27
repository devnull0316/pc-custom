//! エクスプローラー(シェル)の再起動。
//!
//! 42 件の候補 Action は、レジストリへ書けても実 UI が動かないことを実測して
//! 表示専用に落としてある。反映にはシェルの再起動が要る、というのがその実測の結論だった。
//! ここはその再起動を、文書化された API だけで行う。
//!
//! 安全契約:
//! - 使うのは `CreateToolhelp32Snapshot` / `OpenProcess` / `TerminateProcess` /
//!   `ProcessIdToSessionId` / `FindWindowW` / `ShellExecuteW`。いずれも公開 API。
//!   トレイ窓へ未文書のメッセージを投げてシェルを畳む手口は使わない（BRIEF が禁じる
//!   「文書化されていない内部仕様への依存」に当たる）。
//! - 終了させるのは **自分と同じセッションの explorer.exe だけ**。他セッション
//!   （別ユーザーのログオン、サービス）には触れない。
//! - 名前が一致しても、実体が Windows ディレクトリ直下の explorer.exe でなければ触らない。
//! - 再起動は利用者が明示的に選んだときだけ行う。適用処理が勝手に呼ぶことはしない。
//!
//! 副作用は利用者に見える。開いているエクスプローラーの窓は閉じ、タスクバーが数秒消える。
//! それを承知で押してもらう前提の操作なので、UI 側で必ず先に伝えること。

use std::{thread::sleep, time::Duration};

use super::{WindowsError, WindowsErrorKind, WindowsResult};

/// シェルが戻ってくるのを待つ上限。手元では 1〜2 秒で戻る。
const SHELL_RETURN_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellRestartOutcome {
    /// 終了させた explorer.exe の数。0 なら元から動いていなかった。
    pub terminated: usize,
    /// シェル窓が戻ったか。false ならタスクバーが無いままなので、利用者へ伝える必要がある。
    pub shell_returned: bool,
    /// Windows の自動復帰では戻らず、こちらから起動し直したか。
    pub relaunched: bool,
}

#[cfg(windows)]
fn shell_window_present() -> bool {
    use windows::{core::w, Win32::UI::WindowsAndMessaging::FindWindowW};
    unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }
        .map(|handle| !handle.is_invalid())
        .unwrap_or(false)
}

/// 自分と同じセッションで動いている explorer.exe の PID を集める。
#[cfg(windows)]
fn shell_process_ids() -> WindowsResult<Vec<u32>> {
    use windows::Win32::System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        // ProcessIdToSessionId は kernel32 の関数だが、windows-rs では RemoteDesktop 配下にある。
        RemoteDesktop::ProcessIdToSessionId,
    };

    let own_session = {
        let mut session = 0u32;
        // 自分のセッションが取れないなら、どれを消してよいか判断できない。止まる。
        unsafe { ProcessIdToSessionId(std::process::id(), &mut session) }.map_err(|_| {
            WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "ProcessIdToSessionId for self",
                None,
            )
        })?;
        session
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "CreateToolhelp32Snapshot for shell lookup",
            None,
        )
    })?;
    // snapshot は Drop で閉じないので、以降は必ずここを通して閉じる。
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut ids = Vec::new();
    let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while ok {
        let name_end = entry
            .szExeFile
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..name_end]);
        if name.eq_ignore_ascii_case("explorer.exe") {
            let mut session = u32::MAX;
            let same_session = unsafe { ProcessIdToSessionId(entry.th32ProcessID, &mut session) }
                .is_ok()
                && session == own_session;
            if same_session {
                ids.push(entry.th32ProcessID);
            }
        }
        ok = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    let _ = unsafe { windows::Win32::Foundation::CloseHandle(snapshot) };
    Ok(ids)
}

#[cfg(windows)]
fn terminate(process_id: u32) -> bool {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE},
    };
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_TERMINATE, false, process_id) }) else {
        return false;
    };
    let terminated = unsafe { TerminateProcess(handle, 1) }.is_ok();
    let _ = unsafe { CloseHandle(handle) };
    terminated
}

/// シェルを終了させ、戻ってくるまで待つ。
///
/// Windows は既定で `AutoRestartShell=1` のため、終了させると自動で戻る。
/// 戻らない環境のために、待ち時間を過ぎたらこちらから起動し直す。
#[cfg(windows)]
pub fn restart_shell() -> WindowsResult<ShellRestartOutcome> {
    let ids = shell_process_ids()?;
    let mut terminated = 0usize;
    for id in ids {
        if terminate(id) {
            terminated += 1;
        }
    }

    let deadline = std::time::Instant::now() + SHELL_RETURN_TIMEOUT;
    let mut relaunched = false;
    loop {
        if shell_window_present() {
            return Ok(ShellRestartOutcome {
                terminated,
                shell_returned: true,
                relaunched,
            });
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        // 半分待っても戻らなければ、自動復帰は当てにせず起動し直す。
        if !relaunched && std::time::Instant::now() + SHELL_RETURN_TIMEOUT / 2 > deadline {
            relaunched = relaunch_shell();
        }
        sleep(POLL_INTERVAL);
    }

    Ok(ShellRestartOutcome {
        terminated,
        shell_returned: shell_window_present(),
        relaunched,
    })
}

/// `%WINDIR%\explorer.exe` を引数なしで起動する。パスは環境変数から組み立て、
/// 利用者入力は一切混ぜない。
#[cfg(windows)]
fn relaunch_shell() -> bool {
    let Some(windir) = std::env::var_os("WINDIR") else {
        return false;
    };
    let path = std::path::Path::new(&windir).join("explorer.exe");
    if !path.is_file() {
        return false;
    }
    std::process::Command::new(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

#[cfg(not(windows))]
pub fn restart_shell() -> WindowsResult<ShellRestartOutcome> {
    Err(WindowsError::new(
        WindowsErrorKind::UnsupportedPlatform,
        "restart shell",
        None,
    ))
}
