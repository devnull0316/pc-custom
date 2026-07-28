//! 適用した設定が今も残っているかの照合（read-only）。
//!
//! Windows の更新のあと、変更したはずの設定が既定へ戻っていることがある。
//! ここは「戻っているかどうか」を見せるだけで、**Windows を一切変更しない。**
//!
//! # 原因を書かない
//!
//! 値が基準と違っていても、**原因が更新だとは限らない。**
//! 利用者本人が変えたのかもしれないし、別のアプリかもしれない。
//! 時間的に近いことは因果ではないので、画面には「基準と違います」までしか書かない。
//! 更新日時は参考として別枠に出すが、そこにも原因の断定を書かない。
//!
//! # なぜ fingerprint で照合しないのか
//!
//! journal には適用時の `applied_fingerprint` が残っている。ところが fingerprint には
//! 観測時の `os_build` が入っている（`DetectedState::stable_fingerprint`）。
//! つまり**更新でビルドが上がると、値が1バイトも変わっていなくても fingerprint は必ず変わる。**
//! 更新後の照合に使うと全項目が「違う」になる。この機能が一番やってはいけない誤報なので、
//! 照合は fingerprint ではなく、バックアップに残した**実際のバイト列**と現在値の比較で行う。

use serde::{Deserialize, Serialize};

/// 1項目を照合した結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthProbe {
    /// 適用したときの値のままだった。
    HoldsApplied,
    /// 適用する前の値に戻っていた。
    BackToPrevious,
    /// 適用値でも適用前の値でもない、第三の値だった。
    Neither,
    /// 照合できなかった。読めなかった場合と、照合手段がない場合の両方。
    Uncomparable,
}

