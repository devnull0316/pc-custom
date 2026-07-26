//! Fixed allowlist application discovery and direct launch.
//! No public function accepts a path, executable name, argument, or shell string.

use super::{registered_file_identity, WindowsError, WindowsErrorKind, WindowsResult};
use crate::action::{AppLaunchBundle, KnownAppObservation, KnownAppState, KnownAppsObservation};

#[derive(Debug, Clone, Copy)]
pub struct KnownApp {
    pub name: &'static str,
    app_path_name: &'static str,
    process_names: &'static [&'static str],
    system_fallback: bool,
    fixed_args: &'static [&'static str],
}

const EDGE: KnownApp = KnownApp {
    name: "Microsoft Edge",
    app_path_name: "msedge.exe",
    process_names: &["msedge.exe"],
    system_fallback: false,
    fixed_args: &[],
};
const NOTEPAD: KnownApp = KnownApp {
    name: "メモ帳",
    app_path_name: "notepad.exe",
    process_names: &["notepad.exe"],
    system_fallback: true,
    fixed_args: &[],
};
const CALCULATOR: KnownApp = KnownApp {
    name: "電卓",
    app_path_name: "calc.exe",
    process_names: &["calculatorapp.exe", "calc.exe"],
    system_fallback: true,
    fixed_args: &[],
};
const PAINT: KnownApp = KnownApp {
    name: "ペイント",
    app_path_name: "mspaint.exe",
    process_names: &["mspaint.exe"],
    system_fallback: true,
    fixed_args: &[],
};

const POWERTOYS: KnownApp = KnownApp {
    name: "Microsoft PowerToys",
    app_path_name: "PowerToys.exe",
    process_names: &["powertoys.exe"],
    system_fallback: false,
    fixed_args: &[],
};

const STUDY: &[KnownApp] = &[EDGE, NOTEPAD];
const WORK: &[KnownApp] = &[EDGE, NOTEPAD, CALCULATOR];
const CREATIVE: &[KnownApp] = &[PAINT, NOTEPAD];
const POWER_TOYS: &[KnownApp] = &[POWERTOYS];

pub const fn apps_for_bundle(bundle: AppLaunchBundle) -> &'static [KnownApp] {
    match bundle {
        AppLaunchBundle::Study => STUDY,
        AppLaunchBundle::Work => WORK,
        AppLaunchBundle::Creative => CREATIVE,
        AppLaunchBundle::PowerToys => POWER_TOYS,
    }
}

#[cfg(windows)]
fn app_path_registry_value(app: KnownApp) -> WindowsResult<Option<String>> {
    use winreg::{
        enums::{
            RegType, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY,
            KEY_WOW64_64KEY,
        },
        RegKey,
    };

    let subkey = format!(
        r"Software\Microsoft\Windows\CurrentVersion\App Paths\{}",
        app.app_path_name
    );
    let roots = [
        RegKey::predef(HKEY_CURRENT_USER),
        RegKey::predef(HKEY_LOCAL_MACHINE),
    ];
    let views = [KEY_READ | KEY_WOW64_64KEY, KEY_READ | KEY_WOW64_32KEY];
    for root in roots {
        for view in views {
            let Ok(key) = root.open_subkey_with_flags(&subkey, view) else {
                continue;
            };
            let Ok(raw) = key.get_raw_value("") else {
                continue;
            };
            if raw.vtype != RegType::REG_SZ || raw.bytes.len() % 2 != 0 || raw.bytes.len() > 65_536
            {
                continue;
            }
            let units: Vec<u16> = raw
                .bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|value| *value != 0)
                .collect();
            let value = String::from_utf16(&units).map_err(|_| {
                WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "decode App Paths registry value",
                    None,
                )
            })?;
            if !value.is_empty() {
                return Ok(Some(value));
            }
        }
    }
    Ok(None)
}

/// Resolve the one fixed PowerToys App Paths entry. Absence is a normal
/// read-only result; malformed or inaccessible resolved files are errors.
#[cfg(windows)]
pub fn resolve_powertoys_app_path() -> WindowsResult<Option<String>> {
    let Some(candidate) = app_path_registry_value(POWERTOYS)? else {
        return Ok(None);
    };
    if candidate.contains('\0') || candidate.contains('"') {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "reject malformed PowerToys App Paths value",
            None,
        ));
    }
    registered_file_identity(&candidate).map(|(canonical, _)| Some(canonical))
}

#[cfg(not(windows))]
pub fn resolve_powertoys_app_path() -> WindowsResult<Option<String>> {
    Err(WindowsError::unsupported(
        "resolve fixed PowerToys App Paths entry",
    ))
}

#[cfg(windows)]
fn windows_system_path(file_name: &'static str) -> WindowsResult<String> {
    use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;
    let mut buffer = vec![0u16; 32_768];
    let length = unsafe { GetWindowsDirectoryW(Some(&mut buffer)) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "GetWindowsDirectoryW",
            None,
        ));
    }
    let windows = String::from_utf16(&buffer[..length]).map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "decode Windows directory",
            None,
        )
    })?;
    Ok(std::path::Path::new(&windows)
        .join("System32")
        .join(file_name)
        .to_string_lossy()
        .into_owned())
}

