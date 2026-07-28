//! タスクバーの自動的に隠す設定の読み書き。
//!
//! 使うのは `SHAppBarMessage` の `ABM_GETSTATE` / `ABM_SETSTATE` だけ。
//! Explorer への注入はしない。監視も常駐もここではしない。
//!
//! # 戻り値を成功の証拠にしない
//!
//! **`ABM_SETSTATE` は常に `TRUE` を返す。** 何も起きなくても `TRUE` が返る。
//! だから書いたあとは必ず、
//!
//! 1. `ABM_GETSTATE` でビットを読み直し、
//! 2. `ABM_GETTASKBARPOS` でタスクバーの矩形を読み、
//! 3. `SPI_GETWORKAREA` で作業領域を読む
//!
//! の3つを別々に見る。自動的に隠す設定が効いていれば、作業領域は画面いっぱいに広がる。
//! ビットだけ変わって作業領域が変わらないなら、それは「反映されていない」。
//!
//! # 所有するのは1ビットだけ
//!
//! `ABS_ALWAYSONTOP` は Windows 7 以降 `ABM_GETSTATE` が返さない。
//! 保存できないものを復元できるとは言わない。触るのは `ABS_AUTOHIDE` だけ。

use super::{WindowsError, WindowsErrorKind, WindowsResult};

/// 書き込み前後で見比べるための観測。**ビットと見た目の両方を持つ。**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskbarAutoHideObservation {
    /// `ABM_GETSTATE` が返した `ABS_AUTOHIDE`。
    pub auto_hide_bit: bool,
    /// タスクバーの矩形 (left, top, right, bottom)。
    pub taskbar_rect: (i32, i32, i32, i32),
    /// 主画面の作業領域。自動的に隠す設定が効いていれば画面いっぱいになる。
    pub work_area: (i32, i32, i32, i32),
    /// 主画面全体の矩形。
    pub screen: (i32, i32, i32, i32),
}

impl TaskbarAutoHideObservation {
    /// 作業領域が画面いっぱいか。
    ///
    /// 自動的に隠す設定が効いている状態では、タスクバーは作業領域を削らない。
    /// **ビットが立っていることと、これは別の観測。**
    pub const fn work_area_covers_screen(&self) -> bool {
        self.work_area.0 <= self.screen.0
            && self.work_area.1 <= self.screen.1
            && self.work_area.2 >= self.screen.2
            && self.work_area.3 >= self.screen.3
    }
}

#[cfg(windows)]
mod imp {
    use super::{TaskbarAutoHideObservation, WindowsError, WindowsErrorKind, WindowsResult};
    use windows::Win32::{
        Foundation::RECT,
        UI::{
            Shell::{
                SHAppBarMessage, ABM_GETSTATE, ABM_GETTASKBARPOS, ABM_SETSTATE, ABS_AUTOHIDE,
                APPBARDATA,
            },
            WindowsAndMessaging::{
                GetSystemMetrics, SystemParametersInfoW, SM_CXSCREEN, SM_CYSCREEN, SPI_GETWORKAREA,
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
            },
        },
    };

    fn appbar_data() -> APPBARDATA {
        APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            ..Default::default()
        }
    }

    pub fn observe() -> WindowsResult<TaskbarAutoHideObservation> {
        let mut state = appbar_data();
        let flags = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut state) };

        let mut position = appbar_data();
        if unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut position) } == 0 {
            return Err(WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "SHAppBarMessage ABM_GETTASKBARPOS",
                None,
            ));
        }

        let mut work = RECT::default();
        unsafe {
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some((&mut work as *mut RECT).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .map_err(|error| {
            WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "SystemParametersInfoW SPI_GETWORKAREA",
                Some(i64::from(error.code().0)),
            )
        })?;

        let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };

        Ok(TaskbarAutoHideObservation {
            auto_hide_bit: (flags as u32 & ABS_AUTOHIDE) != 0,
            taskbar_rect: (
                position.rc.left,
                position.rc.top,
                position.rc.right,
                position.rc.bottom,
            ),
            work_area: (work.left, work.top, work.right, work.bottom),
            screen: (0, 0, width, height),
        })
    }

    /// `ABS_AUTOHIDE` だけを立て下げする。
    ///
    /// 他のビットは `ABM_GETSTATE` から読み直したものをそのまま載せ替える。
    /// 返り値は見ない。**常に `TRUE` なので何の情報も無い。**
    pub fn set_auto_hide(enabled: bool) -> WindowsResult<()> {
        let mut state = appbar_data();
        let current = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut state) } as u32;
        let next = if enabled {
            current | ABS_AUTOHIDE
        } else {
            current & !ABS_AUTOHIDE
        };
        let mut data = appbar_data();
        data.lParam = windows::Win32::Foundation::LPARAM(next as isize);
        unsafe { SHAppBarMessage(ABM_SETSTATE, &mut data) };
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{TaskbarAutoHideObservation, WindowsError, WindowsResult};

    pub fn observe() -> WindowsResult<TaskbarAutoHideObservation> {
        Err(WindowsError::unsupported("observe taskbar auto-hide"))
    }

    pub fn set_auto_hide(_enabled: bool) -> WindowsResult<()> {
        Err(WindowsError::unsupported("set taskbar auto-hide"))
    }
}

