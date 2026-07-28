//! Windows 11 の電源モード（電池優先／バランス／パフォーマンス優先）の読み取り。
//!
//! 旧来の「電源プラン」とは別物。プランは `power.rs` が扱う。
//! ここが読むのは、AC 接続時と電池使用時それぞれについて**利用者が選んだモード**。
//!
//! # なぜ動的に解決するのか
//!
//! `PowerGet/SetUserConfiguredACPowerMode` と DC 版は、windows-rs 0.58 の
//! メタデータに入っていない。静的リンクすると、エクスポートが無い Windows では
//! **プロセスごと起動しなくなる。** 起動しないのは最悪の失敗なので、
//! `GetProcAddress` で実行時に引き、無ければ「この環境にはありません」と言って終わる。
//!
//! # 要求値と実効値は別
//!
//! Microsoft はこの値を「他の system signal に上書きされ得る vote」と書いている。
//! つまり**ここで読めるのは要求値であって、いま効いているモードではない。**
//! 実効モードは `PowerRegisterForEffectivePowerModeNotifications` で別に観測する。
//! 混ぜて表示しない。

use super::{WindowsError, WindowsErrorKind, WindowsResult};

/// 利用者が選べる3つ。これ以外の値は受け付けない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    /// 電池を長く保たせる側。
    BestEfficiency,
    /// 既定。
    Balanced,
    /// 性能側。**速くなるとは言わない。**
    BestPerformance,
}

/// Windows の電源オーバーレイ GUID。文書化された3値。
const EFFICIENCY: [u8; 16] = [
    0x08, 0x13, 0x84, 0xa1, 0x41, 0x35, 0xab, 0x4f, 0xbc, 0x81, 0xf7, 0x15, 0x56, 0xf2, 0x0b, 0x4a,
];
const BALANCED: [u8; 16] = [0; 16];
const PERFORMANCE: [u8; 16] = [
    0xb5, 0x74, 0xd5, 0xde, 0xa0, 0x45, 0x42, 0x4f, 0x87, 0x37, 0x46, 0x34, 0x5c, 0x09, 0xc2, 0x38,
];

impl PowerMode {
    fn from_bytes(raw: [u8; 16]) -> Option<Self> {
        match raw {
            EFFICIENCY => Some(Self::BestEfficiency),
            BALANCED => Some(Self::Balanced),
            PERFORMANCE => Some(Self::BestPerformance),
            _ => None,
        }
    }
}

/// Windows が「いま実際に効いている」と報告するモード。
///
/// 利用者が選んだモードとは**別物**。Game Mode や Battery Saver が勝つことがある。
/// 同じ画面に並べるときは、必ず別の見出しで出す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveMode {
    BatterySaver,
    BetterBattery,
    Balanced,
    HighPerformance,
    MaxPerformance,
    GameMode,
    MixedReality,
    /// 知らない値が来た。**丸めない。**
    Unrecognised(i32),
}

/// 片方の電源につき、読めた結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerModeReading {
    /// 3値のどれかとして読めた。
    Known(PowerMode),
    /// 読めたが、知らない値だった。**勝手に既定へ丸めない。**
    Unrecognised([u8; 16]),
    /// この環境にはこの機能が無い。
    Unavailable,
}

