use crate::action::{AudioOutputEndpointObservation, AudioOutputObservation};

use super::{WindowsError, WindowsErrorKind, WindowsResult};

const MAX_ACTIVE_RENDER_ENDPOINTS: usize = 64;
const MAX_FRIENDLY_NAME_UTF16: usize = 256;
const MAX_ENDPOINT_ID_UTF16: usize = 4_096;
const MAX_TOPOLOGY_ATTEMPTS: usize = 3;

/// The exact communications capture endpoint and its software-mute setting.
///
/// The endpoint identifier is kept as UTF-16 so rollback can address the same
/// Windows endpoint without lossy conversion or selecting a new default.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommsMicMuteState {
    pub device_id: Vec<u16>,
    pub muted: bool,
}

impl CommsMicMuteState {
    pub fn with_mute(&self, muted: bool) -> Self {
        Self {
            device_id: self.device_id.clone(),
            muted,
        }
    }

    pub fn fingerprint(&self) -> crate::backup::Fingerprint {
        let mut bytes = Vec::with_capacity(self.device_id.len() * 2 + 1);
        for code_unit in &self.device_id {
            bytes.extend_from_slice(&code_unit.to_le_bytes());
        }
        bytes.push(u8::from(self.muted));
        crate::backup::Fingerprint::of_bytes(&bytes)
    }
}

struct RawAudioEndpoint {
    friendly_name: String,
    endpoint_id: Vec<u16>,
}

fn validate_endpoint_id(endpoint_id: &[u16]) -> WindowsResult<()> {
    if endpoint_id.is_empty()
        || endpoint_id.len() > MAX_ENDPOINT_ID_UTF16
        || endpoint_id.contains(&0)
    {
        return Err(WindowsError::new(
            if endpoint_id.len() > MAX_ENDPOINT_ID_UTF16 {
                WindowsErrorKind::ResourceLimit
            } else {
                WindowsErrorKind::InvalidData
            },
            "validate Core Audio endpoint identifier",
            None,
        ));
    }
    Ok(())
}

fn bounded_friendly_name(wide: &[u16]) -> WindowsResult<String> {
    if wide.is_empty() || wide.len() > MAX_FRIENDLY_NAME_UTF16 || wide.contains(&0) {
        return Err(WindowsError::new(
            if wide.len() > MAX_FRIENDLY_NAME_UTF16 {
                WindowsErrorKind::ResourceLimit
            } else {
                WindowsErrorKind::InvalidData
            },
            "validate Core Audio endpoint friendly name",
            None,
        ));
    }
    let name = String::from_utf16(wide).map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::InvalidData,
            "decode Core Audio endpoint friendly name",
            None,
        )
    })?;
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate Core Audio endpoint friendly name",
            None,
        ));
    }
    Ok(name)
}

fn build_observation(
    endpoints: Vec<RawAudioEndpoint>,
    default_endpoint_id: Option<&[u16]>,
) -> WindowsResult<AudioOutputObservation> {
    if endpoints.len() > MAX_ACTIVE_RENDER_ENDPOINTS {
        return Err(WindowsError::new(
            WindowsErrorKind::ResourceLimit,
            "bound active Core Audio render endpoints",
            None,
        ));
    }

    if let Some(default_id) = default_endpoint_id {
        let matches = endpoints
            .iter()
            .filter(|endpoint| endpoint.endpoint_id == default_id)
            .count();
        if matches != 1 {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "match default Core Audio render endpoint",
                None,
            ));
        }
    }

    Ok(AudioOutputObservation {
        endpoints: endpoints
            .into_iter()
            .map(|endpoint| AudioOutputEndpointObservation {
                is_default: default_endpoint_id
                    .is_some_and(|default_id| endpoint.endpoint_id == default_id),
                friendly_name: endpoint.friendly_name,
            })
            .collect(),
    })
}

#[cfg(windows)]
fn api_error(operation: &'static str, error: windows::core::Error) -> WindowsError {
    let code = error.code().0;
    let kind = if code as u32 == 0x8007_0005 {
        WindowsErrorKind::AccessDenied
    } else if code as u32 == 0x8007_0490 {
        WindowsErrorKind::InvalidData
    } else {
        WindowsErrorKind::ApiFailure
    };
    WindowsError::new(kind, operation, Some(i64::from(code)))
}

#[cfg(windows)]
fn read_endpoint_id(endpoint: &windows::Win32::Media::Audio::IMMDevice) -> WindowsResult<Vec<u16>> {
    use windows::{core::PWSTR, Win32::System::Com::CoTaskMemFree};

    struct CoTaskMemWide(PWSTR);

    impl Drop for CoTaskMemWide {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CoTaskMemFree(Some(self.0.as_ptr().cast())) };
            }
        }
    }

    let pointer = unsafe { endpoint.GetId() }
        .map(CoTaskMemWide)
        .map_err(|error| api_error("IMMDevice GetId", error))?;
    if pointer.0.is_null() {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "validate Core Audio endpoint identifier",
            None,
        ));
    }

    let mut endpoint_id = Vec::new();
    for index in 0..=MAX_ENDPOINT_ID_UTF16 {
        let code_unit = unsafe { *pointer.0.as_ptr().add(index) };
        if code_unit == 0 {
            if endpoint_id.is_empty() {
                return Err(WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "validate Core Audio endpoint identifier",
                    None,
                ));
            }
            validate_endpoint_id(&endpoint_id)?;
            return Ok(endpoint_id);
        }
        if index == MAX_ENDPOINT_ID_UTF16 {
            break;
        }
        endpoint_id.push(code_unit);
    }

    Err(WindowsError::new(
        WindowsErrorKind::ResourceLimit,
        "bound Core Audio endpoint identifier",
        None,
    ))
}

