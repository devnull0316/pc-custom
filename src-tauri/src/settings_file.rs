use std::path::Path;

use uuid::Uuid;

use crate::error::{CoreError, CoreResult};

/// 小さな設定 JSON を、既存ファイルがある場合も置き換えて保存する。
pub(crate) fn replace(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("settings.json");
    let temporary = path.with_file_name(format!("{file_name}.{}.tmp", Uuid::new_v4()));
    std::fs::write(&temporary, bytes).map_err(|_| CoreError::storage())?;
    if let Err(error) = replace_file(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> CoreResult<()> {
    use windows::{
        core::HSTRING,
        Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        },
    };

    let from = HSTRING::from(from.as_os_str());
    let to = HSTRING::from(to.as_os_str());
    unsafe {
        MoveFileExW(
            &from,
            &to,
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|_| CoreError::storage())
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> CoreResult<()> {
    std::fs::rename(from, to).map_err(|_| CoreError::storage())
}
