use crate::action::{
    AdvancedColorObservation, DefaultRenderAudioObservation, PrimaryRefreshRateObservation,
};

use super::{WindowsError, WindowsErrorKind, WindowsResult};

#[cfg(windows)]
pub fn read_primary_refresh_rate() -> WindowsResult<PrimaryRefreshRateObservation> {
    use windows::{
        core::PCWSTR,
        Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS},
    };

    let mut mode = DEVMODEW::default();
    mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    if !unsafe { EnumDisplaySettingsW(PCWSTR::null(), ENUM_CURRENT_SETTINGS, &mut mode) }.as_bool()
    {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "EnumDisplaySettingsW current primary mode",
            None,
        ));
    }
    // Windows documents 0 and 1 as hardware-default sentinel values rather
    // than measured refresh rates. Do not present either as Hz.
    if !(2..=1_000).contains(&mode.dmDisplayFrequency) {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate primary display refresh rate",
            None,
        ));
    }
    Ok(PrimaryRefreshRateObservation {
        hertz: mode.dmDisplayFrequency,
    })
}

#[cfg(not(windows))]
pub fn read_primary_refresh_rate() -> WindowsResult<PrimaryRefreshRateObservation> {
    Err(WindowsError::unsupported(
        "EnumDisplaySettingsW current primary mode",
    ))
}

#[cfg(windows)]
pub fn read_active_advanced_color() -> WindowsResult<AdvancedColorObservation> {
    use windows::Win32::{
        Devices::Display::{
            DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
            DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
            DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_MODE_INFO,
            DISPLAYCONFIG_PATH_INFO, QDC_ONLY_ACTIVE_PATHS,
        },
        Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
    };

    const MAX_ACTIVE_PATHS: u32 = 128;
    const MAX_MODE_INFOS: u32 = 512;
    const MAX_ATTEMPTS: usize = 3;

    for _ in 0..MAX_ATTEMPTS {
        let mut path_count = 0u32;
        let mut mode_count = 0u32;
        let size_status = unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
        };
        if size_status != ERROR_SUCCESS {
            return Err(WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "GetDisplayConfigBufferSizes active paths",
                Some(i64::from(size_status.0)),
            ));
        }
        if path_count == 0 {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "find active display path",
                None,
            ));
        }
        if path_count > MAX_ACTIVE_PATHS || mode_count > MAX_MODE_INFOS {
            return Err(WindowsError::new(
                WindowsErrorKind::ResourceLimit,
                "bound active display configuration",
                None,
            ));
        }

        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        let query_status = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if query_status == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if query_status != ERROR_SUCCESS {
            return Err(WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "QueryDisplayConfig active paths",
                Some(i64::from(query_status.0)),
            ));
        }
        if path_count == 0 || path_count as usize > paths.len() {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "validate active display path count",
                None,
            ));
        }

        paths.truncate(path_count as usize);
        let mut supported_path_count = 0u32;
        let mut enabled_path_count = 0u32;
        for path in &paths {
            let mut request = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO::default();
            request.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO;
            request.header.size =
                std::mem::size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32;
            request.header.adapterId = path.targetInfo.adapterId;
            request.header.id = path.targetInfo.id;
            let status = unsafe { DisplayConfigGetDeviceInfo(&mut request.header) };
            if status != 0 {
                return Err(WindowsError::new(
                    WindowsErrorKind::ApiFailure,
                    "DisplayConfigGetDeviceInfo advanced color",
                    Some(i64::from(status)),
                ));
            }
            // DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO documents bit 0 as
            // advancedColorSupported and bit 1 as advancedColorEnabled.
            let flags = unsafe { request.Anonymous.value };
            supported_path_count += u32::from(flags & 0x1 != 0);
            enabled_path_count += u32::from(flags & 0x2 != 0);
        }

        return Ok(AdvancedColorObservation {
            active_path_count: path_count,
            supported_path_count,
            enabled_path_count,
        });
    }

    Err(WindowsError::new(
        WindowsErrorKind::ApiFailure,
        "query stable active display configuration",
        Some(i64::from(ERROR_INSUFFICIENT_BUFFER.0)),
    ))
}

#[cfg(not(windows))]
pub fn read_active_advanced_color() -> WindowsResult<AdvancedColorObservation> {
    Err(WindowsError::unsupported(
        "DisplayConfigGetDeviceInfo advanced color",
    ))
}

#[cfg(windows)]
pub fn read_default_render_audio_endpoint() -> WindowsResult<DefaultRenderAudioObservation> {
    use windows::Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator},
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_MULTITHREADED,
        },
    };

    struct ComUninitializeGuard(bool);
    impl Drop for ComUninitializeGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let owns_initialization = initialized.is_ok();
    if initialized.is_err() && initialized != RPC_E_CHANGED_MODE {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "CoInitializeEx for Core Audio",
            Some(i64::from(initialized.0)),
        ));
    }
    let _guard = ComUninitializeGuard(owns_initialization);

    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER) }.map_err(
            |error| {
                WindowsError::new(
                    WindowsErrorKind::ApiFailure,
                    "CoCreateInstance MMDeviceEnumerator",
                    Some(i64::from(error.code().0)),
                )
            },
        )?;

    match unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) } {
        Ok(_endpoint) => Ok(DefaultRenderAudioObservation {
            endpoint_exists: true,
        }),
        // HRESULT_FROM_WIN32(ERROR_NOT_FOUND): no default endpoint is a known
        // state, not an API failure. No endpoint identifier or name is read.
        Err(error) if error.code().0 as u32 == 0x8007_0490 => Ok(DefaultRenderAudioObservation {
            endpoint_exists: false,
        }),
        Err(error) => Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "IMMDeviceEnumerator GetDefaultAudioEndpoint",
            Some(i64::from(error.code().0)),
        )),
    }
}

#[cfg(not(windows))]
pub fn read_default_render_audio_endpoint() -> WindowsResult<DefaultRenderAudioObservation> {
    Err(WindowsError::unsupported(
        "IMMDeviceEnumerator GetDefaultAudioEndpoint",
    ))
}
