//! 実行ファイルを利用者に選んでもらう。
//!
//! モードにゲームを登録するとき、フルパスを手で打たせていた。
//! 打ち間違えれば別のファイルが登録されるか、そもそも登録できない。
//! ここは Windows 自身のファイル選択画面（`IFileOpenDialog`、公開 COM）を開いて、
//! **利用者が選んだ結果のパスだけ**を受け取る。
//!
//! 安全契約:
//! - 引数を取らない。フロントから渡せるものが無いので、開き先を操作される余地がない。
//! - 返すのは利用者が実際に選んだ 1 件のパスだけ。列挙も走査もしない。
//! - 取り消されたら `None`。エラーとして扱わない（取り消しは失敗ではない）。
//! - 追加の依存は入れていない。使うのは既に有効な `Win32_UI_Shell` / `Win32_System_Com`。

use super::{WindowsError, WindowsErrorKind, WindowsResult};

#[cfg(windows)]
pub fn pick_executable() -> WindowsResult<Option<String>> {
    use windows::Win32::{
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        },
        UI::Shell::{
            Common::COMDLG_FILTERSPEC, FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST,
            FOS_PATHMUSTEXIST, SIGDN_FILESYSPATH,
        },
    };

    // ダイアログは STA を要求する。ここで初期化し、この関数を抜けるときに必ず戻す。
    let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    // 既に別の形で初期化済みでも続行してよい。解放の要否だけ分ける。
    let needs_uninit = init.is_ok();
    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }
    let _com = ComGuard(needs_uninit);

    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }.map_err(|_| {
            WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "CoCreateInstance FileOpenDialog",
                None,
            )
        })?;

    // 実在するファイルだけを選べるようにする。存在しないパスは受け取らない。
    let _ = unsafe { dialog.SetOptions(FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST) };
    let filters = [COMDLG_FILTERSPEC {
        pszName: windows::core::w!("プログラム (*.exe)"),
        pszSpec: windows::core::w!("*.exe"),
    }];
    let _ = unsafe { dialog.SetFileTypes(&filters) };
    let _ =
        unsafe { dialog.SetTitle(windows::core::w!("ゲームの実行ファイルを選ぶ")) };

    // 取り消しはここでエラーとして返る。失敗ではないので None にする。
    if unsafe { dialog.Show(None) }.is_err() {
        return Ok(None);
    }

    let item = unsafe { dialog.GetResult() }.map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "IFileOpenDialog GetResult",
            None,
        )
    })?;
    let raw = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "IShellItem GetDisplayName",
            None,
        )
    })?;
    let path = unsafe { raw.to_string() }.map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "selected path is not UTF-16",
            None,
        )
    })?;
    // GetDisplayName が確保した領域は呼び出し側が解放する。
    unsafe { CoTaskMemFree(Some(raw.as_ptr() as *const core::ffi::c_void)) };

    Ok(Some(path))
}

#[cfg(not(windows))]
pub fn pick_executable() -> WindowsResult<Option<String>> {
    Err(WindowsError::new(
        WindowsErrorKind::UnsupportedPlatform,
        "pick executable",
        None,
    ))
}