#[cfg(windows)]
pub fn resolve_known_app(app: KnownApp) -> WindowsResult<String> {
    let candidate = match app_path_registry_value(app)? {
        Some(value) => value,
        None if app.system_fallback => windows_system_path(app.app_path_name)?,
        None => {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "known app is not registered in App Paths",
                None,
            ))
        }
    };
    if candidate.contains('\0') || candidate.contains('"') {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "reject malformed App Paths value",
            None,
        ));
    }
    registered_file_identity(&candidate).map(|(canonical, _)| canonical)
}

#[cfg(not(windows))]
pub fn resolve_known_app(_app: KnownApp) -> WindowsResult<String> {
    Err(WindowsError::unsupported("resolve fixed App Paths entry"))
}

#[cfg(windows)]
fn running_process_names() -> WindowsResult<std::collections::HashSet<String>> {
    use windows::{
        core::HRESULT,
        Win32::{
            Foundation::ERROR_NO_MORE_FILES,
            System::Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
        },
    };
    struct Handle(windows::Win32::Foundation::HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
        }
    }
    let snapshot = Handle(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.map_err(|e| {
            WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "CreateToolhelp32Snapshot for app launch",
                Some(i64::from(e.code().0)),
            )
        })?,
    );
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe { Process32FirstW(snapshot.0, &mut entry) }.map_err(|e| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "Process32FirstW for app launch",
            Some(i64::from(e.code().0)),
        )
    })?;
    let mut names = std::collections::HashSet::new();
    loop {
        let end = entry
            .szExeFile
            .iter()
            .position(|v| *v == 0)
            .unwrap_or(entry.szExeFile.len());
        names.insert(String::from_utf16_lossy(&entry.szExeFile[..end]).to_ascii_lowercase());
        match unsafe { Process32NextW(snapshot.0, &mut entry) } {
            Ok(()) => {}
            Err(error) if error.code() == HRESULT::from_win32(ERROR_NO_MORE_FILES.0) => break,
            Err(error) => {
                return Err(WindowsError::new(
                    WindowsErrorKind::ApiFailure,
                    "Process32NextW for app launch",
                    Some(i64::from(error.code().0)),
                ))
            }
        }
    }
    Ok(names)
}

fn app_is_running(app: KnownApp, names: &std::collections::HashSet<String>) -> bool {
    app.process_names
        .iter()
        .any(|name| names.contains(&name.to_ascii_lowercase()))
}

pub fn observe_known_apps(bundle: AppLaunchBundle) -> WindowsResult<KnownAppsObservation> {
    let names = running_process_names()?;
    let apps = apps_for_bundle(bundle)
        .iter()
        .map(|app| {
            let state = if app_is_running(*app, &names) {
                KnownAppState::Running
            } else if resolve_known_app(*app).is_ok() {
                KnownAppState::NotRunning
            } else {
                KnownAppState::Unavailable
            };
            KnownAppObservation {
                name: app.name.to_owned(),
                state,
            }
        })
        .collect();
    Ok(KnownAppsObservation { bundle, apps })
}

pub fn launch_known_apps(bundle: AppLaunchBundle) -> WindowsResult<KnownAppsObservation> {
    let plan: Vec<(KnownApp, String)> = apps_for_bundle(bundle)
        .iter()
        .map(|app| resolve_known_app(*app).map(|path| (*app, path)))
        .collect::<WindowsResult<_>>()?;
    let running = running_process_names()?;
    for (app, path) in &plan {
        if app_is_running(*app, &running) {
            continue;
        }
        // 標準入出力を継承させない。継承すると、起動したアプリが PCカスタム のパイプを
        // 掴んだままになり、こちらの出力を読んでいる側がアプリ終了までブロックする。
        // （実機テストがこれで5分ハングした）
        let mut child = std::process::Command::new(path)
            .args(app.fixed_args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| WindowsError::io("spawn fixed allowlisted application", &error))?;
        if let Some(status) = child
            .try_wait()
            .map_err(|error| WindowsError::io("read allowlisted application exit status", &error))?
        {
            if !status.success() {
                return Err(WindowsError::new(
                    WindowsErrorKind::ApiFailure,
                    "allowlisted application exited during launch",
                    status.code().map(i64::from),
                ));
            }
        }
    }
    for _ in 0..30 {
        let observed = observe_known_apps(bundle)?;
        if observed
            .apps
            .iter()
            .all(|app| app.state == KnownAppState::Running)
        {
            return Ok(observed);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err(WindowsError::new(
        WindowsErrorKind::ApiFailure,
        "allowlisted application launch was not observed",
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundles_contain_only_fixed_entries_and_no_arguments() {
        for bundle in [
            AppLaunchBundle::Study,
            AppLaunchBundle::Work,
            AppLaunchBundle::Creative,
            AppLaunchBundle::PowerToys,
        ] {
            let apps = apps_for_bundle(bundle);
            assert!(!apps.is_empty());
            assert!(apps.len() <= 3);
            assert!(apps.iter().all(|app| app.fixed_args.is_empty()));
            assert!(apps.iter().all(|app| app.app_path_name.ends_with(".exe")));
        }
    }
}
