# タスク: 表示専用のうち「測れる（小）」を実測する

`BRIEF.md` と `docs/RULES.md` を読め。
`docs/DISPLAY_ONLY_TRIAGE.md` の難易度「小」の項目を測る。

## 対象

`explorer.launch_target` と `explorer.drive_letters`。
（`info_tips` と `separate_process` は実測済みなので触るな）

## やること

`src-tauri/src/windows/ui_probe.rs` の既存プローブの型に従って、
**書く前・書いた後**を別プロセスの Explorer から観測する。

1. 現在値を読む
2. **今と違う値**を書く（同じ値を書いて同じ値が返っても何も証明しない）
3. **新しく Explorer を開いて** UIA で観測する
4. 元へ戻す
5. もう一度観測して戻ったことを確認する

## 判定

- **観測が変わった** → 昇格候補。`MethodClass` と `ActionKind` を変えてよい。
  ただし**変更経路を開けるなら、ロールバックも実測しろ**
- **観測が変わらなかった** → 表示専用のまま。ただし
  「未確認」から「**実測して効かない**」へ格上げし、`docs/STATUS.md` に証拠を残せ
- **観測できなかった** → `measured=false reason=...` を出し、難易度を「中」か「不可」へ直せ

**どちらに転んでも文書を更新しろ。** 測ったのに記録しないのが一番もったいない。

## 検証

`EVIDENCE:` 行に、前・書いた後・戻した後の**観測値**を出せ。
ウィンドウタイトルやファイル名は出すな。

## 完了条件

- `cargo test --lib` が通る（405件）
- `cargo clippy --all-targets -- -D warnings` が通る
- `cargo fmt --check` が通る
- `npm run build` が通る（テスト23件通過、CSS警告0件）
- 実機テストを**実際に走らせて** `EVIDENCE:` 行を貼れ
- `docs/DISPLAY_ONLY_TRIAGE.md` と、必要なら `docs/STATUS.md` を更新しろ
