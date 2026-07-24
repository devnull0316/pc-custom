use std::collections::BTreeSet;

use super::{WindowsError, WindowsErrorKind, WindowsResult};

#[cfg(windows)]
#[allow(non_snake_case)]
#[derive(serde::Deserialize)]
struct WmiProcessRow {
    ProcessId: Option<u32>,
}

#[cfg(windows)]
pub fn wmi_process_ids() -> WindowsResult<BTreeSet<u32>> {
    std::thread::Builder::new()
        .name("totonoe-wmi-process-snapshot".to_owned())
        .spawn(query_wmi_process_ids)
        .map_err(|error| WindowsError::io("start WMI process observer", &error))?
        .join()
        .map_err(|_| {
            WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "join WMI process observer",
                None,
            )
        })?
}

#[cfg(windows)]
fn query_wmi_process_ids() -> WindowsResult<BTreeSet<u32>> {
    use wmi::{COMLibrary, WMIConnection};

    let com = COMLibrary::new().map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "initialize WMI process observer",
            None,
        )
    })?;
    let connection = WMIConnection::new(com.into()).map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            "connect WMI process observer",
            None,
        )
    })?;
    let rows: Vec<WmiProcessRow> = connection
        .raw_query("SELECT ProcessId FROM Win32_Process")
        .map_err(|_| {
            WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "query WMI process snapshot",
                None,
            )
        })?;
    Ok(rows.into_iter().filter_map(|row| row.ProcessId).collect())
}

#[cfg(not(windows))]
pub fn wmi_process_ids() -> WindowsResult<BTreeSet<u32>> {
    Err(WindowsError::unsupported("query WMI process snapshot"))
}
