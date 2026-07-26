# BRIEF §4 の監査（部分・未完了）

2026-07-26。**この監査は完了していない。** 7観点の並列レビューを走らせたが、9エージェント全てが
セッション上限で落ち、1件も完了しなかった。以下は Claude が自分で直接読んで確認した3点だけ。
残りは未確認であり、「指摘なし」ではなく「見ていない」。

## 確認できた（3件）

### 1. トランザクションの順序は壊れていない
`engine/transaction.rs` の差分は大半が rustfmt の整形。実質の変更は3つで、いずれも強化側。

- `ensure_layout_invocations_current()` を commit 前に追加。配置スナップショットや登録ゲーム集合が
  preview 後に変わっていれば stale として拒否する。
- 復元できなかった対象を `result_details` として返す。黙って飛ばさない。
- `CommitResult` に `details` を追加。

**検証→ロック→バックアップ耐久化→適用→各検証→失敗時は逆順ロールバック**の順序は変更なし。
`applied_indices` によるロールバック経路も無傷。

### 2. `open_windows_settings` は URI を受け取らない
`commands.rs:207`。フロントから来るのは Action ID の文字列だけで、`ActionId::from_str` で
弾いたうえで `settings_link` のコンパイル時テーブルへ引き当てる。任意の URI を
`ShellExecuteW` へ渡す経路はない。

### 3. 復元失敗の表示にウィンドウタイトルは出ない
`WindowLayoutIssue.target` の生成箇所は `window_placement.rs:1853` の1つだけで、
入れているのは `application_label`。その `application_label` は `window_placement.rs:1113` で
実行ファイルの canonical path のファイル名から作られる（`chrome.exe（2）` 形式）。
タイトルではない。タイトル型は `Debug` を `<redacted-window-title>` に潰してある。

## 未確認（見ていない）

- `windows/window_placement.rs` 2,507行の Win32 正当性。特に **HWND の再利用**（保存した
  ハンドル値が別ウィンドウに再割り当てされる）と **マルチモニタ構成変更後の画面外復元**。
  codex は PID とプロセス生成時刻で本人性を確かめる設計だと書いているが、コードは読んでいない。
- `engine/mod.rs` の +282行。
- 許可コマンド 27個のうち、上記1個を除く 26個の引数検証。特に
  `storage_temp_cleanup_apply`（削除は不可逆）と `profile_run_now`。
- バックアップ封筒の後方互換。更新前に書かれたバックアップが読めなくなると、
  更新前の変更を元に戻せなくなる。それを保証するテストの有無は未確認。
- 新規33テストが「何もしない実装でも通るか」の検査。このプロジェクトが一度やられた失敗そのもの。
- UI 文言の BRIEF 適合（9項目の提示、専門用語の排除、誇張の排除）。
- `model.ts` の `riskLabel` が「安全」→「低リスク」に変わった件の波及。BRIEF の
  「安全と言い切らない」に沿う変更だが、他の表示・ドキュメント・読み上げ文言が
  追随しているかは未確認。

## 既知のズレ

`docs/STATUS.md` に codex が書いた「ライブラリテストは252件（通常225成功）」は実測と合わない。
実測は **229 passed / 27 ignored = 256**。自己申告が少なくとも1箇所は古い。
