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
    type SetMode = unsafe extern "system" fn(*const GUID) -> i32;

    struct Setters {
        ac: Option<SetMode>,
        dc: Option<SetMode>,
    }

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

    pub(super) fn register_callback_context<T, R, E>(
        context: Box<T>,
        register: impl FnOnce(*const T) -> Result<R, E>,
    ) -> Result<(R, Box<T>), E> {
        match register(&*context) {
            Ok(registration) => Ok((registration, context)),
            Err(error) => {
                // A racing callback may still arrive even though registration failed.
                // There is no handle to unregister, so freeing this context would be a UAF.
                let _ = Box::leak(context);
                Err(error)
            }
        }
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
        let mut registration: *mut c_void = std::ptr::null_mut();
        let ((), boxed) = register_callback_context(boxed, |context| {
            unsafe {
                PowerRegisterForEffectivePowerModeNotifications(
                    EFFECTIVE_POWER_MODE_V2,
                    Some(on_mode),
                    Some(context.cast()),
                    &mut registration,
                )
            }
            .map_err(|error| {
                WindowsError::new(
                    WindowsErrorKind::ApiFailure,
                    "PowerRegisterForEffectivePowerModeNotifications",
                    Some(i64::from(error.code().0)),
                )
            })
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

    fn setters() -> &'static Setters {
        static CACHE: OnceLock<Setters> = OnceLock::new();
        CACHE.get_or_init(|| {
            let Ok(module) =
                (unsafe { LoadLibraryExW(w!("powrprof.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) })
            else {
                return Setters { ac: None, dc: None };
            };
            let ac = unsafe { GetProcAddress(module, s!("PowerSetUserConfiguredACPowerMode")) };
            let dc = unsafe { GetProcAddress(module, s!("PowerSetUserConfiguredDCPowerMode")) };
            Setters {
                // SAFETY: 文書化された署名 HRESULT(const GUID*)。
                ac: ac.map(|address| unsafe { std::mem::transmute::<_, SetMode>(address) }),
                dc: dc.map(|address| unsafe { std::mem::transmute::<_, SetMode>(address) }),
            }
        })
    }

    fn write(function: Option<SetMode>, raw: [u8; 16], label: &'static str) -> WindowsResult<()> {
        let Some(function) = function else {
            return Err(WindowsError::new(
                WindowsErrorKind::UnsupportedPlatform,
                label,
                None,
            ));
        };
        let guid = guid_from_bytes(raw);
        let status = unsafe { function(&guid) };
        if status != 0 {
            return Err(WindowsError::new(
                WindowsErrorKind::ApiFailure,
                label,
                Some(i64::from(status)),
            ));
        }
        Ok(())
    }

    fn guid_from_bytes(raw: [u8; 16]) -> GUID {
        GUID {
            data1: u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
            data2: u16::from_le_bytes([raw[4], raw[5]]),
            data3: u16::from_le_bytes([raw[6], raw[7]]),
            data4: [
                raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
            ],
        }
    }

    pub fn write_ac_mode(raw: [u8; 16]) -> WindowsResult<()> {
        write(setters().ac, raw, "PowerSetUserConfiguredACPowerMode")
    }

    pub fn write_dc_mode(raw: [u8; 16]) -> WindowsResult<()> {
        write(setters().dc, raw, "PowerSetUserConfiguredDCPowerMode")
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

    pub fn write_ac_mode(_raw: [u8; 16]) -> WindowsResult<()> {
        Err(super::WindowsError::new(
            super::WindowsErrorKind::UnsupportedPlatform,
            "PowerSetUserConfiguredACPowerMode",
            None,
        ))
    }

    pub fn write_dc_mode(_raw: [u8; 16]) -> WindowsResult<()> {
        Err(super::WindowsError::new(
            super::WindowsErrorKind::UnsupportedPlatform,
            "PowerSetUserConfiguredDCPowerMode",
            None,
        ))
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

impl PowerMode {
    /// 書き込みに使う GUID のバイト列。
    pub const fn bytes(self) -> [u8; 16] {
        match self {
            Self::BestEfficiency => EFFICIENCY,
            Self::Balanced => BALANCED,
            Self::BestPerformance => PERFORMANCE,
        }
    }
}

/// 電源接続時のモードを書く。
pub fn write_ac_mode(mode: PowerMode) -> WindowsResult<()> {
    imp::write_ac_mode(mode.bytes())
}

/// 電池使用時のモードを書く。
pub fn write_dc_mode(mode: PowerMode) -> WindowsResult<()> {
    imp::write_dc_mode(mode.bytes())
}

/// 元の値へ戻すときは、3値でなかった可能性があるのでバイト列のまま書く。
pub fn write_ac_mode_raw(raw: [u8; 16]) -> WindowsResult<()> {
    imp::write_ac_mode(raw)
}

pub fn write_dc_mode_raw(raw: [u8; 16]) -> WindowsResult<()> {
    imp::write_dc_mode(raw)
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

    #[cfg(windows)]
    #[test]
    fn failed_effective_mode_registration_does_not_free_callback_context() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };

        struct DropMarker(Arc<AtomicBool>);
        impl Drop for DropMarker {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let dropped = Arc::new(AtomicBool::new(false));
        let result: Result<((), Box<DropMarker>), ()> =
            imp::register_callback_context(Box::new(DropMarker(dropped.clone())), |_context| {
                Err(())
            });
        assert!(result.is_err());
        assert!(
            !dropped.load(Ordering::SeqCst),
            "Windows may still invoke a callback after registration reports failure"
        );
    }

    /// 実機で1往復させる。
    ///
    /// 書けたことを効果の証明にしない。**書いたあと必ず読み直す。**
    /// 触るのは利用者本人の設定なので、panic しても戻るよう `Drop` で戻す。
    struct RestoreOnDrop {
        ac: Option<[u8; 16]>,
        dc: Option<[u8; 16]>,
    }

    impl Drop for RestoreOnDrop {
        fn drop(&mut self) {
            if let Some(raw) = self.ac {
                let _ = write_ac_mode_raw(raw);
            }
            if let Some(raw) = self.dc {
                let _ = write_dc_mode_raw(raw);
            }
        }
    }

    fn raw_of(reading: PowerModeReading) -> Option<[u8; 16]> {
        match reading {
            PowerModeReading::Known(mode) => Some(mode.bytes()),
            PowerModeReading::Unrecognised(raw) => Some(raw),
            PowerModeReading::Unavailable => None,
        }
    }

    #[test]
    #[ignore = "実機の電源モードを一時的に変える"]
    fn setting_a_mode_and_putting_it_back_is_observable_both_times() {
        let before_ac = read_ac_mode().expect("read ac");
        let before_dc = read_dc_mode().expect("read dc");
        println!("EVIDENCE: power_mode_roundtrip before ac={before_ac:?} dc={before_dc:?}");
        let (Some(original_ac), Some(original_dc)) = (raw_of(before_ac), raw_of(before_dc)) else {
            println!("EVIDENCE: power_mode_roundtrip skipped (この環境では読めない)");
            return;
        };
        let _restore = RestoreOnDrop {
            ac: Some(original_ac),
            dc: Some(original_dc),
        };

        // 供給ごとに、今と違う値を書く。
        // 同じ値を書いて同じ値が返っても、書けた証明にはならない。
        //
        // すべての機が3値を受け付けるわけではない。この機は「電池優先」を
        // ERROR_INVALID_PARAMETER で拒否する。受け付ける値が見つかるまで順に試し、
        // どれで測ったかを EVIDENCE に残す。
        let write_something_else = |current: PowerModeReading,
                                    write: &dyn Fn(PowerMode) -> WindowsResult<()>|
         -> Option<PowerMode> {
            [
                PowerMode::Balanced,
                PowerMode::BestPerformance,
                PowerMode::BestEfficiency,
            ]
            .into_iter()
            .filter(|mode| current != PowerModeReading::Known(*mode))
            .find(|mode| write(*mode).is_ok())
        };
        let target_ac =
            write_something_else(before_ac, &write_ac_mode).expect("AC でどれか1つは書けるはず");
        let target_dc =
            write_something_else(before_dc, &write_dc_mode).expect("DC でどれか1つは書けるはず");
        let after_ac = read_ac_mode().expect("re-read ac");
        let after_dc = read_dc_mode().expect("re-read dc");
        println!(
            "EVIDENCE: power_mode_roundtrip wrote ac={target_ac:?} dc={target_dc:?}              after ac={after_ac:?} dc={after_dc:?}"
        );
        assert_eq!(after_ac, PowerModeReading::Known(target_ac));
        assert_eq!(after_dc, PowerModeReading::Known(target_dc));

        write_ac_mode_raw(original_ac).expect("restore ac");
        write_dc_mode_raw(original_dc).expect("restore dc");
        let restored_ac = read_ac_mode().expect("re-read ac after restore");
        let restored_dc = read_dc_mode().expect("re-read dc after restore");
        println!("EVIDENCE: power_mode_roundtrip restored ac={restored_ac:?} dc={restored_dc:?}");
        assert_eq!(restored_ac, before_ac);
        assert_eq!(restored_dc, before_dc);
    }

    /// DC 側が拒否する条件を切り分ける。3値それぞれを試して、結果をそのまま出す。
    #[test]
    #[ignore = "実機の電源モードを一時的に変える"]
    fn which_dc_values_does_this_machine_accept() {
        let before = read_dc_mode().expect("read dc");
        println!("EVIDENCE: dc_probe before={before:?}");
        let original = match before {
            PowerModeReading::Known(mode) => mode.bytes(),
            PowerModeReading::Unrecognised(raw) => raw,
            PowerModeReading::Unavailable => {
                println!("EVIDENCE: dc_probe skipped");
                return;
            }
        };
        let _restore = RestoreOnDrop {
            ac: None,
            dc: Some(original),
        };
        for mode in [
            PowerMode::Balanced,
            PowerMode::BestEfficiency,
            PowerMode::BestPerformance,
        ] {
            let write = write_dc_mode(mode);
            let read_back = read_dc_mode();
            println!("EVIDENCE: dc_probe {mode:?} write={write:?} read_back={read_back:?}");
        }
        // AC 側も同じ3値で試して、DC だけの問題かを見る。
        let before_ac = read_ac_mode().expect("read ac");
        let original_ac = match before_ac {
            PowerModeReading::Known(mode) => mode.bytes(),
            PowerModeReading::Unrecognised(raw) => raw,
            PowerModeReading::Unavailable => return,
        };
        let _restore_ac = RestoreOnDrop {
            ac: Some(original_ac),
            dc: None,
        };
        for mode in [
            PowerMode::Balanced,
            PowerMode::BestEfficiency,
            PowerMode::BestPerformance,
        ] {
            let write = write_ac_mode(mode);
            let read_back = read_ac_mode();
            println!("EVIDENCE: ac_probe {mode:?} write={write:?} read_back={read_back:?}");
        }
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