#[cfg(windows)]
fn audio_enumerator() -> WindowsResult<windows::Win32::Media::Audio::IMMDeviceEnumerator> {
    use windows::Win32::{
        Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator},
        System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
    };

    unsafe {
        CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)
    }
    .map_err(|error| api_error("CoCreateInstance MMDeviceEnumerator", error))
}

#[cfg(windows)]
fn ensure_active_capture_endpoint(
    endpoint: &windows::Win32::Media::Audio::IMMDevice,
) -> WindowsResult<()> {
    use windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE;

    let state = unsafe { endpoint.GetState() }
        .map_err(|error| api_error("IMMDevice GetState for communications microphone", error))?;
    if state != DEVICE_STATE_ACTIVE {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "communications microphone endpoint is not active",
            None,
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn endpoint_mute_state(
    endpoint: &windows::Win32::Media::Audio::IMMDevice,
) -> WindowsResult<CommsMicMuteState> {
    use windows::Win32::{Media::Audio::Endpoints::IAudioEndpointVolume, System::Com::CLSCTX_ALL};

    ensure_active_capture_endpoint(endpoint)?;
    let device_id = read_endpoint_id(endpoint)?;
    let volume: IAudioEndpointVolume = unsafe { endpoint.Activate(CLSCTX_ALL, None) }
        .map_err(|error| api_error("IMMDevice Activate IAudioEndpointVolume", error))?;
    let muted = unsafe { volume.GetMute() }
        .map_err(|error| api_error("IAudioEndpointVolume GetMute", error))?
        .as_bool();
    Ok(CommsMicMuteState { device_id, muted })
}

#[cfg(windows)]
fn default_comms_capture_endpoint(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
) -> WindowsResult<windows::Win32::Media::Audio::IMMDevice> {
    use windows::Win32::Media::Audio::{eCapture, eCommunications};

    unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications) }.map_err(|error| {
        api_error(
            "IMMDeviceEnumerator GetDefaultAudioEndpoint eCapture eCommunications",
            error,
        )
    })
}

#[cfg(windows)]
fn endpoint_by_id(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
    endpoint_id: &[u16],
) -> WindowsResult<windows::Win32::Media::Audio::IMMDevice> {
    use windows::core::PCWSTR;

    validate_endpoint_id(endpoint_id)?;
    let mut terminated = Vec::with_capacity(endpoint_id.len() + 1);
    terminated.extend_from_slice(endpoint_id);
    terminated.push(0);
    unsafe { enumerator.GetDevice(PCWSTR::from_raw(terminated.as_ptr())) }
        .map_err(|error| api_error("IMMDeviceEnumerator GetDevice by saved identifier", error))
}

#[cfg(windows)]
fn set_endpoint_mute(
    endpoint: &windows::Win32::Media::Audio::IMMDevice,
    muted: bool,
) -> WindowsResult<()> {
    use windows::Win32::{
        Foundation::BOOL, Media::Audio::Endpoints::IAudioEndpointVolume, System::Com::CLSCTX_ALL,
    };

    let volume: IAudioEndpointVolume = unsafe { endpoint.Activate(CLSCTX_ALL, None) }
        .map_err(|error| api_error("IMMDevice Activate IAudioEndpointVolume", error))?;
    unsafe { volume.SetMute(BOOL::from(muted), std::ptr::null()) }
        .map_err(|error| api_error("IAudioEndpointVolume SetMute", error))
}

#[cfg(windows)]
fn replace_endpoint_mute(
    endpoint: &windows::Win32::Media::Audio::IMMDevice,
    expected: &CommsMicMuteState,
    muted: bool,
) -> WindowsResult<CommsMicMuteState> {
    let current = endpoint_mute_state(endpoint)?;
    if current != *expected {
        return Err(WindowsError::new(
            WindowsErrorKind::ExternalConflict,
            "communications microphone changed by something else",
            None,
        ));
    }
    set_endpoint_mute(endpoint, muted)?;
    let changed = endpoint_mute_state(endpoint)?;
    if changed.device_id != expected.device_id || changed.muted != muted {
        return Err(WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "verify communications microphone mute write",
            None,
        ));
    }
    Ok(changed)
}

#[cfg(windows)]
fn run_on_audio_com_thread<T, F>(
    thread_name: &'static str,
    join_operation: &'static str,
    operation: F,
) -> WindowsResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> WindowsResult<T> + Send + 'static,
{
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    std::thread::Builder::new()
        .name(thread_name.to_owned())
        .spawn(move || {
            let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            if initialized.is_err() {
                return Err(WindowsError::new(
                    WindowsErrorKind::ApiFailure,
                    "CoInitializeEx for Core Audio communications microphone",
                    Some(i64::from(initialized.0)),
                ));
            }
            struct Uninitialize;
            impl Drop for Uninitialize {
                fn drop(&mut self) {
                    unsafe { CoUninitialize() };
                }
            }
            let _uninitialize = Uninitialize;
            operation()
        })
        .map_err(|error| WindowsError::io("spawn Core Audio COM thread", &error))?
        .join()
        .map_err(|_| WindowsError::new(WindowsErrorKind::ApiFailure, join_operation, None))?
}

