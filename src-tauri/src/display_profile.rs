//! 表示構成（更新頻度など）の読み取りと、2つの構成の意味的な差分。
//!
//! **ここは適用しない。読むだけ。**
//!
//! # なぜ適用を入れないのか
//!
//! `docs/RESEARCH_FEATURES.md` の「9. 更新頻度を含む安全な表示プロファイル」は、
//! この機能を **実験のみ**と結論している。理由は失敗の形が悪いこと。
//! 誤った構成を当てると画面が真っ黒になり、**利用者は戻す操作すら押せない。**
//!
//! そこで研究の結論どおり、まず「2つの構成が、更新頻度の違いだけかどうかを言えるか」を作る。
//! これが言えないうちは適用の話をしない。言えるようになっても、
//! 停電・シェル再起動・黒画面でも動く復帰の仕掛けを実機で示すまでは出荷しない。
//!
//! # 差分は allowlist で見る
//!
//! 「更新頻度だけ違う」を、こちら側の想像で決めない。
//! 対象集合・トポロジ・並び・位置・解像度・画素形式・回転・拡大・出力方式が
//! **すべて同じ**で、違いが更新頻度に対応する値だけのときにだけ、そう呼ぶ。
//! どれか1つでも違えば「更新頻度だけの違いではない」と言う。

use serde::{Deserialize, Serialize};

/// 1つの表示経路のうち、比較に使う部分。
///
/// 生の構造体をそのまま持たない。**比較したい意味だけを取り出す。**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayPathFacts {
    /// 出力先の識別子。どの画面かを表す。
    pub target_id: u32,
    /// 描画元の識別子。**複製と拡張はここで見分ける。**
    /// 2つの出力先が同じ描画元を指していれば複製。
    pub source_id: u32,
    /// 複製グループ。同じ番号の経路は同じ内容を映している。
    pub clone_group: u32,
    /// アダプターの識別子。ログオンし直すと変わりうる。
    pub adapter_id_low: u32,
    pub adapter_id_high: i32,
    /// 画面上の位置。
    pub source_x: i32,
    pub source_y: i32,
    /// 解像度。
    pub width: u32,
    pub height: u32,
    /// 画素形式。
    pub pixel_format: u32,
    /// 回転。
    pub rotation: u32,
    /// 拡大。
    pub scaling: u32,
    /// 接続方式。
    pub output_technology: i32,
    /// 更新頻度（分子・分母）。ここだけが違うなら「更新頻度の違い」。
    pub refresh_numerator: u32,
    pub refresh_denominator: u32,
    /// 可変更新頻度が有効になっている経路。**有効なら比較の対象にしない。**
    pub boost_refresh_rate: bool,
}

/// ある時点の表示構成ぜんぶ。
///
/// トポロジ ID は持たない。`QueryDisplayConfig` がそれを返すのは
/// `QDC_DATABASE_CURRENT` のときだけで、いま画面に出ている構成を読む用途では受け取れない。
/// **受け取れないものを持っているふりをしない。**
/// 複製か拡張かは、経路の描画元が重なっているかどうかで見分ける。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayProfile {
    pub paths: Vec<DisplayPathFacts>,
}

impl DisplayProfile {
    /// 保存済みの配置をディスプレイ構成へ結び付けるために必要な最低条件。
    /// 更新頻度は座標復元の安全性に影響しないため、ここでは分母を要件にしない。
    pub fn has_usable_topology(&self) -> bool {
        !self.paths.is_empty()
            && self.paths.len() <= 64
            && self
                .paths
                .iter()
                .all(|path| path.width > 0 && path.height > 0)
    }
}

/// 2つの構成を見比べた結果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ProfileComparison {
    /// 完全に同じ。
    Identical,
    /// 更新頻度だけが違う。切り替えの候補になりうる唯一の形。
    RefreshOnly {
        /// どの画面の頻度が変わるか。
        changed_target_ids: Vec<u32>,
    },
    /// 更新頻度以外も違う。**切り替えの対象にしない。**
    NotComparable {
        /// 何が違ったか。画面に出す日本語。
        reason: String,
    },
}

/// 可変更新頻度が有効なら、そもそも比較しない。
///
/// この状態の画面は Windows 側が頻度を動かしている。
/// こちらが持っている数字が、いま出ている頻度と同じとは限らない。
fn any_boost(profile: &DisplayProfile) -> bool {
    profile.paths.iter().any(|path| path.boost_refresh_rate)
}

