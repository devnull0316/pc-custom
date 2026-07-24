use std::{
    env,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    compatibility::OsIdentity,
    engine::TotonoeEngine,
    error::{CoreError, CoreResult},
    journal::JournalDatabase,
    presentation::BootstrapStatus,
};

/// Tauri-managed application state. Initialization failures keep the UI alive in
/// fail-closed mode; no command can reach a mutation path without an engine.
pub struct ApplicationState {
    engine: Option<Arc<TotonoeEngine>>,
    initialization_error: Option<CoreError>,
    _instance_guard: Option<crate::windows::AppInstanceGuard>,
}

impl ApplicationState {
    pub fn initialize() -> Self {
        match initialize_engine() {
            Ok((engine, instance_guard)) => Self {
                engine: Some(Arc::new(engine)),
                initialization_error: None,
                _instance_guard: Some(instance_guard),
            },
            Err(error) => Self {
                engine: None,
                initialization_error: Some(error),
                _instance_guard: None,
            },
        }
    }

    pub fn engine(&self) -> CoreResult<Arc<TotonoeEngine>> {
        self.engine.clone().ok_or_else(|| {
            self.initialization_error.clone().unwrap_or_else(|| {
                CoreError::recovery_required(
                    "安全コアを初期化できないため、変更操作を停止しています。",
                )
            })
        })
    }

    pub fn bootstrap_status(&self) -> BootstrapStatus {
        match &self.engine {
            Some(engine) => engine
                .bootstrap_status()
                .unwrap_or_else(|error| fail_closed_status(error.user_message)),
            None => fail_closed_status(
                self.initialization_error
                    .as_ref()
                    .map(|error| error.user_message.clone())
                    .unwrap_or_else(|| {
                        "安全コアを初期化できないため、変更操作を停止しています。".to_owned()
                    }),
            ),
        }
    }
}

fn initialize_engine() -> CoreResult<(TotonoeEngine, crate::windows::AppInstanceGuard)> {
    let data_directory = data_directory()?;
    ensure_private_directory(&data_directory)?;
    let instance_guard =
        crate::windows::acquire_app_instance_lock(&data_directory.join("instance.lock"))
            .map_err(|error| {
                CoreError::new(
                    "APP_INSTANCE_LOCKED",
                    "BOOTSTRAP",
                    error.kind == crate::windows::WindowsErrorKind::ResourceLimit,
                    "別のTotonoeが実行中です。この画面からの変更操作は停止しています。",
                )
            })?;
    let database = Arc::new(JournalDatabase::open(&data_directory.join("totonoe.db"))?);
    let identity = match OsIdentity::load() {
        Ok(identity) => Some(identity),
        // Absence is an explicit engine input: startup reconcile records
        // RECOVERY_REQUIRED and every mutation gate remains closed.
        Err(_identity_error) => None,
    };
    let engine = TotonoeEngine::new(database, identity)?;
    Ok((engine, instance_guard))
}

fn data_directory() -> CoreResult<PathBuf> {
    let local_app_data = env::var_os("LOCALAPPDATA").ok_or_else(|| {
        CoreError::new(
            "DATA_DIRECTORY_UNAVAILABLE",
            "BOOTSTRAP",
            false,
            "ユーザー用データ保存先を確認できないため、変更操作を停止しています。",
        )
    })?;
    Ok(PathBuf::from(local_app_data).join("Totonoe").join("data"))
}

fn ensure_private_directory(path: &Path) -> CoreResult<()> {
    reject_reparse_points(path)?;
    fs::create_dir_all(path).map_err(|_| CoreError::storage())?;
    reject_reparse_points(path)
}

#[cfg(windows)]
fn reject_reparse_points(path: &Path) -> CoreResult<()> {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    let mut cursor = Some(path);
    while let Some(candidate) = cursor {
        match fs::symlink_metadata(candidate) {
            Ok(metadata)
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 =>
            {
                return Err(CoreError::new(
                    "UNSAFE_DATA_DIRECTORY",
                    "BOOTSTRAP",
                    false,
                    "変更記録の保存先が再解析ポイントのため、安全のため停止しました。",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(CoreError::storage()),
        }
        cursor = candidate.parent();
    }
    Ok(())
}

#[cfg(not(windows))]
fn reject_reparse_points(_path: &Path) -> CoreResult<()> {
    Err(CoreError::new(
        "UNSUPPORTED_PLATFORM",
        "BOOTSTRAP",
        false,
        "TotonoeはWindows 11専用です。",
    ))
}

fn fail_closed_status(message: String) -> BootstrapStatus {
    BootstrapStatus {
        mode: "recovery_required".to_owned(),
        os_label: "安全コアを開始できません".to_owned(),
        build: None,
        message,
        recovery_count: 0,
    }
}