#[cfg(windows)]
mod imp {
    use super::{PowerMode, PowerModeReading, WindowsError, WindowsErrorKind, WindowsResult};
    use std::ffi::c_void;
    use std::sync::OnceLock;
    use windows::{
        core::{s, w, GUID},
        Win32::{
            Foundation::{FreeLibrary, HMODULE},
            System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32},
        },
    };

    type GetMode = unsafe extern "system" fn(*mut GUID) -> i32;

    /// powrprof.dll から2つの getter を引く。
    ///
    /// 探索は System32 に固定する。カレントディレクトリの同名 DLL を拾わせない。
    /// モジュールは解放しない（プロセスの寿命で持つ）ため、関数ポインタは有効なまま。
    struct Getters {
        ac: Option<GetMode>,
        dc: Option<GetMode>,
    }

    fn getters() -> &'static Getters {
        static CACHE: OnceLock<Getters> = OnceLock::new();
        CACHE.get_or_init(|| {
            let module: HMODULE = match unsafe {
                LoadLibraryExW(w!("powrprof.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32)
            } {
                Ok(handle) => handle,
                Err(_) => return Getters { ac: None, dc: None },
            };
            let ac = unsafe { GetProcAddress(module, s!("PowerGetUserConfiguredACPowerMode")) };
            let dc = unsafe { GetProcAddress(module, s!("PowerGetUserConfiguredDCPowerMode")) };
            if ac.is_none() && dc.is_none() {
                // 使わないなら参照を残さない。
                let _ = unsafe { FreeLibrary(module) };
                return Getters { ac: None, dc: None };
            }
            Getters {
                // SAFETY: 文書化された署名 HRESULT(GUID*)。取れたときだけ変換する。
                ac: ac.map(|address| unsafe { std::mem::transmute::<_, GetMode>(address) }),
                dc: dc.map(|address| unsafe { std::mem::transmute::<_, GetMode>(address) }),
            }
        })
    }

    fn read(function: Option<GetMode>, label: &'static str) -> WindowsResult<PowerModeReading> {
        let Some(function) = function else {
            return Ok(PowerModeReading::Unavailable);
        };
        let mut guid = GUID::zeroed();
        let status = unsafe { function(&mut guid) };
        if status != 0 {
            return Err(WindowsError::new(
                WindowsErrorKind::ApiFailure,
                label,
                Some(i64::from(status)),
            ));
        }
        let raw = guid_bytes(&guid);
        Ok(match PowerMode::from_bytes(raw) {
            Some(mode) => PowerModeReading::Known(mode),
            None => PowerModeReading::Unrecognised(raw),
        })
    }

    fn guid_bytes(guid: &GUID) -> [u8; 16] {
        let mut raw = [0u8; 16];
        raw[0..4].copy_from_slice(&guid.data1.to_le_bytes());
        raw[4..6].copy_from_slice(&guid.data2.to_le_bytes());
        raw[6..8].copy_from_slice(&guid.data3.to_le_bytes());
        raw[8..16].copy_from_slice(&guid.data4);
        raw
    }

    /// 実効モードを1回だけ読む。
    ///
    /// この API は購読型で、登録した直後に現在値で1回呼ばれる。
    /// 待つのは有限時間だけにして、来なければ `Ok(None)`。
    /// **来ないことを「バランス」と読み替えない。**
    pub fn read_effective_mode() -> WindowsResult<Option<super::EffectiveMode>> {
        use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
        use std::time::Duration;
        use windows::Win32::System::Power::{
            PowerRegisterForEffectivePowerModeNotifications,
            PowerUnregisterFromEffectivePowerModeNotifications, EFFECTIVE_POWER_MODE,
            EFFECTIVE_POWER_MODE_V2,
        };

        unsafe extern "system" fn on_mode(mode: EFFECTIVE_POWER_MODE, context: *const c_void) {
            if context.is_null() {
                return;
            }
            // SAFETY: 登録から解除までの間だけ生きている Sender を指している。
            let sender = unsafe { &*(context as *const Sender<i32>) };
            let _ = sender.send(mode.0);
        }

        let (sender, receiver) = channel::<i32>();
        // 登録が生きている間だけ有効なアドレスを渡す。解除まで動かさない。
        let boxed = Box::new(sender);
        let context: *const c_void = (&*boxed as *const Sender<i32>).cast();
        let mut registration: *mut c_void = std::ptr::null_mut();
        unsafe {
            PowerRegisterForEffectivePowerModeNotifications(
                EFFECTIVE_POWER_MODE_V2,
                Some(on_mode),
                Some(context),
                &mut registration,
            )
        }
        .map_err(|error| {
            WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "PowerRegisterForEffectivePowerModeNotifications",
                Some(i64::from(error.code().0)),
            )
        })?;

        let received = match receiver.recv_timeout(Duration::from_millis(1_500)) {
            Ok(value) => Some(value),
            Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => None,
        };

        // 解除してから Sender を捨てる。順番を逆にするとコールバックが解放後を指す。
        let unregistered =
            unsafe { PowerUnregisterFromEffectivePowerModeNotifications(registration) };
        drop(boxed);
        unregistered.map_err(|error| {
            WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "PowerUnregisterFromEffectivePowerModeNotifications",
                Some(i64::from(error.code().0)),
            )
        })?;

        Ok(received.map(super::EffectiveMode::from_raw))
    }

    pub fn read_ac_mode() -> WindowsResult<PowerModeReading> {
        read(getters().ac, "PowerGetUserConfiguredACPowerMode")
    }

    pub fn read_dc_mode() -> WindowsResult<PowerModeReading> {
        read(getters().dc, "PowerGetUserConfiguredDCPowerMode")
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{PowerModeReading, WindowsResult};

    pub fn read_effective_mode() -> WindowsResult<Option<super::EffectiveMode>> {
        Ok(None)
    }

    pub fn read_ac_mode() -> WindowsResult<PowerModeReading> {
        Ok(PowerModeReading::Unavailable)
    }

    pub fn read_dc_mode() -> WindowsResult<PowerModeReading> {
        Ok(PowerModeReading::Unavailable)
    }
}

