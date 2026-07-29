//! マウスポインターの動き方（速度と加速）の読み書き。
//!
//! `SystemParametersInfoW` の `SPI_GET/SETMOUSE` と `SPI_GET/SETMOUSESPEED` だけを使う。
//! どちらも文書化済みで、管理者権限も再起動も要らない。
//!
//! # 効かない相手がいる
//!
//! Microsoft は、Raw Input で受け取るマウス入力は
//! コントロールパネルのマウス速度の影響を受けないと明記している。
//! つまり **Raw Input を使うゲームには、ここの設定は届かない。**
//! あるゲームが Raw Input を使っているかどうかは外からは分からないので、
//! 「効きます」とも「効きません」とも断定しない。分からないものは分からないと書く。

use super::{WindowsError, WindowsErrorKind, WindowsResult};

/// ポインターの動き方を決める4つの値。
///
/// 3つの整数は `SPI_SETMOUSE` がそのまま受け取る形。意味を解釈せずに保存する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointerFeel {
    /// 加速が効き始める1つ目のしきい値。
    pub threshold_one: i32,
    /// 2つ目のしきい値。
    pub threshold_two: i32,
    /// 0 なら加速なし。0 以外で加速あり。
    pub acceleration: i32,
    /// ポインターの速さ。Windows が扱うのは 1〜20。
    pub speed: i32,
}

impl PointerFeel {
    pub const fn acceleration_enabled(self) -> bool {
        self.acceleration != 0
    }

    /// 加速を切った状態。**しきい値は触らない。**
    ///
    /// 戻すときに元のしきい値をそのまま書き戻せるよう、変えるのは加速だけにする。
    pub const fn without_acceleration(self) -> Self {
        Self {
            acceleration: 0,
            ..self
        }
    }

    /// 加速を入れた状態。
    ///
    /// しきい値が両方 0 だと、加速を 1 にしても動き方が決まらない。
    /// そのときだけ Windows の既定値 6 と 10 を入れる。**入れたことは画面に書く。**
    pub const fn with_acceleration(self) -> Self {
        if self.threshold_one == 0 && self.threshold_two == 0 {
            Self {
                threshold_one: 6,
                threshold_two: 10,
                acceleration: 1,
                speed: self.speed,
            }
        } else {
            Self {
                acceleration: 1,
                ..self
            }
        }
    }

    /// 元の値からしきい値を補ったかどうか。説明文で使う。
    pub const fn would_fill_in_thresholds(self) -> bool {
        self.threshold_one == 0 && self.threshold_two == 0
    }

