# タスク: さっきの測定は、設定が効く経路を通っていない

`BRIEF.md` と `docs/RULES.md` を読め。

## 何が起きたか

`explorer.launch_target` の測定で、観測用に **`explorer.exe /n`** を起動している
（`src-tauri/src/windows/ui_probe.rs:2110` 付近）。

**`/n` は引数付きの起動。** この設定が効くのは、
エクスプローラーを**引数なし**で開いたとき（タスクバーのアイコン、Win+E）。
つまり**設定が効く経路を通さずに「変わらなかった」と結論している。**

自分で書いた `docs/DISPLAY_ONLY_TRIAGE.md` には
「新規 `explorer.exe` を**引数なし**で起動し」とある。実装がそれに従っていない。

これは「効かない」ではなく「**測れていない**」。結論を出す前に直せ。

## やること

1. `explorer.launch_target` の観測を、**引数なしの起動**に直して測り直せ
   - 引数なしだと既存の窓が再利用される可能性がある。
     **新しい窓が本当に開いたことを確かめてから観測しろ**
   - 開かないなら `measured=false reason=...` を出せ。それは正直な結果
2. `explorer.drive_letters` も同じ目で見直せ。
   観測している対象が、その設定が実際に効く場所か確かめろ
   （ドライブ文字の表示は「PC」を開いたときの一覧項目名に出る）
3. **他の測定でも同じ間違いが無いか、`ui_probe.rs` 全体を見ろ。**
   設定が効かない経路を観測しているプローブが他にあれば挙げろ

## 判定

- 変わった → 昇格候補（ロールバックも実測しろ）
- 変わらなかった → 「実測して効かない」へ格上げし `docs/STATUS.md` に証拠を残せ
- 測れなかった → `measured=false reason=...` を出し、難易度を直せ

**「測れていない」を「効かない」と書くな。** そこが今回の失敗。

## 完了条件

- `cargo test --lib` が通る（405件）
- `cargo clippy --all-targets -- -D warnings` が通る
- `cargo fmt --check` が通る
- `npm run build` が通る（テスト23件、CSS警告0件）
- 実機テストを走らせて `EVIDENCE:` 行を貼れ
- `docs/DISPLAY_ONLY_TRIAGE.md` を更新しろ
