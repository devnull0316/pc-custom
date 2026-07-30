//! Windows のコントラストテーマ状態を、文書化された `SystemParametersInfoW` で扱う。
//!
//! `HIGHCONTRASTW` は有効フラグだけではない。正確な復元のため、構造体サイズ、
//! 全フラグ、scheme ポインターの NULL / 空文字 / 文字列を区別して保存する。

use super::{WindowsError, WindowsErrorKind, WindowsResult};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum HighContrastScheme {
    Null,
    Name(String),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HighContrastSnapshot {
    pub structure_size: u32,
    pub flags: u32,
    pub scheme: HighContrastScheme,
}

impl HighContrastSnapshot {
    pub const fn enabled(&self) -> bool {
        #[cfg(windows)]
        {
            self.flags & windows::Win32::UI::Accessibility::HCF_HIGHCONTRASTON.0 != 0
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    pub fn with_enabled(&self) -> Self {
        let mut intended = self.clone();
        #[cfg(windows)]
        {
            intended.flags |= windows::Win32::UI::Accessibility::HCF_HIGHCONTRASTON.0;
        }
        intended
    }

    pub fn fingerprint(&self) -> crate::backup::Fingerprint {
        let bytes = serde_json::to_vec(self)
            .expect("typed high-contrast state serialization is infallible");
        crate::backup::Fingerprint::of_bytes(&bytes)
    }
}

#[cfg(windows)]
mod imp {
    use super::{
        HighContrastScheme, HighContrastSnapshot, WindowsError, WindowsErrorKind, WindowsResult,
    };
    use windows::{
        core::PWSTR,
        Win32::UI::{
            Accessibility::{HIGHCONTRASTW, HIGHCONTRASTW_FLAGS},
            WindowsAndMessaging::{
                SystemParametersInfoW, SPIF_SENDCHANGE, SPI_GETHIGHCONTRAST, SPI_SETHIGHCONTRAST,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
            },
        },
    };

    fn api_error(operation: &'static str, error: windows::core::Error) -> WindowsError {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            operation,
            Some(i64::from(error.code().0)),
        )
    }

    pub fn read() -> WindowsResult<HighContrastSnapshot> {
        let structure_size = std::mem::size_of::<HIGHCONTRASTW>() as u32;
        let mut value = HIGHCONTRASTW {
            cbSize: structure_size,
            ..Default::default()
        };
        unsafe {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                structure_size,
                Some((&mut value as *mut HIGHCONTRASTW).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .map_err(|error| api_error("read high-contrast settings", error))?;
        if value.cbSize != structure_size {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "read high-contrast structure size",
                None,
            ));
        }
        let scheme = if value.lpszDefaultScheme.is_null() {
            HighContrastScheme::Null
        } else {
            HighContrastScheme::Name(unsafe { value.lpszDefaultScheme.to_string() }.map_err(
                |_| {
                    WindowsError::new(
                        WindowsErrorKind::InvalidData,
                        "decode high-contrast scheme name",
                        None,
                    )
                },
            )?)
        };
        Ok(HighContrastSnapshot {
            structure_size,
            flags: value.dwFlags.0,
            scheme,
        })
    }

    fn write(target: &HighContrastSnapshot) -> WindowsResult<()> {
        let expected_size = std::mem::size_of::<HIGHCONTRASTW>() as u32;
        if target.structure_size != expected_size {
            return Err(WindowsError::new(
                WindowsErrorKind::InvalidData,
                "validate high-contrast structure size",
                None,
            ));
        }
        let mut scheme_name = match &target.scheme {
            HighContrastScheme::Null => None,
            HighContrastScheme::Name(name) => Some(
                name.encode_utf16()
                    .chain(std::iter::once(0))
                    .collect::<Vec<_>>(),
            ),
        };
        let mut value = HIGHCONTRASTW {
            cbSize: target.structure_size,
            dwFlags: HIGHCONTRASTW_FLAGS(target.flags),
            lpszDefaultScheme: scheme_name
                .as_mut()
                .map_or(PWSTR::null(), |name| PWSTR(name.as_mut_ptr())),
        };
        unsafe {
            SystemParametersInfoW(
                SPI_SETHIGHCONTRAST,
                target.structure_size,
                Some((&mut value as *mut HIGHCONTRASTW).cast()),
                SPIF_SENDCHANGE,
            )
        }
        .map_err(|error| api_error("write high-contrast settings", error))
    }

    pub fn replace(
        expected: &HighContrastSnapshot,
        target: &HighContrastSnapshot,
    ) -> WindowsResult<()> {
        if read()? != *expected {
            return Err(WindowsError::new(
                WindowsErrorKind::ExternalConflict,
                "high-contrast settings changed by something else",
                None,
            ));
        }
        write(target)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{HighContrastSnapshot, WindowsError, WindowsResult};

    pub fn read() -> WindowsResult<HighContrastSnapshot> {
        Err(WindowsError::unsupported("read high-contrast settings"))
    }

    pub fn replace(
        _expected: &HighContrastSnapshot,
        _target: &HighContrastSnapshot,
    ) -> WindowsResult<()> {
        Err(WindowsError::unsupported("write high-contrast settings"))
    }
}

pub fn read_high_contrast() -> WindowsResult<HighContrastSnapshot> {
    imp::read()
}

/// `expected` が現在値と全フィールド一致するときだけ `target` を書く。
pub fn replace_high_contrast(
    expected: &HighContrastSnapshot,
    target: &HighContrastSnapshot,
) -> WindowsResult<()> {
    imp::replace(expected, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_distinguishes_null_empty_and_named_schemes() {
        let base = HighContrastSnapshot {
            structure_size: 16,
            flags: 126,
            scheme: HighContrastScheme::Null,
        };
        let empty = HighContrastSnapshot {
            scheme: HighContrastScheme::Name(String::new()),
            ..base.clone()
        };
        let named = HighContrastSnapshot {
            scheme: HighContrastScheme::Name("contrast".to_owned()),
            ..base.clone()
        };
        assert_ne!(base.fingerprint(), empty.fingerprint());
        assert_ne!(empty.fingerprint(), named.fingerprint());
        assert_ne!(base.fingerprint(), named.fingerprint());
    }
}
