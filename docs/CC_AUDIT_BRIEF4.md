# BRIEF §4 の監査（完了）

2026-07-27。codex が実装した BRIEF §4 の残り3機能（ウィンドウ配置の保存/復元、既定アプリ案内、
音声出力先の表示）を7観点で監査した。**退行・禁止API・PII漏れ・権限拡大の実害はいずれも無し。**

多エージェントでの並列監査は2回試して2回とも全滅した（計160万トークン、成果0）。
セッション上限に当たり、部分結果も残らなかった。3回目は同じ構成をやめ、以下は全て直接読んで確認した。

## 1. 禁止API / 音声

- `IPolicyConfig` は**リポジトリ全体で0件**。非公開COMは使っていない。
- 音声は公開 `IMMDeviceEnumerator` の**読み取りのみ**。切り替えは `ms-settings:sound` へ案内。
- 実機smoke: `audio_output_count=6 default_exists=true`。**デバイス名は出力していない。**
- BRIEF の他の禁止事項（Defender/FW停止、pagefile、HPET/BCD、injection、任意PowerShell）も0件。
  `cmd.exe` の唯一の出現はテストの否定アサーション、`ShellExecute` は定数 `ms-settings:` URI と
  テスト用プローブのみ。

## 2. Win32 / ウィンドウ配置

- 使用APIは公開のみ。`CreateRemoteThread` / `SetWindowsHookEx` / サブクラス化は無し。
- **HWND再利用は防御済み、かつテスト済み。** `stale_snapshot_creation_time_is_rejected_before_candidate_capture`
  は自プロセスの窓を作り、プロセス生成時刻を1だけずらした偽 identity を渡して `CandidateRead::Skipped`
  になることを確認する。PIDが再利用されても別プロセスなら掴まない。
- `SetWindowPlacement` の前に `placement_is_valid()` で構造体を検証し、
  `SendMessageTimeoutW(WM_NULL, SMTO_ABORTIFHUNG)` で**応答性を確認**してから撃つ。
  WM_NULL は定義上何もしないメッセージで、これは文書化された作法。
- 実機smoke（自プロセスの窓のみ、既存のユーザーウィンドウには触れない）:
  `saved=(120,140 520x360 show=1) → moved=(300,270) → restored=(120,140 520x360 show=1)`

## 3. 個人情報

- 復元失敗の表示 `WindowLayoutIssue.target` の生成箇所は1つだけで、入れているのは
  実行ファイル名由来の `application_label`（`chrome.exe（2）` 形式）。**ウィンドウタイトルではない。**
- タイトル型は `Debug` を `<redacted-window-title>` に潰してある。
- 音声の実機smokeはデバイス名を出さず件数と既定有無だけを出力する。

## 4. 安全コアの退行

**無し。** むしろ強化されている。

- `transaction.rs` の差分は大半が rustfmt の整形。実質は
  `ensure_layout_invocations_current()`（preview後に配置スナップショットや登録ゲームが変われば拒否）、
  復元できなかった対象を返す `result_details`、`CommitResult.details` の3つ。
  **検証→ロック→バックアップ耐久化→適用→各検証→失敗時は逆順ロールバック**の順序は変更なし。
- **第三者変更は上書きしない**が生きている。`Third | Unknown` は
  「適用値と異なる現在状態を検出したため、自動では上書きしません。」で `recovery_required` に止まる。
- 新しい `needs_original_rollback_fence` は `ActionId::SetupWindowLayout` 限定。他66 Actionの
  復旧挙動は変わらない。状態ごとの真偽を固定するテスト付き。
- 新しい `recovery_parameters` も window layout 限定で、復旧時に「その後登録されたゲーム」を
  除外へ足す方向。除外は増えることはあっても減らない。失敗時は Unknown で fail-closed。
- `backup/envelope.rs` は列挙型に variant を足しただけ。過去のバックアップに新 variant は
  現れないので**旧バックアップの読み出しは壊れない**。

## 5. コマンドと権限

- 許可リストは8→27に増えたが、**新規コマンドは2つだけ**（`get_window_layout_status` /
  `save_window_layout`）。残る18個は以前から存在しフロントが呼んでいたもので、
  マニフェストが実態から8個分ズレていたのを揃えた形。
- `save_window_layout` がフロントから受け取るのは **bool 1個だけ**。パスもファイル名も
  ウィンドウ指定も無いので traversal も injection も起こらない。加えて mutation gate、
  プロセス間ロック、未復元があれば拒否、取得後に登録ゲーム集合を読み直して比較する TOCTOU 対策。
- `open_windows_settings` は Action ID を受けて `ActionId::from_str` で弾き、
  コンパイル時テーブルへ引き当てる。**任意URIは渡せない。**

## 6. Guided契約

- `validate` は**無条件に拒否**を返す（種別で分岐するのではなく常にブロック）。
- `assert_mutation_pipeline_refused` が Validate / Backup / Apply / VerifyApplied を
  直接叩いて全段階の拒否を確認し、既定アプリと音声の両方で呼ばれている。
  旧来の `demoted_actions_refuse_to_mutate` に新IDが載っていないのは**漏れではなく**、
  専用のより強いテストを持っているため。
- `UserChoice` は説明文にしか現れず、書き込みは一切ない。

## 7. UI契約

- BRIEF の9項目は揃っている。詳細ビューが 危険度／管理者／再起動／Update影響／復元 を
  チップで出し、新パネルが どんな人向け／現在の状態／案内後・適用後／案内方法 を `<dt>` 対で出す。
- 危険度は Rust が `"safe"` 等の機械可読トークンを送り、TS 側の `riskLabel()` が
  「低リスク」と描画する。**「安全」と言い切る表示は残っていない。**

## 実機テスト

`cargo test --lib -- --ignored` を実際に走らせ、**27件すべて通過**。
既存の降格済みAction（時計の秒、タスクビュー、ウィジェット、隠しファイル、拡張子、コンパクト表示、
チェックボックス）は今も「実UIへ反映されない」と報告し、Guidedへの降格が正しかったことを再確認。
テーマの明暗は反映される（fresh Explorer の輝度 255→25）。

apply→detect→rollback→detect も外部観測で確認:
`saved_A=(120,140), preapply_B=(310,280), applied_A=(120,140), rolled_back_B=(310,280)`。
**ロールバックが保存レイアウトAではなく適用直前のユーザー状態Bへ戻っている** —
BRIEF が要求する「既定値ではなくユーザーの元の状態へ」が実機で成立している。

## 見つけた唯一の実在の食い違い

codex が `docs/STATUS.md` に書いた「ライブラリテストは252件（通常225成功）」は実測と合わない。
実測は **229 passed / 27 ignored**。自己申告が古い。数値以外の主張は上記の通り裏が取れている。
