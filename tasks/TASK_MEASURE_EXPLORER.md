# タスク: Explorer 系の未判定を測る（タスクバーに依存しない）

`BRIEF.md` と `docs/RULES.md` を読め。
`docs/DISPLAY_ONLY_TRIAGE.md` の**訂正の節**と、末尾の `GLFW30` の節を必ず読め。

## なぜ今これか

タスクバー系の測定は、この機で**全画面のアプリ（`GLFW30`）が覆っている**ため止まっている。
一方 **Explorer 系はタスクバーに依存しない。** 新しい窓を開いて中を読むだけ。

`explorer.launch_target` はその型で測れて、**効くと分かって昇格した。**
同じ型が使える項目が残っている。

## 対象（3件まで。確実に）

`docs/DISPLAY_ONLY_TRIAGE.md` の未判定のうち、Explorer の窓を開いて中を読めるもの。
たとえば `explorer.hide_empty_drives`、`explorer.nav_show_all`、`explorer.drive_letters`。
**表を見て、観測方法が書けるものを自分で3件選べ。選んだ理由を書け。**

## 型（`launch_target` と同じ）

`ui_probe.rs` の `explorer_launch_target_write_changes_the_fresh_explorer_window` を読め。
**再実装するな。同じ形に従え。**

1. 現在値を読む
2. **その設定が効く操作は何か**を先に決める
   （新しい窓を開く？ 特定のフォルダーを開く？ ツリーを展開する？）
   **効かない経路を観測して「効かない」と書くのが最悪の失敗。** `launch_target` で一度やっている
3. 今と違う値を書く
4. **新しい Explorer を開いて** UIA で観測する
5. 戻して再観測

## 判定

- 変わった → **昇格候補**として報告（昇格まではしなくてよい）
- 変わらなかった → 「実測して効かない」へ格上げ
- 窓が開かない・要素が取れない → `measured=false reason=...`。**保留。閉じるな**

**同じ測定を2回走らせて、同じ結果になることを確かめてから報告しろ。**

## 絶対に守ること

- 開いた窓は閉じろ。設定は戻せ
- **値が未設定だったなら、未設定へ戻せ。** 既定値を書くな
- ウィンドウタイトル・ファイル名を出すな
- `EVIDENCE:` に前・後・戻し後の観測値を出せ

## 完了条件

- `cargo test --lib` が通る
- `cargo clippy --all-targets -- -D warnings` が通る
- `cargo fmt --check` が通る
- `npm run build` が通る（テスト23件、CSS警告0件）
- `docs/DISPLAY_ONLY_TRIAGE.md` を更新しろ
