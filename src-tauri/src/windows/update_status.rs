//! Read-only Windows Update Agent observations.

use super::{WindowsError, WindowsErrorKind, WindowsResult};
use crate::action::{ReadinessComponent, WindowsUpdateStatusObservation};

#[cfg(windows)]
fn read_on_com_thread() -> WindowsUpdateStatusObservation {
    use windows::Win32::System::{
        Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_MULTITHREADED,
        },
        UpdateAgent::{
            AutomaticUpdates, IAutomaticUpdates2, ISystemInformation, SystemInformation,
        },
    };

    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if initialized.is_err() {
        return WindowsUpdateStatusObservation {
            last_checked_local: ReadinessComponent::Unknown {
                reason_code: format!("COM_INIT_{:08X}", initialized.0 as u32),
            },
            restart_pending: ReadinessComponent::Unknown {
                reason_code: format!("COM_INIT_{:08X}", initialized.0 as u32),
            },
        };
    }
    struct Uninit;
    impl Drop for Uninit {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    let _uninit = Uninit;

    let last_checked_local = unsafe {
        CoCreateInstance::<_, IAutomaticUpdates2>(&AutomaticUpdates, None, CLSCTX_INPROC_SERVER)
    }
    .and_then(|updates| unsafe { updates.Results() })
    .and_then(|results| unsafe { results.LastSearchSuccessDate() })
    .and_then(|variant| f64::try_from(&variant))
    .ok()
    .and_then(automation_date_to_local_text)
    .map(|value| ReadinessComponent::Known { value })
    .unwrap_or_else(|| ReadinessComponent::Unknown {
        reason_code: "WUA_LAST_SEARCH_UNAVAILABLE".to_owned(),
    });

    let restart_pending = unsafe {
        CoCreateInstance::<_, ISystemInformation>(&SystemInformation, None, CLSCTX_INPROC_SERVER)
    }
    .and_then(|information| unsafe { information.RebootRequired() })
    .map(|value| ReadinessComponent::Known {
        value: value.as_bool(),
    })
    .unwrap_or_else(|error| ReadinessComponent::Unknown {
        reason_code: format!("WUA_REBOOT_{:08X}", error.code().0 as u32),
    });

    WindowsUpdateStatusObservation {
        last_checked_local,
        restart_pending,
    }
}

fn automation_date_to_local_text(value: f64) -> Option<String> {
    use chrono::{Duration, NaiveDate};
    if !value.is_finite() {
        return None;
    }
    let base = NaiveDate::from_ymd_opt(1899, 12, 30)?.and_hms_opt(0, 0, 0)?;
    let whole_days = value.trunc() as i64;
    let millis = ((value - value.trunc()) * 86_400_000.0).round() as i64;
    let timestamp = base
        .checked_add_signed(Duration::days(whole_days))?
        .checked_add_signed(Duration::milliseconds(millis))?;
    Some(timestamp.format("%Y-%m-%d %H:%M:%S").to_string())
}

#[cfg(windows)]
pub fn read_windows_update_status() -> WindowsResult<WindowsUpdateStatusObservation> {
    std::thread::Builder::new()
        .name("totonoe-wua-read".to_owned())
        .spawn(read_on_com_thread)
        .map_err(|error| WindowsError::io("spawn Windows Update read thread", &error))?
        .join()
        .map_err(|_| {
            WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "join Windows Update read thread",
                None,
            )
        })
}

#[cfg(not(windows))]
pub fn read_windows_update_status() -> WindowsResult<WindowsUpdateStatusObservation> {
    Err(WindowsError::unsupported(
        "read Windows Update Agent status",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_date_epoch_is_stable() {
        assert_eq!(
            automation_date_to_local_text(0.0).as_deref(),
            Some("1899-12-30 00:00:00")
        );
        assert_eq!(
            automation_date_to_local_text(2.5).as_deref(),
            Some("1900-01-01 12:00:00")
        );
        assert!(automation_date_to_local_text(f64::NAN).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn real_wua_status_is_read_only_and_each_field_is_explicit() {
        let observed = read_windows_update_status().expect("read Windows Update status");
        println!("{observed:#?}");
        assert!(!matches!(
            observed.last_checked_local,
            ReadinessComponent::Unconfigured
        ));
        assert!(!matches!(
            observed.restart_pending,
            ReadinessComponent::Unconfigured
        ));
    }
}