    pub fn fingerprint(self) -> crate::backup::Fingerprint {
        let mut bytes = Vec::with_capacity(16);
        for value in [
            self.threshold_one,
            self.threshold_two,
            self.acceleration,
            self.speed,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        crate::backup::Fingerprint::of_bytes(&bytes)
    }
}

fn write_transaction(
    before: PointerFeel,
    target: PointerFeel,
    mut read: impl FnMut() -> WindowsResult<PointerFeel>,
    mut write_triple: impl FnMut(PointerFeel) -> WindowsResult<()>,
    mut write_speed: impl FnMut(i32) -> WindowsResult<()>,
) -> WindowsResult<()> {
    write_triple(target)?;
    if let Err(error) = write_speed(target.speed) {
        // 失敗を返した API が値を全く変えていないとは仮定しない。
        // 4値をすべて戻し、独立した読み直しで一致した場合だけ元の失敗として返す。
        let _ = write_triple(before);
        let _ = write_speed(before.speed);
        let verified = read().is_ok_and(|current| current == before);
        if !verified {
            return Err(WindowsError::new(
                WindowsErrorKind::RecoveryRequired,
                "pointer settings left half written",
                None,
            ));
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(windows)]
mod imp {
    use super::{PointerFeel, WindowsError, WindowsErrorKind, WindowsResult};
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPI_GETMOUSE, SPI_GETMOUSESPEED, SPI_SETMOUSE,
        SPI_SETMOUSESPEED, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };

    /// 3つ組だけを書く。速さは別の呼び出し。
    fn write_triple(target: PointerFeel) -> WindowsResult<()> {
        let triple = [
            target.threshold_one,
            target.threshold_two,
            target.acceleration,
        ];
        unsafe {
            SystemParametersInfoW(
                SPI_SETMOUSE,
                0,
                Some(triple.as_ptr() as *mut core::ffi::c_void),
                SPIF_SENDCHANGE,
            )
        }
        .map_err(|error| api_error("write pointer acceleration settings", error))
    }

    fn api_error(operation: &'static str, error: windows::core::Error) -> WindowsError {
        WindowsError::new(
            WindowsErrorKind::ApiFailure,
            operation,
            Some(i64::from(error.code().0)),
        )
    }

    pub fn read() -> WindowsResult<PointerFeel> {
        let mut triple = [0i32; 3];
        unsafe {
            SystemParametersInfoW(
                SPI_GETMOUSE,
                0,
                Some(triple.as_mut_ptr().cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .map_err(|error| api_error("read pointer acceleration settings", error))?;

        let mut speed = 0i32;
        unsafe {
            SystemParametersInfoW(
                SPI_GETMOUSESPEED,
                0,
                Some((&mut speed as *mut i32).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }
        .map_err(|error| api_error("read pointer speed", error))?;

        Ok(PointerFeel {
            threshold_one: triple[0],
            threshold_two: triple[1],
            acceleration: triple[2],
            speed,
        })
    }

    fn write_speed(speed: i32) -> WindowsResult<()> {
        unsafe {
            SystemParametersInfoW(
                SPI_SETMOUSESPEED,
                0,
                Some(speed as usize as *mut core::ffi::c_void),
                SPIF_SENDCHANGE,
            )
        }
        .map_err(|error| api_error("write pointer speed", error))
    }

    /// 4つの値をまとめて書く。
    ///
    /// `SPI_SETMOUSESPEED` は値をポインターとしてではなく `pvParam` に直接載せる。
    /// 3つ組のほうは配列のアドレスを渡す。同じ関数でも渡し方が違うので分けて書く。
    pub fn write(target: PointerFeel) -> WindowsResult<()> {
        // 巻き戻し先を先に読む。書いてから読んでも、それはもう変わったあとの値。
        let before = read()?;
        super::write_transaction(before, target, read, write_triple, write_speed)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{PointerFeel, WindowsError, WindowsResult};

    pub fn read() -> WindowsResult<PointerFeel> {
        Err(WindowsError::unsupported("read pointer settings"))
    }

    pub fn write(_target: PointerFeel) -> WindowsResult<()> {
        Err(WindowsError::unsupported("write pointer settings"))
    }
}

pub fn read_pointer_feel() -> WindowsResult<PointerFeel> {
    imp::read()
}

/// 現在値が `expected` と一致するときだけ書く。
///
/// 別のマウスユーティリティが同じ設定を触っていたら、上書きせずに止める。
pub fn replace_pointer_feel(expected: PointerFeel, target: PointerFeel) -> WindowsResult<()> {
    let current = imp::read()?;
    if current != expected {
        return Err(WindowsError::new(
            WindowsErrorKind::ExternalConflict,
            "pointer settings changed by something else",
            None,
        ));
    }
    imp::write(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    fn feel(threshold_one: i32, threshold_two: i32, acceleration: i32, speed: i32) -> PointerFeel {
        PointerFeel {
            threshold_one,
            threshold_two,
            acceleration,
            speed,
        }
    }

    fn failure(operation: &'static str) -> WindowsError {
        WindowsError::new(WindowsErrorKind::ApiFailure, operation, None)
    }

    #[test]
    fn turning_acceleration_off_leaves_the_thresholds_alone() {
        let original = feel(6, 10, 1, 12);
        let off = original.without_acceleration();
        assert_eq!(off, feel(6, 10, 0, 12));
        // 速度も触らない。加速だけの話にする。
        assert_eq!(off.speed, original.speed);
    }

    #[test]
    fn second_write_failure_restores_all_four_values_and_reads_them_back() {
        let before = feel(6, 10, 1, 12);
        let target = feel(6, 10, 0, 18);
        let current = Rc::new(RefCell::new(before));
        let speed_writes = Rc::new(RefCell::new(0_u8));

        let read_state = Rc::clone(&current);
        let triple_state = Rc::clone(&current);
        let speed_state = Rc::clone(&current);
        let speed_calls = Rc::clone(&speed_writes);
        let error = write_transaction(
            before,
            target,
            move || Ok(*read_state.borrow()),
            move |value| {
                let mut state = triple_state.borrow_mut();
                state.threshold_one = value.threshold_one;
                state.threshold_two = value.threshold_two;
                state.acceleration = value.acceleration;
                Ok(())
            },
            move |speed| {
                let mut calls = speed_calls.borrow_mut();
                *calls += 1;
                speed_state.borrow_mut().speed = speed;
                if *calls == 1 {
                    Err(failure("simulated second write"))
                } else {
                    Ok(())
                }
            },
        )
        .expect_err("the original write still failed");

        assert_eq!(error.kind, WindowsErrorKind::ApiFailure);
        assert_eq!(*current.borrow(), before);
        assert_eq!(*speed_writes.borrow(), 2, "speed itself was also restored");
    }

    #[test]
    fn unverifiable_compensation_is_reported_as_recovery_required() {
        let before = feel(6, 10, 1, 12);
        let target = feel(6, 10, 0, 18);
        let current = Rc::new(RefCell::new(before));
        let speed_writes = Rc::new(RefCell::new(0_u8));

        let read_state = Rc::clone(&current);
        let triple_state = Rc::clone(&current);
        let speed_state = Rc::clone(&current);
        let speed_calls = Rc::clone(&speed_writes);
        let error = write_transaction(
            before,
            target,
            move || Ok(*read_state.borrow()),
            move |value| {
                let mut state = triple_state.borrow_mut();
                state.threshold_one = value.threshold_one;
                state.threshold_two = value.threshold_two;
                state.acceleration = value.acceleration;
                Ok(())
            },
            move |speed| {
                let mut calls = speed_calls.borrow_mut();
                *calls += 1;
                if *calls == 1 {
                    speed_state.borrow_mut().speed = speed;
                }
                Err(failure("simulated speed write"))
            },
        )
        .expect_err("compensation cannot be proved");

        assert_eq!(error.kind, WindowsErrorKind::RecoveryRequired);
    }

    #[test]
    fn turning_acceleration_on_keeps_existing_thresholds() {
        let original = feel(3, 7, 0, 5);
        assert_eq!(original.with_acceleration(), feel(3, 7, 1, 5));
        assert!(!original.would_fill_in_thresholds());
    }

    #[test]
    fn turning_acceleration_on_from_zero_thresholds_fills_in_the_documented_defaults() {
        let original = feel(0, 0, 0, 5);
        assert!(original.would_fill_in_thresholds());
        assert_eq!(original.with_acceleration(), feel(6, 10, 1, 5));
    }

    #[test]
    fn the_fingerprint_changes_when_any_of_the_four_values_change() {
        let base = feel(6, 10, 1, 12);
        for changed in [
            feel(7, 10, 1, 12),
            feel(6, 11, 1, 12),
            feel(6, 10, 0, 12),
            feel(6, 10, 1, 13),
        ] {
            assert_ne!(base.fingerprint(), changed.fingerprint());
        }
    }

    /// 実機のマウス設定は1つしかない共有物。
    ///
    /// テストは既定で並列に走るので、2つが同時に触ると互いの変更を
    /// 「第三者の変更」として観測してしまう。実際にそれで1回落とした。
    static REAL_POINTER: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 実機で1往復させる。書けたことを効果の証明にしない。
    struct RestoreOnDrop(PointerFeel);

    impl Drop for RestoreOnDrop {
        fn drop(&mut self) {
            let _ = imp::write(self.0);
        }
    }

    #[test]
    #[ignore = "実機のマウス設定を一時的に変える"]
    fn writing_and_restoring_pointer_settings_is_observable_each_time() {
        let _serial = REAL_POINTER
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let before = read_pointer_feel().expect("read pointer settings");
        println!("EVIDENCE: pointer_feel before={before:?}");
        let _restore = RestoreOnDrop(before);

        // 今と違う状態を作る。同じ値を書いて同じ値が返っても何も言えない。
        let target = if before.acceleration_enabled() {
            before.without_acceleration()
        } else {
            before.with_acceleration()
        };
        // 速度も1つ動かして、3つ組と速度が別経路で書かれることまで確かめる。
        let target = PointerFeel {
            speed: if before.speed >= 20 {
                before.speed - 1
            } else {
                before.speed + 1
            },
            ..target
        };
        replace_pointer_feel(before, target).expect("write pointer settings");

        let after = read_pointer_feel().expect("re-read pointer settings");
        println!("EVIDENCE: pointer_feel wrote={target:?} after={after:?}");
        assert_eq!(after, target);

        replace_pointer_feel(target, before).expect("restore pointer settings");
        let restored = read_pointer_feel().expect("re-read after restore");
        println!("EVIDENCE: pointer_feel restored={restored:?}");
        assert_eq!(restored, before, "4つの値すべてを元へ戻す");
    }

    #[test]
    #[ignore = "実機のマウス設定を一時的に変える"]
    fn a_third_party_change_stops_the_write_instead_of_overwriting_it() {
        let _serial = REAL_POINTER
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let before = read_pointer_feel().expect("read pointer settings");
        let _restore = RestoreOnDrop(before);
        // 別のものが変えた状況を作る。
        let meddled = PointerFeel {
            threshold_one: before.threshold_one + 1,
            ..before
        };
        imp::write(meddled).expect("simulate a third party write");

        // 変更前の値を期待して書こうとすると、上書きせずに止まるはず。
        let outcome = replace_pointer_feel(before, before.without_acceleration());
        println!("EVIDENCE: pointer_feel conflict outcome={outcome:?}");
        let error = outcome.expect_err("第三者の変更を上書きしない");
        assert_eq!(error.kind, WindowsErrorKind::ExternalConflict);
        let unchanged = read_pointer_feel().expect("re-read after refusal");
        assert_eq!(unchanged, meddled, "断ったなら何も書いていないこと");
    }
}