/// Reads the current `eCommunications` default capture endpoint and its mute bit.
#[cfg(windows)]
pub fn read_default_comms_mic_mute() -> WindowsResult<CommsMicMuteState> {
    run_on_audio_com_thread(
        "totonoe-comms-mic-read-default",
        "join Core Audio default communications microphone read thread",
        || {
            let enumerator = audio_enumerator()?;
            let endpoint = default_comms_capture_endpoint(&enumerator)?;
            endpoint_mute_state(&endpoint)
        },
    )
}

/// Re-reads one saved endpoint by its exact identifier.
#[cfg(windows)]
pub fn read_comms_mic_mute_by_id(endpoint_id: &[u16]) -> WindowsResult<CommsMicMuteState> {
    validate_endpoint_id(endpoint_id)?;
    let endpoint_id = endpoint_id.to_vec();
    run_on_audio_com_thread(
        "totonoe-comms-mic-read-saved",
        "join Core Audio saved communications microphone read thread",
        move || {
            let enumerator = audio_enumerator()?;
            let endpoint = endpoint_by_id(&enumerator, &endpoint_id)?;
            let observed = endpoint_mute_state(&endpoint)?;
            if observed.device_id != endpoint_id {
                return Err(WindowsError::new(
                    WindowsErrorKind::InvalidData,
                    "match saved communications microphone endpoint identifier",
                    None,
                ));
            }
            Ok(observed)
        },
    )
}

/// Changes the default communications microphone only if its exact saved state
/// is still current.
#[cfg(windows)]
pub fn replace_default_comms_mic_mute(
    expected: &CommsMicMuteState,
    muted: bool,
) -> WindowsResult<CommsMicMuteState> {
    validate_endpoint_id(&expected.device_id)?;
    let expected = expected.clone();
    run_on_audio_com_thread(
        "totonoe-comms-mic-write-default",
        "join Core Audio default communications microphone write thread",
        move || {
            let enumerator = audio_enumerator()?;
            let endpoint = default_comms_capture_endpoint(&enumerator)?;
            replace_endpoint_mute(&endpoint, &expected, muted)
        },
    )
}

/// Changes only the saved endpoint. It never resolves or touches the current
/// default endpoint, so a default-device switch cannot redirect rollback.
#[cfg(windows)]
pub fn replace_comms_mic_mute_by_id(
    expected: &CommsMicMuteState,
    muted: bool,
) -> WindowsResult<CommsMicMuteState> {
    validate_endpoint_id(&expected.device_id)?;
    let expected = expected.clone();
    run_on_audio_com_thread(
        "totonoe-comms-mic-write-saved",
        "join Core Audio saved communications microphone write thread",
        move || {
            let enumerator = audio_enumerator()?;
            let endpoint = endpoint_by_id(&enumerator, &expected.device_id)?;
            replace_endpoint_mute(&endpoint, &expected, muted)
        },
    )
}

#[cfg(not(windows))]
pub fn read_default_comms_mic_mute() -> WindowsResult<CommsMicMuteState> {
    Err(WindowsError::unsupported(
        "read default communications microphone mute",
    ))
}

#[cfg(not(windows))]
pub fn read_comms_mic_mute_by_id(_endpoint_id: &[u16]) -> WindowsResult<CommsMicMuteState> {
    Err(WindowsError::unsupported(
        "read saved communications microphone mute",
    ))
}

#[cfg(not(windows))]
pub fn replace_default_comms_mic_mute(
    _expected: &CommsMicMuteState,
    _muted: bool,
) -> WindowsResult<CommsMicMuteState> {
    Err(WindowsError::unsupported(
        "write default communications microphone mute",
    ))
}

#[cfg(not(windows))]
pub fn replace_comms_mic_mute_by_id(
    _expected: &CommsMicMuteState,
    _muted: bool,
) -> WindowsResult<CommsMicMuteState> {
    Err(WindowsError::unsupported(
        "write saved communications microphone mute",
    ))
}

#[cfg(windows)]
fn read_friendly_name(endpoint: &windows::Win32::Media::Audio::IMMDevice) -> WindowsResult<String> {
    use windows::{
        core::BSTR,
        Win32::{Devices::FunctionDiscovery::PKEY_Device_FriendlyName, System::Com::STGM_READ},
    };

    let store = unsafe { endpoint.OpenPropertyStore(STGM_READ) }
        .map_err(|error| api_error("IMMDevice OpenPropertyStore", error))?;
    let value = unsafe { store.GetValue(&PKEY_Device_FriendlyName) }
        .map_err(|error| api_error("IPropertyStore GetValue friendly name", error))?;
    let name = BSTR::try_from(&value)
        .map_err(|error| api_error("convert Core Audio friendly name property", error))?;
    bounded_friendly_name(name.as_wide())
}

#[cfg(windows)]
fn default_endpoint_id(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
) -> WindowsResult<Option<Vec<u16>>> {
    use windows::Win32::Media::Audio::{eConsole, eRender};

    match unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) } {
        Ok(endpoint) => read_endpoint_id(&endpoint).map(Some),
        // HRESULT_FROM_WIN32(ERROR_NOT_FOUND) means that Windows currently has
        // no eConsole default render endpoint. This is an observed state.
        Err(error) if error.code().0 as u32 == 0x8007_0490 => Ok(None),
        Err(error) => Err(api_error(
            "IMMDeviceEnumerator GetDefaultAudioEndpoint",
            error,
        )),
    }
}