pub fn observe_taskbar_auto_hide() -> WindowsResult<TaskbarAutoHideObservation> {
    imp::observe()
}

/// 現在のビットが `expected` と一致するときだけ書き、書いたあと読み直す。
///
/// 書けたかどうかは戻り値では分からないので、**読み直した結果を返す。**
/// 呼んだ側は、返ってきた観測を見て自分で判断する。
pub fn replace_taskbar_auto_hide(
    expected: bool,
    enabled: bool,
) -> WindowsResult<TaskbarAutoHideObservation> {
    let before = imp::observe()?;
    if before.auto_hide_bit != expected {
        return Err(WindowsError::new(
            WindowsErrorKind::ExternalConflict,
            "taskbar auto-hide changed by something else",
            None,
        ));
    }
    imp::set_auto_hide(enabled)?;
    imp::observe()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(
        auto_hide_bit: bool,
        work_area: (i32, i32, i32, i32),
        screen: (i32, i32, i32, i32),
    ) -> TaskbarAutoHideObservation {
        TaskbarAutoHideObservation {
            auto_hide_bit,
            taskbar_rect: (0, 0, 0, 0),
            work_area,
            screen,
        }
    }

    #[test]
    fn a_work_area_short_of_the_screen_is_not_full_screen() {
        // タスクバーが作業領域を削っている状態。
        let shrunk = observation(true, (0, 0, 2560, 1392), (0, 0, 2560, 1440));
        assert!(!shrunk.work_area_covers_screen());
    }

    #[test]
    fn a_work_area_matching_the_screen_is_full_screen() {
        let full = observation(true, (0, 0, 2560, 1440), (0, 0, 2560, 1440));
        assert!(full.work_area_covers_screen());
    }

    /// 実機で1往復させる。
    ///
    /// **`ABM_SETSTATE` の戻り値は使わない。** 常に TRUE なので証拠にならない。
    /// ビットと作業領域を別々に読み直して、両方が動いたかを見る。
    struct RestoreAutoHide(bool);

    impl Drop for RestoreAutoHide {
        fn drop(&mut self) {
            let _ = imp::set_auto_hide(self.0);
        }
    }

    #[test]
    #[ignore = "実機のタスクバーを一時的に変える"]
    fn setting_auto_hide_moves_both_the_bit_and_the_work_area() {
        let before = observe_taskbar_auto_hide().expect("observe taskbar");
        println!("EVIDENCE: taskbar_autohide before={before:?}");
        let _restore = RestoreAutoHide(before.auto_hide_bit);

        let target = !before.auto_hide_bit;
        let after =
            replace_taskbar_auto_hide(before.auto_hide_bit, target).expect("set taskbar auto-hide");
        // Windows がシェルへ通知して作業領域が変わるまで少し待つ。
        std::thread::sleep(std::time::Duration::from_millis(600));
        let settled = observe_taskbar_auto_hide().expect("re-observe taskbar");
        println!("EVIDENCE: taskbar_autohide wrote={target} immediately={after:?}");
        println!("EVIDENCE: taskbar_autohide settled={settled:?}");
        println!(
            "EVIDENCE: taskbar_autohide work_area_full before={} after={}",
            before.work_area_covers_screen(),
            settled.work_area_covers_screen()
        );
        assert_eq!(settled.auto_hide_bit, target, "ビットが動くこと");

        let restored = replace_taskbar_auto_hide(target, before.auto_hide_bit)
            .expect("restore taskbar auto-hide");
        std::thread::sleep(std::time::Duration::from_millis(600));
        let settled_back = observe_taskbar_auto_hide().expect("re-observe after restore");
        println!("EVIDENCE: taskbar_autohide restored={restored:?} settled={settled_back:?}");
        assert_eq!(settled_back.auto_hide_bit, before.auto_hide_bit);
        assert_eq!(
            settled_back.work_area, before.work_area,
            "作業領域も元へ戻ること"
        );
    }

    #[test]
    #[ignore = "実機のタスクバーを一時的に変える"]
    fn a_third_party_change_stops_the_write() {
        let before = observe_taskbar_auto_hide().expect("observe taskbar");
        let _restore = RestoreAutoHide(before.auto_hide_bit);
        // 別のものが変えた状況を作る。
        imp::set_auto_hide(!before.auto_hide_bit).expect("simulate a third party write");
        std::thread::sleep(std::time::Duration::from_millis(300));

        let outcome = replace_taskbar_auto_hide(before.auto_hide_bit, before.auto_hide_bit);
        println!("EVIDENCE: taskbar_autohide conflict outcome={outcome:?}");
        let error = outcome.expect_err("第三者の変更を上書きしない");
        assert_eq!(error.kind, WindowsErrorKind::ExternalConflict);
        let unchanged = observe_taskbar_auto_hide().expect("re-observe after refusal");
        assert_eq!(
            unchanged.auto_hide_bit, !before.auto_hide_bit,
            "断ったなら何も書いていないこと"
        );
    }
}
