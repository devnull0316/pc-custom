use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    compatibility::OsIdentity,
    engine::PcCustomEngine,
    error::{CoreError, CoreResult},
    journal::JournalDatabase,
    presentation::BootstrapStatus,
};

/// Tauri-managed application state. Initialization failures keep the UI alive in
/// fail-closed mode; no command can reach a mutation path without an engine.
pub struct ApplicationState {
    engine: Option<Arc<PcCustomEngine>>,
    profile_store: Option<Arc<crate::game_profile::ProfileStore>>,
    theme_schedule_store: Option<Arc<crate::theme_schedule::ThemeScheduleStore>>,
    taskbar_store: Option<Arc<crate::taskbar_watcher::TaskbarAutoHideStore>>,
    hot_corner_store: Option<Arc<crate::hot_corner::HotCornerStore>>,
    share_session_store: Option<Arc<crate::share_session::ShareSessionStore>>,
    hot_corner_presenter: Option<Arc<crate::hot_corner::HotCornerPresenter>>,
    mode_ribbon: Option<Arc<crate::windows::ModeRibbonController>>,
    initialization_error: Option<CoreError>,
    _instance_guard: Option<crate::windows::AppInstanceGuard>,
    profile_watcher: Option<crate::game_profile::ProfileWatcher>,
}

impl ApplicationState {
    pub fn initialize() -> Self {
        match initialize_engine() {
            Ok((
                engine,
                instance_guard,
                profile_store,
                theme_schedule_store,
                taskbar_store,
                hot_corner_store,
                share_session_store,
            )) => {
                let engine = Arc::new(engine);
                let hot_corner_presenter =
                    Arc::new(crate::hot_corner::HotCornerPresenter::default());
                let mode_ribbon = match crate::windows::ModeRibbonController::spawn() {
                    Ok(controller) => {
                        let controller = Arc::new(controller);
                        sync_manual_mode_ribbons(&controller, &profile_store);
                        Some(controller)
                    }
                    Err(error) => {
                        eprintln!("mode ribbon initialization failed: {error}");
                        None
                    }
                };
                // 有効プロファイルのゲーム起動を検知して準備を適用/復元する背景監視。
                // 既定ではどのプロファイルも自動適用オフのため、実質待機で始まる。
                match crate::game_profile::ProfileWatcher::spawn(
                    engine.clone(),
                    profile_store.clone(),
                    Some(theme_schedule_store.clone()),
                    Some(taskbar_store.clone()),
                    mode_ribbon.clone(),
                    Some(hot_corner_store.clone()),
                    Some(hot_corner_presenter.clone()),
                ) {
                    Ok(watcher) => Self {
                        engine: Some(engine),
                        profile_store: Some(profile_store),
                        theme_schedule_store: Some(theme_schedule_store),
                        taskbar_store: Some(taskbar_store),
                        hot_corner_store: Some(hot_corner_store),
                        share_session_store: Some(share_session_store),
                        hot_corner_presenter: Some(hot_corner_presenter),
                        mode_ribbon,
                        initialization_error: None,
                        _instance_guard: Some(instance_guard),
                        profile_watcher: Some(watcher),
                    },
                    Err(error) => Self {
                        // 監視開始失敗時も store は残し、自動適用の無効化・削除だけは許可する。
                        // engine() は initialization_error により fail-closed になる。
                        engine: Some(engine),
                        profile_store: Some(profile_store),
                        theme_schedule_store: Some(theme_schedule_store),
                        taskbar_store: Some(taskbar_store),
                        hot_corner_store: Some(hot_corner_store),
                        share_session_store: Some(share_session_store),
                        hot_corner_presenter: Some(hot_corner_presenter),
                        mode_ribbon,
                        initialization_error: Some(error),
                        _instance_guard: Some(instance_guard),
                        profile_watcher: None,
                    },
                }
            }
            Err(error) => Self {
                engine: None,
                profile_store: None,
                theme_schedule_store: None,
                taskbar_store: None,
                hot_corner_store: None,
                share_session_store: None,
                hot_corner_presenter: None,
                mode_ribbon: None,
                initialization_error: Some(error),
                _instance_guard: None,
                profile_watcher: None,
            },
        }
    }