#[cfg(windows)]
fn enumerate_active_render_endpoints(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
) -> WindowsResult<Vec<RawAudioEndpoint>> {
    use windows::Win32::Media::Audio::{eRender, DEVICE_STATE_ACTIVE};

    let collection = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }
        .map_err(|error| api_error("IMMDeviceEnumerator EnumAudioEndpoints", error))?;
    let count = unsafe { collection.GetCount() }
        .map_err(|error| api_error("IMMDeviceCollection GetCount", error))?;
    if count as usize > MAX_ACTIVE_RENDER_ENDPOINTS {
        return Err(WindowsError::new(
            WindowsErrorKind::ResourceLimit,
            "bound active Core Audio render endpoints",
            None,
        ));
    }

    let mut endpoints = Vec::with_capacity(count as usize);
    for index in 0..count {
        let endpoint = unsafe { collection.Item(index) }
            .map_err(|error| api_error("IMMDeviceCollection Item", error))?;
        endpoints.push(RawAudioEndpoint {
            friendly_name: read_friendly_name(&endpoint)?,
            endpoint_id: read_endpoint_id(&endpoint)?,
        });
    }
    Ok(endpoints)
}

/// Enumerates active render endpoints and marks the current eConsole default.
///
/// Endpoint identifiers are used only for an in-process exact comparison and
/// are freed before this function returns. They never enter the observation.
#[cfg(windows)]
pub fn read_audio_output_observation() -> WindowsResult<AudioOutputObservation> {
    use windows::Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator},
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
            "CoInitializeEx for Core Audio output observation",
            Some(i64::from(initialized.0)),
        ));
    }
    let _guard = ComUninitializeGuard(owns_initialization);

    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| api_error("CoCreateInstance MMDeviceEnumerator", error))?;

    for _ in 0..MAX_TOPOLOGY_ATTEMPTS {
        let default_before = default_endpoint_id(&enumerator)?;
        let endpoints = enumerate_active_render_endpoints(&enumerator)?;
        let default_after = default_endpoint_id(&enumerator)?;
        if default_before != default_after {
            continue;
        }
        match build_observation(endpoints, default_before.as_deref()) {
            Ok(observation) => return Ok(observation),
            Err(error)
                if error.kind == WindowsErrorKind::InvalidData
                    && error.operation == "match default Core Audio render endpoint" =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
    }

    Err(WindowsError::new(
        WindowsErrorKind::ApiFailure,
        "read stable Core Audio output topology",
        None,
    ))
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppVolumeSessionState {
    pub device_id: Vec<u16>,
    pub session_instance_id: Vec<u16>,
    pub volume: f32,
    pub muted: bool,
}

impl AppVolumeSessionState {
    pub fn fingerprint(&self) -> crate::backup::Fingerprint {
        let device_id: Vec<_> = self
            .device_id
            .iter()
            .flat_map(|code_unit| code_unit.to_le_bytes())
            .collect();
        let session_instance_id: Vec<_> = self
            .session_instance_id
            .iter()
            .flat_map(|code_unit| code_unit.to_le_bytes())
            .collect();
        let volume = self.volume.to_le_bytes();
        let muted = [u8::from(self.muted)];
        crate::backup::Fingerprint::of_parts([
            device_id.as_slice(),
            session_instance_id.as_slice(),
            volume.as_slice(),
            muted.as_slice(),
        ])
    }
}

impl Eq for AppVolumeSessionState {}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppVolumeRestoreOutcome {
    pub success_count: usize,
    pub missing_count: usize,
}

fn merge_endpoint_session_reads(
    reads: impl IntoIterator<Item = WindowsResult<Vec<AppVolumeSessionState>>>,
) -> WindowsResult<Vec<AppVolumeSessionState>> {
    let mut all_sessions = Vec::new();
    for read in reads {
        all_sessions.extend(read?);
    }
    Ok(all_sessions)
}

fn write_volume_then_mute_with_compensation(
    original_volume: f32,
    original_muted: bool,
    target_volume: f32,
    target_muted: bool,
    mut read: impl FnMut() -> WindowsResult<(f32, bool)>,
    mut set_volume: impl FnMut(f32) -> WindowsResult<()>,
    mut set_mute: impl FnMut(bool) -> WindowsResult<()>,
) -> WindowsResult<()> {
    set_volume(target_volume)?;
    if let Err(mute_error) = set_mute(target_muted) {
        // A failed COM setter is not proof that nothing changed. Restore both
        // values, including the failed field, and accept the original error only
        // after an independent read observes the exact pre-write pair.
        let _ = set_volume(original_volume);
        let _ = set_mute(original_muted);
        let restored = read().is_ok_and(|(volume, muted)| {
            (volume - original_volume).abs() <= f32::EPSILON && muted == original_muted
        });
        if !restored {
            return Err(WindowsError::new(
                WindowsErrorKind::RecoveryRequired,
                "verify app volume compensation after mute write failure",
                mute_error.os_code,
            ));
        }
        return Err(mute_error);
    }
    Ok(())
}

#[cfg(windows)]
fn ensure_active_render_endpoint(
    endpoint: &windows::Win32::Media::Audio::IMMDevice,
) -> WindowsResult<()> {
    use windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE;

    let state = unsafe { endpoint.GetState() }
        .map_err(|error| api_error("IMMDevice GetState for render endpoint", error))?;
    if state != DEVICE_STATE_ACTIVE {
        return Err(WindowsError::new(
            WindowsErrorKind::InvalidData,
            "render endpoint is not active",
            None,
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn read_endpoint_sessions(
    endpoint: &windows::Win32::Media::Audio::IMMDevice,
) -> WindowsResult<Vec<AppVolumeSessionState>> {
    use windows::{
        core::Interface,
        Win32::{
            Media::Audio::{
                IAudioSessionControl, IAudioSessionControl2, IAudioSessionEnumerator,
                IAudioSessionManager2, ISimpleAudioVolume,
            },
            System::Com::{CoTaskMemFree, CLSCTX_ALL},
        },
    };

    ensure_active_render_endpoint(endpoint)?;
    let device_id = read_endpoint_id(endpoint)?;
    let manager: IAudioSessionManager2 = unsafe { endpoint.Activate(CLSCTX_ALL, None) }
        .map_err(|error| api_error("IMMDevice Activate IAudioSessionManager2", error))?;
    let enumerator: IAudioSessionEnumerator = unsafe { manager.GetSessionEnumerator() }
        .map_err(|error| api_error("IAudioSessionManager2 GetSessionEnumerator", error))?;

    let count = unsafe { enumerator.GetCount() }
        .map_err(|error| api_error("IAudioSessionEnumerator GetCount", error))?;

    let mut sessions = Vec::new();
    for index in 0..count {
        let control: IAudioSessionControl = match unsafe { enumerator.GetSession(index) } {
            Ok(c) => c,
            Err(_) => continue,
        };

        let control2: IAudioSessionControl2 = match control.cast() {
            Ok(c) => c,
            Err(_) => continue,
        };

        if unsafe { control2.IsSystemSoundsSession() } == windows::core::HRESULT(0) {
            continue;
        }

        let instance_id_pwstr = match unsafe { control2.GetSessionInstanceIdentifier() } {
            Ok(pwstr) => pwstr,
            Err(_) => continue,
        };

        if instance_id_pwstr.is_null() {
            continue;
        }

        struct PWStrGuard(windows::core::PWSTR);
        impl Drop for PWStrGuard {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    unsafe { CoTaskMemFree(Some(self.0.as_ptr().cast())) };
                }
            }
        }
        let guard = PWStrGuard(instance_id_pwstr);

        let mut instance_id = Vec::new();
        for idx in 0..MAX_ENDPOINT_ID_UTF16 {
            let code_unit = unsafe { *guard.0.as_ptr().add(idx) };
            if code_unit == 0 {
                break;
            }
            instance_id.push(code_unit);
        }

        if instance_id.is_empty() {
            continue;
        }

        let simple_volume: ISimpleAudioVolume = match control.cast() {
            Ok(v) => v,
            Err(_) => continue,
        };

        let volume = unsafe { simple_volume.GetMasterVolume() }
            .map_err(|error| api_error("ISimpleAudioVolume GetMasterVolume", error))?;

        let muted = unsafe { simple_volume.GetMute() }
            .map_err(|error| api_error("ISimpleAudioVolume GetMute", error))?
            .as_bool();

        sessions.push(AppVolumeSessionState {
            device_id: device_id.clone(),
            session_instance_id: instance_id,
            volume,
            muted,
        });
    }

    Ok(sessions)
}

/// Reads the active app volume mixer sessions across all active render endpoints.
#[cfg(windows)]
pub fn read_app_volume_sessions() -> WindowsResult<Vec<AppVolumeSessionState>> {
    run_on_audio_com_thread(
        "totonoe-app-volume-read",
        "join Core Audio app volume read thread",
        || {
            let enumerator = audio_enumerator()?;
            let endpoints = enumerate_active_render_endpoints(&enumerator)?;
            merge_endpoint_session_reads(endpoints.into_iter().map(|raw_endpoint| {
                let endpoint = endpoint_by_id(&enumerator, &raw_endpoint.endpoint_id)?;
                read_endpoint_sessions(&endpoint)
            }))
        },
    )
}

/// Restores saved app volume session states.
#[cfg(windows)]
pub fn restore_app_volume_sessions(
    expected: &[AppVolumeSessionState],
) -> WindowsResult<AppVolumeRestoreOutcome> {
    let expected = expected.to_vec();
    run_on_audio_com_thread(
        "totonoe-app-volume-write",
        "join Core Audio app volume write thread",
        move || {
            use windows::{
                core::Interface,
                Win32::{
                    Foundation::BOOL,
                    Media::Audio::{
                        IAudioSessionControl, IAudioSessionControl2, IAudioSessionEnumerator,
                        IAudioSessionManager2, ISimpleAudioVolume,
                    },
                    System::Com::{CoTaskMemFree, CLSCTX_ALL},
                },
            };

            let enumerator = audio_enumerator()?;
            let raw_endpoints = enumerate_active_render_endpoints(&enumerator)?;

            let mut success_count = 0;
            let mut missing_count = 0;

            for exp in &expected {
                let mut found = false;
                for raw_ep in &raw_endpoints {
                    if raw_ep.endpoint_id == exp.device_id {
                        if let Ok(endpoint) = endpoint_by_id(&enumerator, &raw_ep.endpoint_id) {
                            let manager: IAudioSessionManager2 =
                                match unsafe { endpoint.Activate(CLSCTX_ALL, None) } {
                                    Ok(m) => m,
                                    Err(_) => continue,
                                };
                            let enum_mgr: IAudioSessionEnumerator =
                                match unsafe { manager.GetSessionEnumerator() } {
                                    Ok(e) => e,
                                    Err(_) => continue,
                                };
                            let count = match unsafe { enum_mgr.GetCount() } {
                                Ok(c) => c,
                                Err(_) => continue,
                            };

                            for i in 0..count {
                                let control: IAudioSessionControl =
                                    match unsafe { enum_mgr.GetSession(i) } {
                                        Ok(c) => c,
                                        Err(_) => continue,
                                    };
                                let control2: IAudioSessionControl2 = match control.cast() {
                                    Ok(c) => c,
                                    Err(_) => continue,
                                };
                                let pwstr = match unsafe { control2.GetSessionInstanceIdentifier() }
                                {
                                    Ok(p) => p,
                                    Err(_) => continue,
                                };
                                if pwstr.is_null() {
                                    continue;
                                }
                                struct PWStrGuard(windows::core::PWSTR);
                                impl Drop for PWStrGuard {
                                    fn drop(&mut self) {
                                        if !self.0.is_null() {
                                            unsafe { CoTaskMemFree(Some(self.0.as_ptr().cast())) };
                                        }
                                    }
                                }
                                let _guard = PWStrGuard(pwstr);
                                let mut instance_id = Vec::new();
                                for idx in 0..MAX_ENDPOINT_ID_UTF16 {
                                    let code_unit = unsafe { *pwstr.as_ptr().add(idx) };
                                    if code_unit == 0 {
                                        break;
                                    }
                                    instance_id.push(code_unit);
                                }

                                if instance_id == exp.session_instance_id {
                                    found = true;
                                    let simple_vol: ISimpleAudioVolume = match control.cast() {
                                        Ok(v) => v,
                                        Err(_) => continue,
                                    };
                                    let original_volume = unsafe { simple_vol.GetMasterVolume() }
                                        .map_err(|err| {
                                        api_error("ISimpleAudioVolume GetMasterVolume", err)
                                    })?;
                                    let original_muted = unsafe { simple_vol.GetMute() }
                                        .map_err(|err| {
                                            api_error("ISimpleAudioVolume GetMute", err)
                                        })?
                                        .as_bool();
                                    write_volume_then_mute_with_compensation(
                                        original_volume,
                                        original_muted,
                                        exp.volume,
                                        exp.muted,
                                        || {
                                            let volume = unsafe { simple_vol.GetMasterVolume() }
                                                .map_err(|err| {
                                                    api_error(
                                                        "ISimpleAudioVolume GetMasterVolume",
                                                        err,
                                                    )
                                                })?;
                                            let muted = unsafe { simple_vol.GetMute() }
                                                .map_err(|err| {
                                                    api_error("ISimpleAudioVolume GetMute", err)
                                                })?
                                                .as_bool();
                                            Ok((volume, muted))
                                        },
                                        |volume| {
                                            unsafe {
                                                simple_vol.SetMasterVolume(volume, std::ptr::null())
                                            }
                                            .map_err(
                                                |err| {
                                                    api_error(
                                                        "ISimpleAudioVolume SetMasterVolume",
                                                        err,
                                                    )
                                                },
                                            )
                                        },
                                        |muted| {
                                            unsafe {
                                                simple_vol
                                                    .SetMute(BOOL::from(muted), std::ptr::null())
                                            }
                                            .map_err(
                                                |err| api_error("ISimpleAudioVolume SetMute", err),
                                            )
                                        },
                                    )?;

                                    success_count += 1;
                                    break;
                                }
                            }
                        }
                    }
                    if found {
                        break;
                    }
                }
                if !found {
                    missing_count += 1;
                }
            }

            Ok(AppVolumeRestoreOutcome {
                success_count,
                missing_count,
            })
        },
    )
}

#[cfg(not(windows))]
pub fn read_app_volume_sessions() -> WindowsResult<Vec<AppVolumeSessionState>> {
    Err(WindowsError::unsupported(
        "read Core Audio app volume sessions",
    ))
}

#[cfg(not(windows))]
pub fn restore_app_volume_sessions(
    _expected: &[AppVolumeSessionState],
) -> WindowsResult<AppVolumeRestoreOutcome> {
    Err(WindowsError::unsupported(
        "restore Core Audio app volume sessions",
    ))
}

#[cfg(not(windows))]
pub fn read_audio_output_observation() -> WindowsResult<AudioOutputObservation> {
    Err(WindowsError::unsupported(
        "read Core Audio output observation",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_endpoint(name: &str, id: &[u16]) -> RawAudioEndpoint {
        RawAudioEndpoint {
            friendly_name: name.to_owned(),
            endpoint_id: id.to_vec(),
        }
    }

    #[test]
    fn exact_default_is_marked_without_exposing_endpoint_ids() {
        let observation = build_observation(
            vec![
                raw_endpoint("Speakers", &[1, 2, 3]),
                raw_endpoint("Headset", &[4, 5, 6]),
            ],
            Some(&[4, 5, 6]),
        )
        .expect("build observation");

        assert_eq!(observation.endpoints.len(), 2);
        assert!(!observation.endpoints[0].is_default);
        assert!(observation.endpoints[1].is_default);

        let serialized = serde_json::to_value(&observation).expect("serialize observation");
        let endpoint = serialized["endpoints"][0]
            .as_object()
            .expect("endpoint object");
        assert_eq!(endpoint.len(), 2);
        assert!(endpoint.contains_key("friendly_name"));
        assert!(endpoint.contains_key("is_default"));
        assert!(!serialized.to_string().contains("endpoint_id"));
    }

    #[test]
    fn missing_default_marks_no_endpoint() {
        let observation = build_observation(
            vec![
                raw_endpoint("Speakers", &[1]),
                raw_endpoint("Headset", &[2]),
            ],
            None,
        )
        .expect("build observation");

        assert!(observation
            .endpoints
            .iter()
            .all(|endpoint| !endpoint.is_default));
    }

    #[test]
    fn unmatched_or_duplicate_default_is_rejected() {
        let missing = build_observation(vec![raw_endpoint("Speakers", &[1])], Some(&[2]))
            .expect_err("unmatched default must fail");
        assert_eq!(missing.kind, WindowsErrorKind::InvalidData);

        let duplicate = build_observation(
            vec![
                raw_endpoint("Speakers A", &[1]),
                raw_endpoint("Speakers B", &[1]),
            ],
            Some(&[1]),
        )
        .expect_err("duplicate default must fail");
        assert_eq!(duplicate.kind, WindowsErrorKind::InvalidData);
    }

    #[test]
    fn friendly_names_are_strictly_bounded_utf16() {
        assert_eq!(
            bounded_friendly_name(&vec![u16::from(b'a'); MAX_FRIENDLY_NAME_UTF16])
                .expect("name at limit")
                .len(),
            MAX_FRIENDLY_NAME_UTF16
        );

        let too_long = bounded_friendly_name(&vec![u16::from(b'a'); MAX_FRIENDLY_NAME_UTF16 + 1])
            .expect_err("overlong name must fail");
        assert_eq!(too_long.kind, WindowsErrorKind::ResourceLimit);
        assert_eq!(
            bounded_friendly_name(&[])
                .expect_err("empty name must fail")
                .kind,
            WindowsErrorKind::InvalidData
        );
        assert_eq!(
            bounded_friendly_name(&[u16::from(b'a'), 0])
                .expect_err("embedded nul must fail")
                .kind,
            WindowsErrorKind::InvalidData
        );
        assert_eq!(
            bounded_friendly_name(&[u16::from(b'a'), u16::from(b'\n')])
                .expect_err("control characters must fail")
                .kind,
            WindowsErrorKind::InvalidData
        );
        assert_eq!(
            bounded_friendly_name(&[0xD800])
                .expect_err("invalid utf16 must fail")
                .kind,
            WindowsErrorKind::InvalidData
        );
    }

    #[test]
    fn endpoint_count_is_rejected_instead_of_truncated() {
        let endpoints = (0..=MAX_ACTIVE_RENDER_ENDPOINTS)
            .map(|index| raw_endpoint("Output", &[index as u16 + 1]))
            .collect();
        let error = build_observation(endpoints, None).expect_err("over limit must fail");
        assert_eq!(error.kind, WindowsErrorKind::ResourceLimit);
    }

    #[test]
    fn endpoint_session_read_failure_is_not_reported_as_zero_sessions() {
        let empty = merge_endpoint_session_reads(std::iter::empty())
            .expect("a completed read of zero endpoints is a valid empty result");
        assert!(empty.is_empty());

        let failure = WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "injected endpoint session read failure",
            Some(1),
        );
        let result = merge_endpoint_session_reads([
            Ok(Vec::new()),
            Err(failure),
            Ok(vec![AppVolumeSessionState {
                device_id: vec![1],
                session_instance_id: vec![2],
                volume: 0.5,
                muted: false,
            }]),
        ]);

        let error = result.expect_err("an incomplete read must remain an error");
        assert_eq!(error.kind, WindowsErrorKind::ApiFailure);
        assert_eq!(error.operation, "injected endpoint session read failure");
    }

    #[test]
    fn mute_failure_restores_the_volume_written_immediately_before_it() {
        let state = std::rc::Rc::new(std::cell::RefCell::new((0.25, false)));
        let volume_writes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mute_writes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let read_state = std::rc::Rc::clone(&state);
        let volume_state = std::rc::Rc::clone(&state);
        let volume_calls = std::rc::Rc::clone(&volume_writes);
        let mute_state = std::rc::Rc::clone(&state);
        let mute_calls = std::rc::Rc::clone(&mute_writes);
        let error = write_volume_then_mute_with_compensation(
            0.25,
            false,
            0.75,
            true,
            move || Ok(*read_state.borrow()),
            move |volume| {
                volume_calls.borrow_mut().push(volume);
                volume_state.borrow_mut().0 = volume;
                Ok(())
            },
            move |muted| {
                let mut calls = mute_calls.borrow_mut();
                calls.push(muted);
                mute_state.borrow_mut().1 = muted;
                if calls.len() == 1 {
                    Err(WindowsError::new(
                        WindowsErrorKind::ApiFailure,
                        "injected mute write failure",
                        Some(456),
                    ))
                } else {
                    Ok(())
                }
            },
        )
        .expect_err("mute write is injected to fail");

        assert_eq!(error.operation, "injected mute write failure");
        assert_eq!(*state.borrow(), (0.25, false));
        assert_eq!(*volume_writes.borrow(), vec![0.75, 0.25]);
        assert_eq!(*mute_writes.borrow(), vec![true, false]);
    }

    #[test]
    fn communications_mic_state_fingerprint_covers_id_and_mute() {
        let base = CommsMicMuteState {
            device_id: vec![1, 2, 3],
            muted: false,
        };
        assert_ne!(base.fingerprint(), base.with_mute(true).fingerprint());
        assert_ne!(
            base.fingerprint(),
            CommsMicMuteState {
                device_id: vec![1, 2, 4],
                muted: false,
            }
            .fingerprint()
        );
    }

    #[test]
    fn saved_endpoint_identifiers_are_strictly_bounded() {
        assert!(validate_endpoint_id(&[1]).is_ok());
        assert_eq!(
            validate_endpoint_id(&[])
                .expect_err("empty identifier")
                .kind,
            WindowsErrorKind::InvalidData
        );
        assert_eq!(
            validate_endpoint_id(&[1, 0])
                .expect_err("embedded nul")
                .kind,
            WindowsErrorKind::InvalidData
        );
        assert_eq!(
            validate_endpoint_id(&vec![1; MAX_ENDPOINT_ID_UTF16 + 1])
                .expect_err("overlong identifier")
                .kind,
            WindowsErrorKind::ResourceLimit
        );
    }

    #[cfg(windows)]
    /// 保存した端末が見つからないとき、**既定の端末へすり替わらない**こと。
    ///
    /// これがこの機能で一番怖いところ。抜き差しで既定が変わったあとに
    /// 「戻す」を押したら別のマイクがミュートされる、という事故になる。
    /// 存在しない ID を渡して、既定の状態が返ってこないことを実機で確かめる。
    #[cfg(windows)]
    #[test]
    #[ignore = "実機の音声端末を読む"]
    fn a_saved_endpoint_that_is_gone_never_falls_back_to_the_current_default() {
        let Ok(current) = read_default_comms_mic_mute() else {
            println!("EVIDENCE: comms_mic_fallback skipped (既定の通話マイクが無い)");
            return;
        };
        // 実在しない端末 ID。形は本物に似せる。
        let bogus: Vec<u16> = "{0.0.1.00000000}.{00000000-0000-0000-0000-000000000000}"
            .encode_utf16()
            .collect();
        assert_ne!(
            bogus, current.device_id,
            "取り違え用の ID が本物と一致している"
        );

        let outcome = read_comms_mic_mute_by_id(&bogus);
        println!(
            "EVIDENCE: comms_mic_fallback bogus_id_read_is_err={} detail={:?}",
            outcome.is_err(),
            outcome.as_ref().err()
        );
        let observed = outcome.err();
        assert!(
            observed.is_some(),
            "見つからない端末で既定の状態を返してはいけない"
        );
    }

    #[test]
    #[ignore = "real-machine Core Audio read-only smoke"]
    fn real_machine_audio_output_smoke_prints_no_names() {
        let observation =
            read_audio_output_observation().expect("read active Core Audio render endpoints");
        let default_exists = observation
            .endpoints
            .iter()
            .any(|endpoint| endpoint.is_default);
        println!(
            "audio_output_count={} default_exists={}",
            observation.endpoints.len(),
            default_exists
        );
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "実機のアプリ別音量ミキサーを読む/変更する"]
    fn real_machine_app_volume_mixer_smoke() {
        let sessions = match read_app_volume_sessions() {
            Ok(s) => s,
            Err(e) => {
                println!(
                    "EVIDENCE: app_volume_mixer measured=false reason=\"Core Audio読み取り失敗: {:?}\"",
                    e
                );
                return;
            }
        };

        let count = sessions.len();
        if count == 0 {
            println!("EVIDENCE: app_volume_mixer measured=false reason=\"アクティブなアプリ音声セッションが0件\"");
            return;
        }

        let target_original = sessions[0].clone();
        let original_vol = target_original.volume;

        struct RestoreGuard {
            session: Option<AppVolumeSessionState>,
        }
        impl Drop for RestoreGuard {
            fn drop(&mut self) {
                if let Some(session) = self.session.take() {
                    let _ = restore_app_volume_sessions(&[session]);
                }
            }
        }

        let mut guard = RestoreGuard {
            session: Some(target_original.clone()),
        };

        let new_vol = if original_vol > 0.5 {
            (original_vol - 0.2).max(0.0)
        } else {
            (original_vol + 0.2).min(1.0)
        };
        assert_ne!(new_vol, original_vol, "変更前後の音量が同じになっている");

        let mut changed_session = target_original.clone();
        changed_session.volume = new_vol;

        restore_app_volume_sessions(&[changed_session]).expect("一時変更書き込み");

        let after_change_sessions = read_app_volume_sessions().expect("変更後の読み直し");
        let changed_vol = after_change_sessions
            .iter()
            .find(|s| {
                s.device_id == target_original.device_id
                    && s.session_instance_id == target_original.session_instance_id
            })
            .map(|s| s.volume)
            .expect("対象セッションが存在すること");

        let restore_target = guard.session.take().unwrap();
        restore_app_volume_sessions(&[restore_target]).expect("復元書き出し");

        let after_restore_sessions = read_app_volume_sessions().expect("復元後の読み直し");
        let restored_vol = after_restore_sessions
            .iter()
            .find(|s| {
                s.device_id == target_original.device_id
                    && s.session_instance_id == target_original.session_instance_id
            })
            .map(|s| s.volume)
            .expect("対象セッションが存在すること");

        println!(
            "EVIDENCE: app_volume_mixer count={} before={:.2} changed={:.2} restored={:.2}",
            count, original_vol, changed_vol, restored_vol
        );

        assert!(
            (changed_vol - new_vol).abs() < 0.05,
            "変更後の音量が反映されていない"
        );
        assert!(
            (restored_vol - original_vol).abs() < 0.05,
            "復元後の音量が元に戻っていない"
        );
    }
}