/// 更新頻度以外がすべて同じか。
fn same_except_refresh(left: &DisplayPathFacts, right: &DisplayPathFacts) -> bool {
    left.target_id == right.target_id
        // 描画元と複製グループが変われば複製と拡張の別。更新頻度だけの違いではない。
        && left.source_id == right.source_id
        && left.clone_group == right.clone_group
        && left.adapter_id_low == right.adapter_id_low
        && left.adapter_id_high == right.adapter_id_high
        && left.source_x == right.source_x
        && left.source_y == right.source_y
        && left.width == right.width
        && left.height == right.height
        && left.pixel_format == right.pixel_format
        && left.rotation == right.rotation
        && left.scaling == right.scaling
        && left.output_technology == right.output_technology
}

/// 2つの構成を見比べる。純関数。
pub fn compare_profiles(left: &DisplayProfile, right: &DisplayProfile) -> ProfileComparison {
    if any_boost(left) || any_boost(right) {
        return ProfileComparison::NotComparable {
            reason: "画面の更新頻度をWindowsが自動で変えている状態です。見比べられません。"
                .to_owned(),
        };
    }
    if left.paths.len() != right.paths.len() {
        return ProfileComparison::NotComparable {
            reason: "つながっている画面の数が違います。".to_owned(),
        };
    }

    let mut changed = Vec::new();
    // 並び順そのものも比較の対象にする。順番が違えば同じ構成とは言えない。
    for (left_path, right_path) in left.paths.iter().zip(right.paths.iter()) {
        if !same_except_refresh(left_path, right_path) {
            return ProfileComparison::NotComparable {
                reason: "更新頻度のほかにも違うところがあります。".to_owned(),
            };
        }
        if left_path.refresh_numerator != right_path.refresh_numerator
            || left_path.refresh_denominator != right_path.refresh_denominator
        {
            changed.push(left_path.target_id);
        }
    }

    if changed.is_empty() {
        ProfileComparison::Identical
    } else {
        ProfileComparison::RefreshOnly {
            changed_target_ids: changed,
        }
    }
}

/// ウィンドウ座標を戻せる同じデスクトップ構成かを比べる。
///
/// 更新頻度と可変更新頻度の状態はウィンドウ座標を変えないため比較対象外。
/// 画面数、接続先、複製関係、向き、解像度、拡張デスクトップ上の位置が違えば
/// 別構成として必ず停止する。
pub fn compare_topology(left: &DisplayProfile, right: &DisplayProfile) -> Result<(), String> {
    if !left.has_usable_topology() || !right.has_usable_topology() {
        return Err("表示構成を安全に読み取れませんでした。".to_owned());
    }
    if left.paths.len() != right.paths.len() {
        return Err("つながっている画面の数が保存時と違います。".to_owned());
    }
    if left
        .paths
        .iter()
        .zip(right.paths.iter())
        .any(|(saved, current)| !same_except_refresh(saved, current))
    {
        return Err("画面の接続先、並び、向き、または解像度が保存時と違います。".to_owned());
    }
    Ok(())
}

/// 表示のために頻度を Hz へ直す。割り切れないものは丸めない。
pub fn refresh_hz(path: &DisplayPathFacts) -> Option<f64> {
    if path.refresh_denominator == 0 {
        return None;
    }
    Some(f64::from(path.refresh_numerator) / f64::from(path.refresh_denominator))
}

#[cfg(windows)]
mod imp {
    use super::{DisplayPathFacts, DisplayProfile};
    use crate::windows::{WindowsError, WindowsErrorKind, WindowsResult};
    use windows::Win32::Devices::Display::{
        GetDisplayConfigBufferSizes, QueryDisplayConfig, DISPLAYCONFIG_MODE_INFO,
        DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_MODE_INFO_TYPE_TARGET,
        DISPLAYCONFIG_PATH_INFO, QDC_ONLY_ACTIVE_PATHS, QDC_VIRTUAL_MODE_AWARE,
        QDC_VIRTUAL_REFRESH_RATE_AWARE,
    };

    /// 可変更新頻度を示すフラグ。`DISPLAYCONFIG_PATH_INFO` の flags に入る。
    const PATH_BOOST_REFRESH_RATE: u32 = 0x0000_0010;