    pub fn taskbar_store(&self) -> CoreResult<Arc<crate::taskbar_watcher::TaskbarAutoHideStore>> {
        self.taskbar_store.clone().ok_or_else(|| {
            CoreError::recovery_required(
                "タスクバー設定の保存領域を初期化できなかったため、操作を停止しました。",
            )
        })
    }

    pub fn hot_corner_store(&self) -> CoreResult<Arc<crate::hot_corner::HotCornerStore>> {
        self.hot_corner_store.clone().ok_or_else(|| {
            CoreError::recovery_required(
                "ホットコーナー設定の保存領域を初期化できなかったため、操作を停止しました。",
            )
        })
    }

    pub fn share_session_store(&self) -> CoreResult<Arc<crate::share_session::ShareSessionStore>> {
        self.share_session_store.clone().ok_or_else(|| {
            CoreError::recovery_required(
                "画面共有セッションの保存領域を初期化できなかったため、操作を停止しました。",
            )
        })
    }

    pub fn register_hot_corner_target(&self, app: tauri::AppHandle) {
        use tauri::{Emitter, Manager};

        if let Some(presenter) = self.hot_corner_presenter.as_ref() {
            presenter.register(move || {
                let window = app.get_webview_window("main").ok_or_else(|| {
                    CoreError::recovery_required(
                        "PCカスタムの画面を確認できないため、ホットコーナーを停止しました。",
                    )
                })?;
                window.show().map_err(|_| {
                    CoreError::new(
                        "HOT_CORNER_SHOW_FAILED",
                        "HOT_CORNER",
                        true,
                        "PCカスタムの画面を表示できませんでした。",
                    )
                })?;
                window.unminimize().map_err(|_| {
                    CoreError::new(
                        "HOT_CORNER_RESTORE_FAILED",
                        "HOT_CORNER",
                        true,
                        "PCカスタムの画面を元の大きさへ戻せませんでした。",
                    )
                })?;
                window.set_focus().map_err(|_| {
                    CoreError::new(
                        "HOT_CORNER_FOCUS_FAILED",
                        "HOT_CORNER",
                        true,
                        "PCカスタムの画面を前へ出せませんでした。",
                    )
                })?;
                app.emit(crate::hot_corner::HOT_CORNER_EVENT, ())
                    .map_err(|_| {
                        CoreError::new(
                            "HOT_CORNER_NAVIGATION_FAILED",
                            "HOT_CORNER",
                            true,
                            "モード画面を開けませんでした。",
                        )
                    })
            });
        }
    }

    pub fn mode_ribbon(&self) -> CoreResult<Arc<crate::windows::ModeRibbonController>> {
        self.mode_ribbon.clone().ok_or_else(|| {
            CoreError::new(
                "MODE_RIBBON_UNAVAILABLE",
                "MODE_RIBBON",
                true,
                "モードリボンを開始できませんでした。アプリを開き直して再試行してください。",
            )
        })
    }

    pub fn sync_manual_mode_ribbons(&self) -> CoreResult<()> {
        let controller = self.mode_ribbon()?;
        let store = self.profile_store()?;
        sync_manual_mode_ribbons(&controller, &store);
        Ok(())
    }

    pub fn engine(&self) -> CoreResult<Arc<PcCustomEngine>> {
        if let Some(error) = &self.initialization_error {
            return Err(error.clone());
        }
        if let Some(error) = self
            .profile_watcher
            .as_ref()
            .and_then(crate::game_profile::ProfileWatcher::health_error)
        {
            return Err(error);
        }
        self.engine.clone().ok_or_else(|| {
            CoreError::recovery_required("安全コアを初期化できないため、変更操作を停止しています。")
        })
    }

    pub fn profile_store(&self) -> CoreResult<Arc<crate::game_profile::ProfileStore>> {
        self.profile_store.clone().ok_or_else(|| {
            self.initialization_error.clone().unwrap_or_else(|| {
                CoreError::recovery_required(
                    "安全コアを初期化できないため、プロファイル操作を停止しています。",
                )
            })
        })
    }

    pub fn theme_schedule_store(
        &self,
    ) -> CoreResult<Arc<crate::theme_schedule::ThemeScheduleStore>> {
        self.theme_schedule_store.clone().ok_or_else(|| {
            self.initialization_error.clone().unwrap_or_else(|| {
                CoreError::recovery_required(
                    "安全コアを初期化できないため、自動切り替え設定を操作できません。",
                )
            })
        })
    }