/// 照合にかける前の1項目。
#[derive(Debug, Clone)]
pub struct HealthInput {
    pub action_id: String,
    /// 画面に出す名前。Action ID やレジストリパスは出さない。
    pub name: String,
    /// 最後に適用した時刻の表示文字列。
    pub applied_at: String,
    pub probe: HealthProbe,
    /// 照合できなかったときだけ、その理由を日本語で。
    pub uncomparable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthEntry {
    pub action_id: String,
    pub name: String,
    /// 何が起きているか。原因は書かない。
    pub note: String,
    pub applied_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    /// 比較の基準があるか。無ければ他のリストはすべて空になる。
    pub has_baseline: bool,
    /// 基準が無いときに画面へ出す一文。
    pub no_baseline_note: Option<String>,
    /// 基準どおりだったもの。
    pub holding: Vec<HealthEntry>,
    /// 基準と違っていたもの。**原因は書かない。**
    pub changed: Vec<HealthEntry>,
    /// 確認できなかったもの。ここを `holding` に混ぜてはいけない。
    pub unknown: Vec<HealthEntry>,
    /// 参考情報としての更新日時。原因の断定ではないと分かる文にする。
    pub update_reference: Option<String>,
    /// 見出しに出す一文。
    pub summary: String,
}

/// 基準が1件も無いときの文言。「たぶん大丈夫」とは書かない。
const NO_BASELINE: &str =
    "まだ照合の基準がありません。このアプリで設定を1つでも適用すると、そのときの値を基準にして、\
     あとから今の値と見比べられるようになります。";

/// 参考として更新の確認日時を出すときの文言。**因果を断定しない。**
///
/// Windows から取れるのは「更新を最後に確認した日時」であって、
/// 「更新を当てた日時」でも「更新が何かを書き換えた日時」でもない。そのまま書く。
fn update_reference_note(last_checked: &str) -> String {
    format!(
        "参考: Windows が更新を最後に確認したのは {last_checked} です。\
         日付が近いことと、設定が変わった原因が更新であることは別の話です。\
         利用者自身や他のアプリが変えた可能性もあります。"
    )
}

/// 純関数。I/O をしないので、Windows 無しでも挙動を固定できる。
pub fn build_health_report(
    inputs: Vec<HealthInput>,
    update_last_checked: Option<&str>,
) -> HealthReport {
    if inputs.is_empty() {
        return HealthReport {
            has_baseline: false,
            no_baseline_note: Some(NO_BASELINE.to_owned()),
            holding: Vec::new(),
            changed: Vec::new(),
            unknown: Vec::new(),
            // 基準が無いのに更新日時だけ出すと、見た人が勝手に因果を作る。出さない。
            update_reference: None,
            summary: "照合できる基準がまだありません。".to_owned(),
        };
    }

    let mut holding = Vec::new();
    let mut changed = Vec::new();
    let mut unknown = Vec::new();

    for input in inputs {
        let entry = |note: String| HealthEntry {
            action_id: input.action_id.clone(),
            name: input.name.clone(),
            note,
            applied_at: input.applied_at.clone(),
        };
        match input.probe {
            HealthProbe::HoldsApplied => {
                holding.push(entry("適用したときの状態のままです。".to_owned()));
            }
            HealthProbe::BackToPrevious => {
                changed.push(entry(
                    "適用する前の状態に戻っています。もう一度適用できます。".to_owned(),
                ));
            }
            HealthProbe::Neither => {
                changed.push(entry(
                    "適用したときとも、適用する前とも違う状態です。".to_owned(),
                ));
            }
            HealthProbe::Uncomparable => {
                let reason = input
                    .uncomparable_reason
                    .clone()
                    .unwrap_or_else(|| "今の状態を読み取れませんでした。".to_owned());
                unknown.push(entry(reason));
            }
        }
    }

    let summary = if changed.is_empty() && unknown.is_empty() {
        format!("{}件すべて、適用したときの状態のままです。", holding.len())
    } else if changed.is_empty() {
        format!(
            "{}件は適用したときのまま、{}件は確認できませんでした。",
            holding.len(),
            unknown.len()
        )
    } else {
        format!(
            "{}件が適用したときと違う状態です。（そのままが{}件、確認できないものが{}件）",
            changed.len(),
            holding.len(),
            unknown.len()
        )
    };

    HealthReport {
        has_baseline: true,
        no_baseline_note: None,
        holding,
        changed,
        unknown,
        update_reference: update_last_checked.map(update_reference_note),
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(action_id: &str, probe: HealthProbe) -> HealthInput {
        HealthInput {
            action_id: action_id.to_owned(),
            name: "テスト項目".to_owned(),
            applied_at: "2026-07-20 10:00".to_owned(),
            probe,
            uncomparable_reason: None,
        }
    }

    #[test]
    fn without_a_baseline_it_says_so_instead_of_reporting_health() {
        let report = build_health_report(Vec::new(), Some("2026-07-25"));
        assert!(!report.has_baseline);
        assert!(report.no_baseline_note.is_some());
        assert!(report.holding.is_empty());
        assert!(report.changed.is_empty());
        assert!(report.unknown.is_empty());
        // 基準が無いときは更新日時も出さない。出すと見た人が因果を作る。
        assert!(report.update_reference.is_none());
    }

    #[test]
    fn unreadable_items_never_land_in_the_healthy_list() {
        let report = build_health_report(
            vec![
                input("a", HealthProbe::HoldsApplied),
                input("b", HealthProbe::Uncomparable),
            ],
            None,
        );
        assert_eq!(report.holding.len(), 1);
        assert_eq!(report.unknown.len(), 1);
        assert_eq!(report.unknown[0].action_id, "b");
        assert!(report.changed.is_empty());
        // 「確認できない」が「大丈夫」に混ざっていないこと。
        assert!(!report.holding.iter().any(|entry| entry.action_id == "b"));
        assert!(report.summary.contains("確認できません"));
    }

    #[test]
    fn a_changed_item_is_never_blamed_on_windows_update() {
        let report = build_health_report(
            vec![
                input("a", HealthProbe::BackToPrevious),
                input("b", HealthProbe::Neither),
            ],
            Some("2026-07-25"),
        );
        assert_eq!(report.changed.len(), 2);
        for entry in &report.changed {
            // 原因を名指ししていないこと。
            assert!(!entry.note.contains("Update"));
            assert!(!entry.note.contains("更新"));
            assert!(!entry.note.contains("原因"));
            assert!(!entry.note.contains("せい"));
        }
        assert!(!report.summary.contains("更新"));
        // 更新日時は参考として出るが、断定しない文が必ず添う。
        let reference = report.update_reference.expect("参考情報");
        assert!(reference.starts_with("参考:"));
        assert!(reference.contains("別の話"));
        assert!(reference.contains("可能性"));
    }

    #[test]
    fn uncomparable_reason_is_shown_when_given() {
        let mut item = input("c", HealthProbe::Uncomparable);
        item.uncomparable_reason = Some("この項目は照合のしくみがありません。".to_owned());
        let report = build_health_report(vec![item], None);
        assert_eq!(
            report.unknown[0].note,
            "この項目は照合のしくみがありません。"
        );
    }

    #[test]
    fn all_healthy_reads_as_all_healthy() {
        let report = build_health_report(
            vec![
                input("a", HealthProbe::HoldsApplied),
                input("b", HealthProbe::HoldsApplied),
            ],
            None,
        );
        assert_eq!(report.summary, "2件すべて、適用したときの状態のままです。");
        assert!(report.changed.is_empty());
        assert!(report.unknown.is_empty());
    }
}
