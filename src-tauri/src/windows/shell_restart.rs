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
            // セッションが同じで、かつ実体が %WINDIR%\explorer.exe のものだけ。
            // 名前だけで判断すると、利用者が自分のプログラムを explorer.exe と
            // 名付けて動かしていた場合にそれを落とす。
            if same_session && is_windows_shell_image(entry.th32ProcessID) {
                ids.push(entry.th32ProcessID);
            }
        }
        ok = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    let _ = unsafe { windows::Win32::Foundation::CloseHandle(snapshot) };
    Ok(ids)
}

/// プロセスの実行ファイルの実体が `%WINDIR%\explorer.exe` かを確かめる。
///
/// スナップショットが返すのはファイル名だけで、置き場所は分からない。
/// 名前が一致しただけで終了させると、利用者が自分のプログラムを explorer.exe と
/// 名付けて動かしていた場合にそれを落とす。実体を確かめられないものには触らない。
#[cfg(windows)]
fn is_windows_shell_image(process_id: u32) -> bool {
    use windows::{
        core::PWSTR,
        Win32::{
            Foundation::{CloseHandle, MAX_PATH},
            System::Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
    };
    let Some(windir) = std::env::var_os("WINDIR") else {
        return false;
    };
    let expected = std::path::Path::new(&windir).join("explorer.exe");

    let Ok(handle) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) })
    else {
        return false;
    };
    let mut buffer = [0u16; MAX_PATH as usize];
    let mut length = buffer.len() as u32;
    let queried = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    }
    .is_ok();
    let _ = unsafe { CloseHandle(handle) };
    if !queried {
        return false;
    }
    let actual = String::from_utf16_lossy(&buffer[..length as usize]);
    // Windows のパスは大文字小文字を区別しない。実測では API が "C:\Windows\explorer.exe" を
    // 返す一方 %WINDIR% は "C:\WINDOWS" で、単純比較だと本物のシェルを取り逃がす。
    actual.to_lowercase() == expected.to_string_lossy().to_lowercase()
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
    // タスクバーが出ているのにシェルを1つも特定できないなら、こちらの判定が壊れている。
    // 何もしていないのに成功を返すと、呼び出し側は「再起動したのに反映されない」と誤解する。
    if ids.is_empty() && shell_window_present() {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "shell process not identified while a taskbar is present",
            None,
        ));
    }
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

#[cfg(all(test, windows))]
mod regression_tests {
    /// タスクバーが出ているなら、シェルのプロセスを必ず1つは特定できなければならない。
    ///
    /// 実体パス検査を入れた際、`%WINDIR%` が "C:\WINDOWS"、API が "C:\Windows" を返すため
    /// 単純比較で本物のシェルを取り逃がし、**再起動が丸ごと無効になっていた**。
    /// しかも `terminated: 0` で成功が返るため、呼び出し側からは気づけなかった。
    /// この検査はそれを次に必ず捕まえる。
    #[test]
    fn shell_lookup_finds_the_real_shell_when_a_taskbar_exists() {
        if !super::shell_window_present() {
            println!("タスクバーが無い環境のためスキップ");
            return;
        }
        let ids = super::shell_process_ids().expect("シェル探索は成功すること");
        assert!(
            !ids.is_empty(),
            "タスクバーが出ているのにシェルのプロセスを特定できていない。
             実体パスの比較が大文字小文字で落ちていないか、
             セッション判定が誤っていないかを疑うこと。"
        );
    }
}

#[cfg(all(test, windows))]
mod diagnostic_tests {
    /// シェルを探す処理が実際に何を見ているかを出す。変更はしない。
    #[test]
    #[ignore = "シェル検出の診断。何も変更しない"]
    fn dump_what_the_shell_lookup_sees() {
        use windows::Win32::System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            RemoteDesktop::ProcessIdToSessionId,
        };
        let mut own_session = 0u32;
        let ok = unsafe { ProcessIdToSessionId(std::process::id(), &mut own_session) }.is_ok();
        println!("自分のセッション取得={ok} session={own_session}");
        println!("WINDIR={:?}", std::env::var_os("WINDIR"));

        let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
            println!("スナップショット取得に失敗");
            return;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
        let mut found = 0;
        while more {
            let end = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
            if name.eq_ignore_ascii_case("explorer.exe") {
                found += 1;
                let mut session = u32::MAX;
                let session_ok =
                    unsafe { ProcessIdToSessionId(entry.th32ProcessID, &mut session) }.is_ok();
                let image_ok = super::is_windows_shell_image(entry.th32ProcessID);
                println!(
                    "  pid={} session_ok={session_ok} session={session} 同一={} 実体一致={image_ok}",
                    entry.th32ProcessID,
                    session == own_session
                );
            }
            more = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
        }
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(snapshot) };
        println!("explorer.exe という名前のプロセス数={found}");
        println!(
            "shell_process_ids() の結果={:?}",
            super::shell_process_ids()
        );
    }
}