    pub fn read() -> WindowsResult<DisplayProfile> {
        read_with(QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE | QDC_VIRTUAL_REFRESH_RATE_AWARE)
    }

    pub fn read_with(
        flags: windows::Win32::Devices::Display::QUERY_DISPLAY_CONFIG_FLAGS,
    ) -> WindowsResult<DisplayProfile> {
        let virtual_aware = (flags.0 & QDC_VIRTUAL_MODE_AWARE.0) != 0;
        let mut path_count = 0u32;
        let mut mode_count = 0u32;
        let status =
            unsafe { GetDisplayConfigBufferSizes(flags, &mut path_count, &mut mode_count) };
        if status.is_err() {
            return Err(WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "GetDisplayConfigBufferSizes",
                Some(i64::from(status.0)),
            ));
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        // 第6引数は `QDC_DATABASE_CURRENT` のときだけ渡せる。
        // それ以外で渡すと ERROR_INVALID_PARAMETER になる。実際にそれで落ちた。
        let status = unsafe {
            QueryDisplayConfig(
                flags,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if status.is_err() {
            return Err(WindowsError::new(
                WindowsErrorKind::ApiFailure,
                "QueryDisplayConfig",
                Some(i64::from(status.0)),
            ));
        }
        paths.truncate(path_count as usize);
        modes.truncate(mode_count as usize);

        let facts = paths
            .iter()
            .map(|path| {
                // 位置・解像度・画素形式は source モードから、頻度は target モードから読む。
                // path 側の refreshRate ではなく target の vSyncFreq を使う。
                // full な target モードがあるときはそちらが使われるため。
                // `QDC_VIRTUAL_MODE_AWARE` を渡すと、添字は 32bit の生値ではなく
                // 16bit ずつに詰められた形になる。生値のまま読むと必ず外れ、
                // **解像度が 0x0 として返る。** 実際に一度そうなった。
                let (source_raw, target_raw) = if virtual_aware {
                    (
                        packed_high(unsafe { path.sourceInfo.Anonymous.Anonymous._bitfield }),
                        packed_high(unsafe { path.targetInfo.Anonymous.Anonymous._bitfield }),
                    )
                } else {
                    (unsafe { path.sourceInfo.Anonymous.modeInfoIdx }, unsafe {
                        path.targetInfo.Anonymous.modeInfoIdx
                    })
                };
                let source = mode_index(&modes, source_raw, true);
                let target = mode_index(&modes, target_raw, false);
                let (source_x, source_y, width, height, pixel_format) = match source {
                    Some(index) => {
                        let mode = unsafe { modes[index].Anonymous.sourceMode };
                        (
                            mode.position.x,
                            mode.position.y,
                            mode.width,
                            mode.height,
                            mode.pixelFormat.0 as u32,
                        )
                    }
                    None => (0, 0, 0, 0, 0),
                };
                let (refresh_numerator, refresh_denominator) = match target {
                    Some(index) => {
                        let mode = unsafe { modes[index].Anonymous.targetMode };
                        (
                            mode.targetVideoSignalInfo.vSyncFreq.Numerator,
                            mode.targetVideoSignalInfo.vSyncFreq.Denominator,
                        )
                    }
                    // target モードが無い経路は、path 側の値しか手がかりが無い。
                    None => (
                        path.targetInfo.refreshRate.Numerator,
                        path.targetInfo.refreshRate.Denominator,
                    ),
                };
                DisplayPathFacts {
                    target_id: path.targetInfo.id,
                    source_id: path.sourceInfo.id,
                    clone_group: if virtual_aware {
                        packed_low(unsafe { path.sourceInfo.Anonymous.Anonymous._bitfield })
                    } else {
                        0
                    },
                    adapter_id_low: path.targetInfo.adapterId.LowPart,
                    adapter_id_high: path.targetInfo.adapterId.HighPart,
                    source_x,
                    source_y,
                    width,
                    height,
                    pixel_format,
                    rotation: path.targetInfo.rotation.0 as u32,
                    scaling: path.targetInfo.scaling.0 as u32,
                    output_technology: path.targetInfo.outputTechnology.0,
                    refresh_numerator,
                    refresh_denominator,
                    boost_refresh_rate: (path.flags & PATH_BOOST_REFRESH_RATE) != 0,
                }
            })
            .collect();

        Ok(DisplayProfile { paths: facts })
    }

    /// 詰められた添字の上位16ビット。ここに mode の添字が入る。
    /// 下位16ビットは複製グループ／デスクトップモードの添字で、別の意味。
    const fn packed_high(bitfield: u32) -> u32 {
        let value = bitfield >> 16;
        // 16bit 側の無効値は 0xffff。32bit の番兵へ揃えてから渡す。
        if value == 0xffff {
            0xffff_ffff
        } else {
            value
        }
    }

    /// 詰められた添字の下位16ビット。複製グループの識別に使う。
    const fn packed_low(bitfield: u32) -> u32 {
        bitfield & 0xffff
    }

    /// モード配列の添字を安全に取り出す。無効値や種別違いは `None`。
    fn mode_index(modes: &[DISPLAYCONFIG_MODE_INFO], raw: u32, want_source: bool) -> Option<usize> {
        const INVALID: u32 = 0xffff_ffff;
        if raw == INVALID {
            return None;
        }
        let index = raw as usize;
        let mode = modes.get(index)?;
        let wanted = if want_source {
            DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE
        } else {
            DISPLAYCONFIG_MODE_INFO_TYPE_TARGET
        };
        if mode.infoType == wanted {
            Some(index)
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::DisplayProfile;
    use crate::windows::{WindowsError, WindowsResult};

    pub fn read() -> WindowsResult<DisplayProfile> {
        Err(WindowsError::unsupported("read display configuration"))
    }
}

/// いまの表示構成を読む。**読むだけ。**
pub fn read_display_profile() -> crate::windows::WindowsResult<DisplayProfile> {
    imp::read()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(target_id: u32, numerator: u32) -> DisplayPathFacts {
        DisplayPathFacts {
            target_id,
            source_id: target_id,
            clone_group: 0,
            adapter_id_low: 1,
            adapter_id_high: 0,
            source_x: 0,
            source_y: 0,
            width: 1920,
            height: 1080,
            pixel_format: 1,
            rotation: 1,
            scaling: 1,
            output_technology: 5,
            refresh_numerator: numerator,
            refresh_denominator: 1,
            boost_refresh_rate: false,
        }
    }

    fn profile(paths: Vec<DisplayPathFacts>) -> DisplayProfile {
        DisplayProfile { paths }
    }

    #[test]
    fn the_same_configuration_twice_is_identical() {
        let one = profile(vec![path(1, 60)]);
        assert_eq!(compare_profiles(&one, &one), ProfileComparison::Identical);
    }

    #[test]
    fn only_the_refresh_rate_differing_is_recognised_as_such() {
        let low = profile(vec![path(1, 60)]);
        let high = profile(vec![path(1, 144)]);
        assert_eq!(
            compare_profiles(&low, &high),
            ProfileComparison::RefreshOnly {
                changed_target_ids: vec![1],
            }
        );
    }

    #[test]
    fn a_resolution_change_is_not_a_refresh_change() {
        let low = profile(vec![path(1, 60)]);
        let mut other = path(1, 144);
        other.width = 2560;
        let high = profile(vec![other]);
        assert!(matches!(
            compare_profiles(&low, &high),
            ProfileComparison::NotComparable { .. }
        ));
    }

    #[test]
    fn a_different_screen_count_is_not_comparable() {
        let one = profile(vec![path(1, 60)]);
        let two = profile(vec![path(1, 60), path(2, 60)]);
        assert!(matches!(
            compare_profiles(&one, &two),
            ProfileComparison::NotComparable { .. }
        ));
    }

    #[test]
    fn switching_between_extend_and_clone_is_not_a_refresh_change() {
        // 拡張: 2つの画面が別々の描画元。
        let extended = profile(vec![path(1, 60), path(2, 60)]);
        // 複製: 2つの画面が同じ描画元を指す。
        let mut second = path(2, 60);
        second.source_id = 1;
        let cloned = profile(vec![path(1, 60), second]);
        assert!(matches!(
            compare_profiles(&extended, &cloned),
            ProfileComparison::NotComparable { .. }
        ));
    }

    #[test]
    fn a_variable_refresh_screen_is_never_compared() {
        // 頻度をWindowsが動かしている画面。手元の数字が今の値とは限らない。
        let mut boosted = path(1, 144);
        boosted.boost_refresh_rate = true;
        let one = profile(vec![path(1, 60)]);
        let other = profile(vec![boosted]);
        assert!(matches!(
            compare_profiles(&one, &other),
            ProfileComparison::NotComparable { .. }
        ));
        // 逆向きでも同じ。
        assert!(matches!(
            compare_profiles(&other, &one),
            ProfileComparison::NotComparable { .. }
        ));
    }

    #[test]
    fn a_reordered_path_list_is_not_treated_as_the_same_configuration() {
        let ordered = profile(vec![path(1, 60), path(2, 60)]);
        let swapped = profile(vec![path(2, 60), path(1, 60)]);
        assert!(matches!(
            compare_profiles(&ordered, &swapped),
            ProfileComparison::NotComparable { .. }
        ));
    }

    #[test]
    fn topology_comparison_ignores_refresh_but_not_geometry() {
        let saved = profile(vec![path(1, 60)]);
        let mut faster = path(1, 144);
        faster.boost_refresh_rate = true;
        assert_eq!(compare_topology(&saved, &profile(vec![faster])), Ok(()));

        let mut moved = path(1, 60);
        moved.source_x = 1920;
        assert!(compare_topology(&saved, &profile(vec![moved])).is_err());
    }

    #[test]
    fn an_unusable_denominator_gives_no_hz_rather_than_a_wrong_one() {
        let mut broken = path(1, 60);
        broken.refresh_denominator = 0;
        assert_eq!(refresh_hz(&broken), None);
        assert_eq!(refresh_hz(&path(1, 60)), Some(60.0));
    }

    /// どのフラグの組み合わせなら読めるのかを実機で確かめる。
    ///
    /// `QDC_VIRTUAL_*` は新しい環境でしか通らない可能性がある。
    /// **通らないものを黙って使い続けない。** 通る組み合わせを測ってから決める。
    #[cfg(windows)]
    #[test]
    #[ignore = "実機の表示構成を読む"]
    fn which_query_flags_does_this_machine_accept() {
        use windows::Win32::Devices::Display::{
            QDC_ONLY_ACTIVE_PATHS, QDC_VIRTUAL_MODE_AWARE, QDC_VIRTUAL_REFRESH_RATE_AWARE,
        };
        for (label, flags) in [
            ("active", QDC_ONLY_ACTIVE_PATHS),
            (
                "active+virtual",
                QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE,
            ),
            (
                "active+virtual+refresh",
                QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE | QDC_VIRTUAL_REFRESH_RATE_AWARE,
            ),
        ] {
            let outcome = imp::read_with(flags);
            println!(
                "EVIDENCE: display_profile flags={label} ok={} detail={:?}",
                outcome.is_ok(),
                outcome.as_ref().err()
            );
        }
    }

    /// 実機の表示構成を1回読む。**適用はしない。**
    #[test]
    #[ignore = "実機の表示構成を読む"]
    fn read_the_real_display_configuration() {
        let profile = read_display_profile().expect("read display configuration");
        println!("EVIDENCE: display_profile paths={}", profile.paths.len());
        for path in &profile.paths {
            println!(
                "EVIDENCE: display_profile target={} source={} clone={} {}x{} at ({},{}) hz={:?} boost={} tech={}",
                path.target_id,
                path.source_id,
                path.clone_group,
                path.width,
                path.height,
                path.source_x,
                path.source_y,
                refresh_hz(path),
                path.boost_refresh_rate,
                path.output_technology
            );
        }
        // 同じ瞬間に2回読めば同じはず。読み取りが安定していることの最低限の確認。
        let again = read_display_profile().expect("read display configuration twice");
        println!(
            "EVIDENCE: display_profile second_read_matches={}",
            profile == again
        );
        assert_eq!(
            compare_profiles(&profile, &again),
            ProfileComparison::Identical
        );

        // 「2回読んで同じ」だけだと、全経路が 0x0 / 分母0 で返っても緑になる。
        // 実際に一度そうなった。読めた値そのものが成立しているかを見る。
        assert!(!profile.paths.is_empty(), "画面が1つも読めていない");
        for path in &profile.paths {
            assert!(
                path.width > 0 && path.height > 0,
                "解像度が 0 のまま返っている。添字の読み方が違う"
            );
            let hz = refresh_hz(path).expect("更新頻度の分母が 0");
            assert!(
                (20.0..=1000.0).contains(&hz),
                "更新頻度が現実的な範囲にない: {hz}"
            );
        }
    }
}
