use crate::action::{AudioOutputEndpointObservation, AudioOutputObservation};

use super::{WindowsError, WindowsErrorKind, WindowsResult};

const MAX_ACTIVE_RENDER_ENDPOINTS: usize = 64;
const MAX_FRIENDLY_NAME_UTF16: usize = 256;
const MAX_ENDPOINT_ID_UTF16: usize = 4_096;
const MAX_TOPOLOGY_ATTEMPTS: usize = 3;

struct RawAudioEndpoint {
    friendly_name: String,
    endpoint_id: Vec<u16>,
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

    #[cfg(windows)]
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
}