/// 電源に接続しているときの、利用者が選んだモード。
pub fn read_ac_mode() -> WindowsResult<PowerModeReading> {
    imp::read_ac_mode()
}

/// 電池で動いているときの、利用者が選んだモード。
pub fn read_dc_mode() -> WindowsResult<PowerModeReading> {
    imp::read_dc_mode()
}

/// Windows が報告する実効モード。取れなければ `None`。
pub fn read_effective_mode() -> WindowsResult<Option<EffectiveMode>> {
    imp::read_effective_mode()
}

impl EffectiveMode {
    fn from_raw(raw: i32) -> Self {
        match raw {
            0 => Self::BatterySaver,
            1 => Self::BetterBattery,
            2 => Self::Balanced,
            3 => Self::HighPerformance,
            4 => Self::MaxPerformance,
            5 => Self::GameMode,
            6 => Self::MixedReality,
            other => Self::Unrecognised(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_documented_guids_map_to_the_three_modes() {
        assert_eq!(PowerMode::from_bytes(BALANCED), Some(PowerMode::Balanced));
        assert_eq!(
            PowerMode::from_bytes(EFFICIENCY),
            Some(PowerMode::BestEfficiency)
        );
        assert_eq!(
            PowerMode::from_bytes(PERFORMANCE),
            Some(PowerMode::BestPerformance)
        );
    }

    #[test]
    fn an_unknown_guid_is_not_rounded_to_balanced() {
        // 知らない値を既定として扱うと、画面に嘘が出る。
        let mut raw = [0u8; 16];
        raw[0] = 0x99;
        assert_eq!(PowerMode::from_bytes(raw), None);
    }

    /// 実機で1回読んで、返ってきた値をそのまま出す。
    ///
    /// **署名を当てているので、これが本題。** 手書きの FFI が正しければ、
    /// 3つの文書化 GUID のどれかが返るはず。壊れていれば読めない値が出る。
    #[test]
    #[ignore = "実機で読む"]
    fn read_the_real_machine_modes() {
        println!("EVIDENCE: power_mode ac={:?}", read_ac_mode());
        println!("EVIDENCE: power_mode dc={:?}", read_dc_mode());
        println!("EVIDENCE: power_mode effective={:?}", read_effective_mode());
    }
}