    pub fn bootstrap_status(&self) -> BootstrapStatus {
        if let Some(error) = &self.initialization_error {
            return fail_closed_status(error.user_message.clone());
        }
        if let Some(error) = self
            .profile_watcher
            .as_ref()
            .and_then(crate::game_profile::ProfileWatcher::health_error)
        {
            return fail_closed_status(error.user_message);
        }
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

fn sync_manual_mode_ribbons(
    controller: &crate::windows::ModeRibbonController,
    store: &crate::game_profile::ProfileStore,
) {
    let active = store
        .list()
        .into_iter()
        .filter(|profile| profile.is_manual() && profile.active_run.is_some())
        .map(|profile| crate::windows::ActiveModeRibbon {
            profile_id: profile.id,
            color: profile.ribbon_color,
        })
        .collect();
    controller.sync_manual_profiles(active);
}

impl Drop for ApplicationState {
    fn drop(&mut self) {
        // instance lock や engine を解放する前に、監視中のプロファイルを復元する。
        if let Some(mut watcher) = self.profile_watcher.take() {
            if let Err(error) = watcher.shutdown() {
                eprintln!("application shutdown profile restore failed: {error}");
            }
        }
    }
}

type EngineBootstrap = (
    PcCustomEngine,
    crate::windows::AppInstanceGuard,
    Arc<crate::game_profile::ProfileStore>,
    Arc<crate::theme_schedule::ThemeScheduleStore>,
    Arc<crate::taskbar_watcher::TaskbarAutoHideStore>,
    Arc<crate::hot_corner::HotCornerStore>,
    Arc<crate::share_session::ShareSessionStore>,
);

fn initialize_engine() -> CoreResult<EngineBootstrap> {
    let data_directory = data_directory()?;
    ensure_private_directory(&data_directory)?;
    let instance_guard = crate::windows::acquire_app_instance_lock(
        &data_directory.join("instance.lock"),
    )
    .map_err(|error| {
        CoreError::new(
            "APP_INSTANCE_LOCKED",
            "BOOTSTRAP",
            error.kind == crate::windows::WindowsErrorKind::ResourceLimit,
            "別のPCカスタムが実行中です。この画面からの変更操作は停止しています。",
        )
    })?;
    let database = Arc::new(JournalDatabase::open(&data_directory.join("pc-custom.db"))?);
    // Absence is an explicit engine input: startup reconcile records
    // RECOVERY_REQUIRED and every mutation gate remains closed.
    let identity = OsIdentity::load().ok();
    let profile_store = Arc::new(crate::game_profile::ProfileStore::open(
        data_directory.join("profiles.json"),
    )?);
    let window_layout_store = Arc::new(crate::window_layout::WindowLayoutStore::open(
        data_directory.join("window-layout.json"),
    )?);
    let engine = PcCustomEngine::new_with_runtime_stores(
        database,
        identity,
        Some(profile_store.clone()),
        Some(window_layout_store.clone()),
    )?;
    // 試用したまま閉じられた変更を、開き直した時点で元へ戻す。
    // 失敗しても起動は止めない（通常の復旧経路が残りを拾う）。
    if let Err(error) = engine.revert_expired_trials() {
        eprintln!("expired trial revert failed: {error:?}");
    }
    let theme_schedule_store = Arc::new(crate::theme_schedule::ThemeScheduleStore::open(
        data_directory.join("theme-schedule.json"),
    )?);
    let taskbar_store = Arc::new(crate::taskbar_watcher::TaskbarAutoHideStore::open(
        data_directory.join("taskbar-autohide.json"),
    )?);
    let hot_corner_store = Arc::new(crate::hot_corner::HotCornerStore::open(
        data_directory.join("hot-corners.json"),
    )?);
    let share_session_store = Arc::new(crate::share_session::ShareSessionStore::open(
        data_directory.join("share-session.json"),
    )?);
    Ok((
        engine,
        instance_guard,
        profile_store,
        theme_schedule_store,
        taskbar_store,
        hot_corner_store,
        share_session_store,
    ))
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
    // 保存先はASCIIに保つ。実行ファイル名(PCCustom)と揃うほうが追いやすく、
    // 日本語パスは起動時にしか通らず、この環境では実地確認ができないため。
    Ok(PathBuf::from(local_app_data).join("PCCustom").join("data"))
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
            Ok(metadata) if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 => {
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
        "PCカスタムはWindows 11専用です。",
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
