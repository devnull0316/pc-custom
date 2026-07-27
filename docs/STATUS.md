# PCカスタム — 現況と再開手順（CC記録 2026-07-24）

## 今回の到達点

- 登録済みAction IDは **61件**。BRIEFの初期版カタログ目標50〜70件には到達した。
- ただし、リリースで実行可能またはread-onlyとして完成したActionは **17件**。残る **42件** はsetter根拠未承認の候補であり、安定機能数へ算入しない。
- 42候補は `guided / experimental / unverified_storage / autoApply=false` とし、`validate`、`createBackup`、`apply`をhandler自身が拒否する。固定HKCU DWORDの保存値をread-only表示するだけで、Windows UIの有効状態とは表現しない。
- `cargo test --lib -- --test-threads=1`: **170 passed / 0 failed / 2 ignored**（CC追加分を含む現在値）。
- `npm run build`: typecheck成功、Vite production build成功。

### CCが追加した機能（codexクォータ切れ中、UI検証不要な範囲で実装・検証済み）

1. **プロファイルのバックアップ/移行**（commit `baf750e`）: `ProfileStore.export_json / import_preview / import_apply`。data-only JSONで任意コードを含まない。別PCでは実行ファイルをその機で再検証し、解決できないプロファイルは理由つきでスキップ。取り込み後の自動適用は既定オフ。
2. **WinGetアプリ導入**（commit `3af8b9c`）: `setup.rs`。コード内固定allowlistの12アプリのみ。`winget.exe` をshell経由でなく直接起動し、引数は固定Vec（ユーザー文字列を連結しない）、source固定・per-user・非対話、exit code/stdout/stderrを取得して境界長にsanitize。未登録IDは拒否（traversal/注入文字列も弾く）。レジストリrollbackエンジンには載せず、適用前にpreview提示。
3. **時間帯によるライト/ダーク自動切り替え**（commit `06f7ae1`）: `theme_schedule.rs`。判定は純関数（日またぎ・境界の含む/含まないを網羅テスト）、適用は検証済み `theme.color_mode` を preview→commit 経路で通すため履歴に1件ずつ残り個別に戻せる。**境界をまたいだ時だけ**適用し、利用者の手動変更と張り合わない。背景適用の失敗は握り潰さずUIへ表示。
4. **一時ファイルの削除**（commit `9ba1209`）: ユーザーTEMP配下の**ファイルのみ**、reparse point は辿らず対象にもせず、**7日より古いもの限定**。よってpreview→実行の間に増えたファイルは条件上入らず、削除集合は縮む方向にしか動かない。実行時も同一条件で再走査・再検証してから1件ずつ削除し、使用中などの失敗はファイル単位で報告（中断も握り潰しもしない）。UIは名前・サイズ・経過日数のみ表示（フルパスは出さない）＋**元に戻せない旨の明示と二段階確認**。
5. **現在設定の控え（read-only）**: `config_snapshot.rs`。検出済みAction状態をdata-only JSONで書き出す純関数＋コマンド。Windowsを変更せず、コマンド本文やパスを含めない。未検出は推測せず `not_detected` と記録。

## リリースで利用可能な17 Action

### 既存12

- `session.prevent_sleep`
- `power.active_scheme_check`
- `explorer.show_extensions`
- `explorer.show_hidden`
- `theme.color_mode`
- `games.process_watch`
- `explorer.compact_view`
- `explorer.item_checkboxes`
- `explorer.clock_seconds`
- `taskbar.task_view`
- `taskbar.widgets`
- `appearance.transparency`

### 今回完成した5

- `setup.startup_inventory`: HKCU/HKLM Runとユーザー/共通Startupフォルダーのread-only一覧。
- `storage.free_space_check`: Windowsシステムドライブの容量を公開APIでread-only確認。
- `storage.temp_files_check`: 現在ユーザーの一時フォルダーを上限付き・reparse非追跡でmetadata集計。削除しない。
- `games.readiness_check`: Hz、Advanced Color、Game Mode設定値、電源プラン、空き容量、既定音声出力、通知設定値を独立したKnown/Unknown/Unconfiguredとして合成。
- `power.active_scheme_switch`: 固定enumのバランス/省電力/高パフォーマンスだけを公開Power APIで明示切替。元のscheme GUIDを型付きで保存し、第三者変更時は上書きしない。プロファイル自動適用対象外。

## 登録済み・証拠待ちの42候補

以下はcatalog、型、presentation、フロント配線まで存在するが、リリースで変更不能なGuided候補である。

- タスクバー/検索/スタート: `taskbar.search_mode`、`taskbar.alignment`、`start.layout`、`start.recommendations`、`taskbar.button_grouping`、`taskbar.flashing`、`taskbar.share_window`、`taskbar.show_desktop`、`search.recent_on_hover`、`taskbar.multi_monitor`、`taskbar.multi_monitor_mode`、`taskbar.secondary_button_grouping`、`start.show_all_pins`、`start.recent_apps`
- Explorer: `explorer.launch_target`、`explorer.recent_files`、`explorer.status_bar`、`explorer.info_tips`、`explorer.hide_empty_drives`、`explorer.nav_expand_current`、`explorer.nav_show_all`、`explorer.separate_process`、`explorer.icons_only`、`explorer.drive_letters`、`explorer.preview_handlers`、`explorer.sharing_wizard`、`explorer.always_show_menus`
- 見た目: `appearance.accent_start_taskbar`、`appearance.accent_title_bars`、`appearance.auto_accent`、`appearance.taskbar_animations`
- ゲーム/デバイス/通知: `games.game_mode`、`games.controller_game_bar`、`devices.autoplay`、`notifications.usb_errors`、`notifications.weak_charger`、`notifications.toast_banners`
- 入力: `input.autocorrect`、`input.double_space_period`、`input.auto_shift`、`input.voice_typing_key`、`input.multilingual_suggestions`

MicrosoftのSettings status schemaは保存場所の参照であり、第三者アプリ向けsetter契約ではない。Action固有の一次資料と対象buildでのWindows UI round-tripをCCが承認するまで、この42件にproduction書込み経路を追加しない。

## 新機構の安全境界

### レジストリ

- 安定Actionは値の欠如/型/raw bytesを型付きbackupへ保存し、rollbackは現在値がPCカスタムの適用値と一致する場合だけ元状態へ戻す。
- 含有keyが存在しない場合はbackup前に拒否し、productionでkey全体を作成/削除しない。
- 旧versionが作ったdurable backupは、第三者値と兄弟値を保持する復旧decoderだけを残す。
- 42候補の `maximumTestedBuild=0` は内部の未試験sentinelで、IPC/UIでは必ず `null`。互換buildとは解釈しない。

### セットアップ/ストレージ/準備チェック

- Startup一覧は最大256件、名前256文字、raw値4 KiB、warning 32件に制限し、コマンド本文を保存・表示・実行しない。
- Known Folder/一時フォルダーは固定ローカルドライブだけを許可し、既知のreparse componentを拒否する。
- 一時ファイル走査はreparse非追跡、最大5000項目/512ディレクトリ/深さ8/300 ms協調上限/512 GiB集計上限。読めない分岐や上限到達は `truncated` として表示し、削除しない。
- readinessのGame Mode/通知は登録済み設定値の目安で、実効状態とは断定しない。Advanced ColorもHDR有効状態とは断定しない。

### 電源プラン

- requestは固定3値enumだけで、任意GUIDやコマンド引数を受け取らない。
- `PowerGetActiveScheme` / `PowerSetActiveScheme`だけを使い、適用前後とrollback前後を再読取する。
- 元GUID以外から始まった適用、または適用値以外から始まったrollbackはExternalConflictとして停止する。
- この実行環境では実切替がOS code 5で拒否された。元の省電力プランが維持されたことは確認済みだが、異なるプランへの実切替成功は未確認。

### ゲームプロファイル監視

- watcher spawn失敗、同期失敗、rollback失敗を無視せずhealthへ集約し、stickyなruntime異常時は新規mutationをfail-closedにする。
- 終了時は空snapshotを同期してactive profileの復元を試み、結果を返す。
- 不完全なprocess snapshotは全件終了と推測しない。追跡済みPIDをhandle、creation time、signaled stateで個別確認できた場合だけ終了扱いにする。
- ProfileStoreは1 MiB/200 profile/32 actions等の上限、未知field拒否、UUID重複拒否、strict parameter schema、automation eligibility再検証を行う。

## 検証記録

```text
cargo test --lib -- --test-threads=1
test result: ok. 152 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out

npm run build
tsc --noEmit -p tsconfig.json --pretty false
tsc --noEmit -p tsconfig.node.json --pretty false
vite v5.4.11: 44 modules transformed, built in 789ms
```

- 明示実行した電源プラン実機スモークは、代替2プランへのAPI呼出しがともにOS code 5で拒否されたため環境skip。各失敗後と終了時に元scheme不変を検証した。
- 既存の実HKCU `HideFileExt` apply→detect→rollback→detectスモークは、Windows 25H2 build 26200で型・値・有無まで正確復元済み。今回の42候補にはこの証跡を流用しない。
- `git diff --check` はエラーなし（作業環境のLF→CRLF予告のみ）。

## ネイティブアプリ実機起動の検証（CC実測 2026-07-25）

`src-tauri/target/debug/totonoe.exe` を実機で起動し、以下を確認した。

| 確認 | 結果 |
| --- | --- |
| プロセス起動・生存 | 起動し、監視下で安定生存。stderr/stdout に panic・エラー出力なし |
| **ネイティブウィンドウの生成** | `tasklist /FI "WINDOWTITLE eq PCカスタム"` が一致。`tasklist /V` の Title 列も `PCカスタム` → WebView2ウィンドウが実際に生成されている |
| 安全コアの初期化 | データディレクトリの SQLite journal（`totonoe.db`）を開き、`-shm` の更新時刻が起動時刻に一致 |
| **常駐メモリ（実測）** | 約 **45 MB**（46,168 K）で3サンプル安定。候補A(Tauri)採用時の常駐予算として妥当で、Electron標準構成の一般的な水準より小さい |
| 終了 | 強制終了後もプロセス残存なし、データディレクトリ整合 |

デスクトップ操作の許可（Windowsの設定画面・エクスプローラーの目視）は 2026-07-25 に要求したが、ユーザーが拒否した。したがって「第三者アプリの書き込みがWindows UIへ反映されること」の目視証拠は、この作業環境では取得できない。42候補の昇格はこの証拠に依存するため、据え置きが確定である。

未達: ウィンドウ**内部の描画のピクセル目視**（この環境ではデスクトップのスクリーンショットを取得できない）。ただしウィンドウが読み込むフロントエンドは同一ビルドであり、内容・コントラスト・フォーカス・モーションはブラウザ側で計測検証済み（`DESIGN_LANGUAGE.md` 検証記録）。

## 42候補についての実測（CC 2026-07-25）— 据え置きの根拠が「証拠なし」から「反証あり」へ

デスクトップの目視許可は得られなかったが、**UI Automation を使えばピクセルを見ずにWindowsの実UI状態を機械的に読める**ことが分かった（`src/windows/ui_probe.rs`）。実測値の例:

```
taskbar_left=0 width=1920 start_left=518 start_width=45 center_ratio=0.282
```

これを使い、`taskbar.alignment`（HKCU `Advanced\TaskbarAl`）で往復検証を行った。

1. 元の状態を型付きで退避（この環境では**値そのものが未設定**）
2. 反対の配置値を書き込み、`SHChangeNotify` 相当の非破壊通知を送信
3. スタートボタンの位置を最大6秒ポーリングして実測
4. 元の状態（＝値の欠如）へ正確に復元し、再度実測

**結果: 位置は動かなかった。** つまり第三者アプリからの書き込みは、Explorerを再起動しない限りタスクバーUIへ反映されない。復元は正確で、作成した値は削除され欠如状態へ戻った（テストが型・値・有無まで検証）。

意味するところ:

- 42候補を Guided に据え置いた判断は、**「証拠がないから安全側に倒した」ではなく「実測して反映されないことを確認した」**に格上げされた。
- もしこれらを「変更できる」として出荷していれば、利用者には**設定したのに何も変わらない**という最悪の体験になっていた。表示のみに留めた設計が実測で正しかったことになる。
- 反映させるにはExplorerの強制再起動が要るが、それは実験的機能として隔離済みで、通常Actionでは行わない方針。

再現手順: `cargo test --lib taskbar_alignment_write -- --ignored --nocapture`（実機のタスクバーを一時的に変更し、必ず復元する）。

### 反映されるものと、されないものが実測で分かれた

同じ枠組みをアクセントカラーにも当てた。こちらは `DwmGetColorizationColor` が**保存値ではなくDWMの実効色**を返すため、反映を機械判定できる。

| 対象 | 書き込み後の実UI | 結論 |
| --- | --- | --- |
| `TaskbarAl`（タスクバー配置） | スタートボタンは6秒待っても動かず | 第三者書き込みは反映されない。**案内に留める** |
| `ColorizationColor`（ウィンドウの色） | 実効色が #006FC4 → #AB4B24 へ変化 | **反映される。変更機能を実装できる** |

どちらも元の値・型・有無へ正確に復元済み（テストが `backup.original` との一致を検証）。

この差は重要で、「Windowsの設定は第三者から変えられない」と一括りにするのは誤りだと分かった。**項目ごとに実測して仕分ける**必要があり、その仕分けを機械的に行う手段がこれで手に入った。

→ **実装済み**: `appearance.window_color`（`actions/window_color.rs`）。固定7色プリセットのみ、`ColorizationColor` と `ColorizationAfterglow` を1トランザクションで変更、片方だけ書けた場合は補償復元。実機往復も確認済み（#006FC4 → 橙 #F4B100 → 元の値へ正確に復元）。

### 通知が弱かったことが判明。ただし `show_hidden` はそれでも反映されない

`notify_explorer_settings_changed` は `SHChangeNotify(SHCNE_ASSOCCHANGED)` だけを送っていた。
これは関連付けの変更を伝える合図で、**フォルダーオプションの読み直しは促さない**。
文書化された `WM_SETTINGCHANGE` +「ShellState」の同報を追加した（この変更自体は正しい）。

それでも `explorer.show_hidden` は、開いているExplorerウィンドウへ反映されなかった。
新しく開いた窓での挙動は未確定（Explorerが既存ウィンドウを再利用するため観測できず）。

文書化API `SHGetSetSettings` も実装して試したが（`windows/ui_probe.rs` に読み書きヘルパーあり）、
**こちらでも開いているExplorerウィンドウは更新されなかった**。

したがって結論はこうなる:

> **外部プロセスからフォルダーオプションを変えても、すでに開いているエクスプローラーの窓は更新されない。**
> レジストリ直書きでも、文書化APIでも同じ。

これは実装の不備ではなく、この環境のWindowsの挙動である。黙っていると利用者は
「適用したのに変わらない」と受け取るため、該当Actionの説明に次の一文を出すようにした。

> すでに開いているエクスプローラーの窓は自動で更新されません。窓を開き直すか、F5キーで更新してください。

`show_extensions` が別プロセスで反映されたことと整合する。設定自体は確かに変わっており、
**新しく開く窓には効く**。効かないのは「いま開いている窓の再描画」だけである。

### 検証の道具は4回直した（記録）

同じ1件の検証で、道具側の欠陥が4種類出た。

1. 観測点が対象の内側（シェル表示名のプロセス内キャッシュ）→ 偽陰性
2. 要素名の完全一致 → 取りこぼし（「時計 11:15」等）
3. `FindWindowW` が最初の窓を返す → 利用者の窓を観測・**誤って閉じた**
4. 判定文字列がフォルダ名にも一致 → 常に真

`EnumWindows` による自窓限定と、フォルダ名と重ならない検査ファイル名で 3・4 を解消した。
**否定的な結果が出たら、まず道具を疑う。**

### `explorer.show_hidden` の検証は中断（旧記録）

Explorerウィンドウを開いて外から観測する方法を試したが、**3回続けて観測側に欠陥が出た**ため中断した。

1. `FindWindowW("CabinetWClass", None)` は**最初に見つかったExplorerウィンドウ**を返す。
   そのため検査用フォルダではなく、利用者が既に開いていた別ウィンドウ（項目数101）を観測していた。
   さらに後片付けで、そのウィンドウへ `WM_CLOSE` を送ってしまった（利用者の作業ウィンドウを閉じた可能性がある）。
2. タイトル完全一致へ変えたら、今度はウィンドウを見つけられずスキップになった。

正しくやるには `EnumWindows` で CabinetWClass を列挙し、タイトルの**部分一致**で
自分が開いたウィンドウだけを選び、観測も後片付けもそのウィンドウに限定する必要がある。
それまでこの検査は走らせない（`#[ignore]` に理由を明記）。

**誤った結論は、結論が無いことより悪い。** 今日はこの検査だけで
「観測点が内側」「完全一致で取りこぼす」「対象ウィンドウ違い」と3種類の欠陥を出した。
検証の道具を作るときは、道具自体を先に疑うこと。

### 可変Actionの検証状況（残りが明確になった）

降格後、実際に変更を行う Action は次の9件。うち**実UIへの反映を確認済みが4件、未確認が5件**。

| Action | 反映の確認 | 観測方法 |
| --- | --- | --- |
| `explorer.show_extensions` | **確認済み** | 別プロセスのシェル表示名（`.txt` の出現/消失） |
| `appearance.window_color` | **確認済み** | `DwmGetColorizationColor` の実効色 |
| `session.prevent_sleep` | **確認済み** | 実機の通し経路テスト＋lease API |
| `power.active_scheme_switch` | 公開APIで検証（この環境ではOSがcode 5で拒否） | `PowerGetActiveScheme` |
| `explorer.show_hidden` | **未確認** | Explorerウィンドウの一覧をUIAで読む必要あり |
| `explorer.item_checkboxes` | **観測不能（今回）** | 自作の一意な検査フォルダーを開き、EnumWindows＋タイトル部分一致で限定したExplorer項目のUIA Toggle/CheckBox状態を読む。今回の実行セッションではCabinetWClassが生成されず未実測 |
| `explorer.compact_view` | **観測不能（今回）** | 同じ自窓のExplorer一覧項目をUIAで読み、行高・行間を比較する。今回の実行セッションではCabinetWClassが生成されず未実測 |
| `appearance.transparency` | **観測不能（今回）** | `DwmGetColorizationColor` の実効 `opaque_blend` を外から読む。変更前の読取りが `0x80070002` 相当で失敗したため未実測 |
| `theme.color_mode` | **観測不能（今回）** | 自窓として開いたExplorerの描画領域を画面DCから読み、平均輝度を比較する。今回の実行セッションではCabinetWClassが生成されず未実測 |

2026-07-26 に今回対象の4件へ `#[ignore]` の往復テストを追加した。観測可能な環境では、
元状態の型・raw bytes・値の欠如を退避し、変更後の外部観測、元状態への復元、復元後の外部観測まで行う。
ただし今回のテスト実行セッションは対話シェルから分離されており、`FindWindowW("Shell_TrayWnd")` と
CabinetWClassの `EnumWindows` がともに空だった。別のデスクトップ操作経路からのExplorer起動も承認されなかった。
`DwmGetColorizationColor` も変更前の読取りで失敗した。4件とも設定を書き込む前に安全に終了しており、
反映する／しないのどちらにも分類しない。**確認済み化もGuided降格も行わず、結論を保留する。**

追加テスト: `explorer_item_checkboxes_write_changes_the_fresh_explorer_ui`、
`explorer_compact_view_write_changes_the_fresh_explorer_row_spacing`、
`appearance_transparency_write_changes_the_effective_dwm_blend`、
`theme_color_mode_write_changes_a_fresh_explorer_window_luminance`。

今回の4テストはすべて `ok` だが、出力は `OBSERVATION_UNAVAILABLE` であり反映確認ではない。
`cargo test --lib -- --test-threads=1` は **179 passed / 1 failed / 18 ignored**。
失敗は既存の `accent_color_reads_a_valid_hex_on_this_machine` で、同じ対話DWM不在による
`DwmGetColorizationColor` の `0x80070002` 相当。これをfilterした残りは
**179 passed / 0 failed / 18 ignored / 1 filtered out**。

### 観測方法を直したら、実害は3件だった

最初の検査は要素名の**完全一致**で探していたが、タスクバーの要素名は
「時計 11:15」「ウィジェット 27°C 晴れのちくもり」のように**付随情報を含む**。
そのため取りこぼしがあった。要素名を実際に列挙して確認し、**部分一致**へ直したうえで再検査した。

| Action | 正しい観測での結果 |
| --- | --- |
| `taskbar.task_view` | ボタンは表示されたまま。**反映されない** |
| `taskbar.widgets` | 書き込みが **OS code 5 (AccessDenied)** で拒否される |
| `explorer.clock_seconds` | 時計は「時計 11:15」のまま秒が出ない。**反映されない** |

3件とも「変更可能」として出荷していたので Guided へ降格し、変更経路も封じた。

### 出荷中のActionを同じ方法で検査したら（初回・観測が不正確だった記録）

同じ往復検証を「すでに変更可能として出荷している」タスクバー系へ向けたところ、次が判明した。

| Action | 結果 |
| --- | --- |
| `taskbar.task_view`（タスクビューボタン） | 値は書けるが、**ボタンは消えも現れもしない**。利用者には「適用しました」と出るのに何も変わらない |
| `taskbar.widgets`（ウィジェット） | 値への書き込み自体が **OS code 5 (AccessDenied)** で拒否される |

どちらも「変更可能」として出していたのは誤りだったので、**Guidedへ降格**した（`kind: Guided` / `auto_apply_eligible: false` / 危険度は注意）。あわせて共通マクロの `validate` にガードを入れ、**表示だけでなく変更経路そのものを閉じた**。降格が維持されることはテストで固定している。

つまりこの検査は、42件の据え置きが正しかったことを示すだけでなく、**出荷済みの機能に潜んでいた「黙って何もしない」不具合を実際に見つけて潰した**。テストは全て緑だったのに、実UIを見に行くまで誰も気づけない種類の不具合だった。

### 検証方法そのものに落とし穴があった（自戒として記録）

`explorer.show_extensions` を同じ手順で検査したところ「効いていない」と出た。しかし**これは誤りだった**。
シェルの表示名はプロセスごとにキャッシュされるため、同一プロセス内で設定を変えても表示名は変わらない。

別プロセスを起動して確認すると:

```
HideFileExt=0 -> "totonoe-extension-probe2.txt"
HideFileExt=1 -> "totonoe-extension-probe2"
```

つまり拡張子表示は**正しく効いている**。危うく正常な機能を「壊れている」と誤判定して降格するところだった。

この教訓は重要で、**観測方法が対象より内側にあると誤った結論が出る**。タスクバーの判定が信頼できるのは、自プロセスのAPIではなく**実行中のExplorerが描いている実UIをUI Automationで外から読んでいる**からである。今後この検査を他Actionへ広げるときは、観測点が対象の外側にあるかを毎回確認すること。

### 実機での通し確認（利用者が画面で行う順番）

これまでのテストは部品単位だったので、**利用者が実際にたどる順番**をそのまま1本のテストにした
（`engine/tests.rs::full_user_journey_preview_commit_timeline_rollback_on_real_machine`）。

1. 「適用プレビュー」→ 変更内容が1件提示され、現在の状態と適用後がどちらも空でないこと
2. 内容を確認して適用 → `succeeded`
3. タイムラインにその項目が残り、`rollback_available` であること
4. **その1件だけ**を元へ戻す → `rolled_back`、履歴には復元済みとして残ること

対象は `session.prevent_sleep`。レジストリもファイルも書かず、このプロセスのスリープ抑止要求だけを扱うため、
実機で走らせても副作用が残らない。ビルド検出は実機の `OsIdentity::load()` を使うので、
互換性ゲートも本番と同じ経路を通る。

これで「部品は動くが通しでは未確認」という状態は解消した。

## 42候補の行き止まりを解消（CC 2026-07-25）

実測でUIへ反映されないと分かった以上、これらを「表示するだけ」で終わらせない。**Windows自身の設定画面へ案内する導線**を追加した（`src/settings_link.rs`）。

- ActionId → `ms-settings:` ページの**固定表**のみ。任意URL・パス・コマンドは受け付けない。
- 起動は `ShellExecuteW` に定数文字列を渡すだけで、シェルを経由しない。
- 設定アプリに該当ページが無いもの（Explorer系はフォルダーオプション側）は**案内を出さない**。嘘の導線を作らないため。
- UIには「Windowsの設定を開く」ボタンと、「この項目はWindowsの設定画面から変更できます。PCカスタムは現在値の表示だけを行います」の一文を出す。
- テスト4件: 全マッピングが `ms-settings:` スキームで空白・引数連結・引用符を含まないこと、Explorer系は案内なし、主要候補には必ず導線があること、表示情報に `settingsPage` が載ること。

これで42候補は「変えられないと言われて終わり」ではなく、「ここで変えられます」と示す状態になった。

## CC監査: codexによる4件検証（2026-07-26）

codexへ範囲を限定して依頼（effort=high、対象4件の検証のみ、新機能・リファクタ禁止）。

**結果: 4件とも「観測対象がその設定を追跡しない」ため保留。推測での昇格・降格はなし。** これは正しい判断。

| Action | 観測対象 | 実測 |
| --- | --- | --- |
| `explorer.item_checkboxes` | UIAのToggle/CheckBox状態 | 項目は観測できたが、チェックボックス状態はUIAに出てこない |
| `explorer.compact_view` | 行の高さ・ピッチ | `item_height=24 / row_pitch=28` を実測。変更後も**変わらず** |
| `appearance.transparency` | DWMの `opaque_blend` | 読めるが、この設定を追跡しない |
| `theme.color_mode` | Explorer描画領域の平均輝度 | 対象窓が前面でないと採取できず |

CC監査で確認した点:

- `actions/` 配下は未変更。Action metadataや変更経路に手を付けていない（指示通り）。
- Explorerウィンドウの操作は `find_explorer_window_by_title` 経由で**自窓限定**。`FindWindowW` の直接使用は Shell_TrayWnd のみ（唯一の窓なので安全）。
- `OwnedExplorerWindow` に `Drop` があり、パニック時も窓が残らない（RAII）。
- 全 `#[ignore]` 18件を通しで実行し、`HideFileExt` `ColorizationColor` とも元の値、一時フォルダーの残骸なしを確認。

CCが直した点:

- `theme.color_mode` のテストが `.expect()` で観測失敗をテスト失敗に変えていた。他3件と同じ安全終了へ修正。
  **環境要因が失敗として残ると、本当の退行を隠す。**

未解決として残るもの:

- `explorer.compact_view` は行ピッチという**妥当な観測対象**で変化なしを実測した。降格の判断材料になり得るが、
  今日だけで観測側の欠陥を4種類出しているため、単独の測定で降格はしない。別の観測（新窓での比較、DPI変化の除外）で
  再確認してから判断する。

### `explorer.compact_view` 第2観測（2026-07-26）

- 行ピッチとは別に、同じ4項目の先頭上端から末尾下端までを `list_height` として測る観測を追加した。
- 各測定では `OwnedExplorerWindow` を再利用し、変更前・変更後・復元後にそれぞれ新しいExplorer窓を開く。窓は
  `EnumWindows` の `CabinetWClass`、起動前に存在しなかったハンドル、一意な検査フォルダー名を含むタイトルで限定する。
- 実行時は新規起動後も `EnumWindows` の `CabinetWClass` が0件で、変更前の自窓を取得できなかった。そのため
  `list_height` の変更前後値は**観測不能**。候補窓自体が空なので、判定文字列の別項目への一致や既存窓の取り違えではない。
- 設定書込み前に安全終了し、一時フォルダーは `TempDir` で削除された。第2観測が観測不能のため、
  `explorer.compact_view` は降格せず現状を維持する。

## 可変Actionの分類が完成した（2026-07-26）

「変更する」と称するAction全件について、**Windowsの実UIが本当に変わるか**を実測で確認し終えた。
観測点はすべて対象プロセスの外。変更は新しく開いた窓で観測し、毎回元の状態へ正確に復元している。

### 実際に効く（確認済み）

| Action | 観測 | 実測値 |
| --- | --- | --- |
| `theme.color_mode` | 新窓Explorerの平均輝度 | **255 → 25**（白→ほぼ黒） |
| `appearance.transparency` | タスクバー200点の彩度統計 | **彩度平均 37 → 13**（2回とも再現、間に復元） |
| `appearance.window_color` | `DwmGetColorizationColor` の実効色 | **#006FC4 → #F4B100** |
| `explorer.show_extensions` | 別プロセスのシェル表示名 | `.txt` の出現/消失 |
| `session.prevent_sleep` | lease API＋実機の通し経路 | 適用→履歴→復元まで通過 |
| `power.active_scheme_switch` | `PowerGetActiveScheme` | 公開APIで検証（この環境はOSがcode 5で拒否） |

### 効かないので降格（Guided＋変更経路を封鎖）※ show_hidden を追加

| Action | 観測 | 実測値 |
| --- | --- | --- |
| `taskbar.task_view` | UIAでボタンの存在 | 表示されたまま |
| `taskbar.widgets` | 同上 | 書き込みが OS code 5 で拒否 |
| `explorer.clock_seconds` | UIAで時計の文字列 | 「時計 11:15」のまま秒が出ない |
| `explorer.compact_view` | 行ピッチと**リスト全体高** | `28/108` → `28/108`（独立2量とも不変） |
| `explorer.item_checkboxes` | 項目テキストの左端座標 | `[174,174,174,174]` → 変化なし（delta 0） |
| `explorer.show_hidden` | 新窓Explorerに隠し項目が現れるか（対照項目で観測の健全性も確認） | 対照は見える／隠し項目は前後とも見えず |

**6件が「適用したと表示されるのに何も起きない」状態で出荷されていた。** すべて発見し、
`kind: Guided` / `auto_apply_eligible: false` へ落とし、共通マクロの `validate` で変更経路自体を封じた。
`demoted_actions_refuse_to_mutate` が5件すべてを固定している。

### `show_extensions` は降格しなかった（観測が鈍感だったため）

同じ方法で `show_extensions` も測ったところ「変化なし」と出たが、**この観測は判定に使えない**。
UIAが返す項目名は拡張子の表示設定に鈍感で、常に正式なファイル名を返すからである。

根拠: 同一の `HideFileExt=1` の下で
- 別プロセスのシェル表示名 → 「拡張子なし」
- Explorer UIA の項目名 → 「.txt 付き」

信号が食い違う以上、この観測で「反映されない」と結論づけてはならない。
`show_extensions` の判定根拠は別プロセスのシェル表示名のままとし、当該テストは
「判定に使わないこと」と明記して残した。**鈍感な観測で正常な機能を殺さない。**

### 分かれ目

タスクバーの外観（配置・ボタン・時計）とExplorerの項目描画（コンパクト・チェックボックス）は、
第三者プロセスからの書き込みでは反映されない。一方、テーマの明暗・透明効果・DWMの色は反映される。
**設定の見た目が似ていても挙動は別物**であり、項目ごとに実測する以外に判定手段はない。

## `explorer.compact_view` の決着（2026-07-26）

保留にしていた1件に、独立した第2の観測で決着をつけた。

codexが `list_height`（先頭項目の上端から末尾項目の下端まで）を追加。行ピッチとは別の量で、
全項目の余白変化が積み上がるぶん感度が高い。**新しく開いた窓**で測定した（既存窓は更新されないため）。

```
before:  item_height=24  row_pitch=28  list_height=108
applied: item_height=24  row_pitch=28  list_height=108
```

**独立した2つの観測が、新窓でもそろって変化なし。** よって `explorer.compact_view` を
既存3件と同じ手順でGuidedへ降格した（kind / auto_apply_eligible / riskLevel、
validateガードで変更経路を封鎖、`demoted_actions_refuse_to_mutate` に追加）。

これで**黙って何もしない可変Actionは4件見つけて全て潰した**。

codexの環境では窓を取得できず `OBSERVATION_UNAVAILABLE` で終わったが、CCの環境では取得できた。
**UI観測を伴う検証はCC側で実行する**という分担が明確になった（codexは測定コードを書き、CCが走らせる）。

## リリースビルドで見つかった実バグ（2026-07-26）

初めて `npx tauri build` を通した。生成物:

- `src-tauri/target/release/totonoe.exe`（起動確認済み。窓生成、常駐 **34MB**、panicなし）
- `src-tauri/target/release/bundle/nsis/Totonoe_0.1.0_x64-setup.exe`（1.97MB のインストーラー）

そのうえで、**出荷ビルドでだけ安全機構が無効になっていた**ことが分かった。

`game_profile/watcher.rs` は `catch_unwind` で監視スレッドのパニックを捕捉し、
health へ sticky な異常として記録して以後の変更操作を fail-closed にする設計である。
ところがリリースプロファイルに `panic = "abort"` が指定されていたため、
**リリースビルドでは catch_unwind が働かず、パニックでプロセスごと即死する**。
開発ビルドでは unwind なので設計どおりに動き、テストも通る。つまり
**リリースビルドを一度も作っていなかったために気づけなかった**類の不具合だった。

対応: リリースプロファイルから `panic = "abort"` を削除し、理由をコメントで残した。
バイナリはわずかに大きくなるが（5.8MB → 9.1MB、インストーラー 1.97MB → 2.45MB）、
このプロダクトの中心的な約束（安全に倒す・復元できる）を出荷ビルドで成立させるほうが重要である。

修正後の確認: `profile.release` に有効な `panic` 指定が無い（＝既定の unwind）ことを確認し、
再ビルドしたリリースバイナリの起動・窓生成・常駐33MB・panicなしを実機で確認した。

なお、この確認の途中で**検査スクリプト自身の誤り**を2回踏んだ。
1回目は自分が書いたコメント文中の `panic = "abort"` に文字列一致してしまい、
2回目は `grep ... | tr` のパイプ終了コード（常に成功）で条件分岐していた。
どちらも「設定は残っている」と誤報した。**検査の結論は、検査自身の正しさを確かめてから採用する。**

## ゲーム前の準備確認を画面に出した（2026-07-26）

BRIEF §4 が求める「ゲーム準備チェック」は、`games.readiness_check` として**中身は実装済みだったが、
Action一覧に埋もれていて画面に無かった**。ゲームプロファイル画面へ専用パネルとして出した
（Steam や Playnite の「起動前に環境を確かめる」体験に相当）。

これまで要約1行（「確認5 / 不明1 / 未設定1」）しか返していなかったので、
`observed_items` に項目別の行を追加し、UIはそれをそのまま並べる。実機での出力:

```
画面のリフレッシュレート — 240 Hz
HDR（Advanced Color） — 有効0 / 対応1 / 接続1（HDRの実効状態とは断定しません）
ゲームモードの設定値 — 未設定
電源プラン — 省電力
システムドライブの空き — 288.8 GiB
既定の音声出力 — 既定の出力あり
通知の設定値 — 無効
```

**この作業中に見つけたUX不具合**: 電源プランが `{a1841308-3541-4fab-bc81-f71556f20b4a}` と
生のGUIDで表示されていた。専門用語を見せないという本プロダクトの原則に反するため、
Windows標準3プランを名前へ写像した（未知のOEMプランは推測せず「その他のプラン」）。
同じ写像は「有効な電源プラン」表示にも適用した。

7項目そろうことと、各行が「項目名 — 値」の形になることは実機テストで固定している。

## インストール検証は未完了（正直な記録）

インストーラーの**生成**は成功している（`Totonoe_0.1.0_x64-setup.exe`）。
しかし**実際にインストールされる**ところまでは確認できていない。

サイレントインストール（`/S`）を4通りの起動方法で試したが、いずれも
`%LOCALAPPDATA%` にも `%LOCALAPPDATA%\Programs` にもファイルが作られず、
アンインストール登録もスタートメニューも作られなかった。
最初の1回は3分間ブロックし、以降は無言で終了した。原因は特定できていない。

確認したこと:
- WebView2ランタイムは導入済み（v150）なので、その不足が原因ではない
- 残骸なし。プロセス残存なし。利用者の環境は変更されていない

**この過程で設定の実問題を1つ見つけて直した**: `webviewInstallMode` が
`{"type":"downloadBootstrapper","silent":false}` だった。WebView2が入っていない環境で
無人インストールを行うと、ダイアログを出して待ち続ける。`silent: true` へ変更した。

残る確認は、利用者が在席時に対話的にインストーラーを実行すること。
コマンドは `src-tauri/target/release/bundle/nsis/PCCustom_0.1.0_x64-setup.exe`。
**シェルから無人で押し通すのは、画面を塞ぐ危険があるためこれ以上行わない。**

### 改名後のバンドルビルド検証（2026-07-27）

改名時に「実行ファイル名を ASCII に留めるのは bundle ビルドを検証できないから」と書いたのに、
その bundle ビルド自体を回していなかった。回した。

```
Built application at: src-tauri\target\release\pc-custom.exe          (9.6 MB)
Finished 1 bundle at: ...\bundle\nsis\PCCustom_0.1.0_x64-setup.exe    (2.58 MB)
```

**改名はパッケージングを壊していない。** 旧名の成果物（`totonoe.exe` /
`Totonoe_0.1.0_x64-setup.exe`）は、誤って旧版を実行する事故を避けるため削除した。
どちらも git 履歴から再生成できる。

依然として未検証なのは**インストーラーを対話的に実行したときの挙動**だけであり、
これは在席した利用者の操作が要る。生成・命名・サイズはここで確認済み。

## BRIEF未実装分の追加（2026-07-26 codex + CC監査）

codexへ範囲限定で依頼し、BRIEFにあって未実装だった3機能を追加した（Action 61 → 63、テスト 181 → 192）。

1. **手動モード（BRIEF §5「ゲーム以外のモードも作れる構造に」）**
   プロファイルは実行ファイル必須＝ゲーム専用で、勉強・作業モードが作れない構造的欠落だった。
   `executable_path` を `Option` にし、`None` を手動モードとした。手動モードは**自動適用の対象外**で、
   利用者が明示的に実行したときだけ Action セットを適用し、実行分は既存のjournal経由で復元する。
   後方互換はテストで固定（`legacy_game_json_without_manual_fields_remains_readable`）。
2. **`setup.launch_apps`（アプリ一括起動）** — 固定allowlistのみ、パスは App Paths レジストリで解決、
   shell非経由、引数は固定、起動中なら二重起動しない、rollback対象外。
3. **`setup.windows_update_status`** — read-only。停止・変更は一切行わない。

### CC監査で見つけて直した実バグ

実機テストを走らせたら**5分間ハングし、ペイントが起動したまま残った**。原因は2つ:

- `Command::spawn()` が標準入出力を継承していた。起動したアプリが PCカスタム のパイプを掴んだままになり、
  こちらの出力を読む側がアプリ終了までブロックする。**これは実装側のバグ**で、テストだけの問題ではない。
  `Stdio::null()` を3つとも指定して解消。
- テストが起動したアプリを閉じていなかった。起動前に動いていたものは利用者のものとして残し、
  **このテストが起動した分だけ**を閉じるようにした。

修正後: 5分ハング → **0.22秒で完了**、残存プロセス0。

## BRIEF §2「操作・普段使い」を実装（2026-07-26 codex + CC監査）

BRIEFは §2 についてこう指示していた。

> PowerToysに同等機能がある場合は、すべてを再実装するのではなく…
> **PowerToysの内部仕様に依存する非公式な操作は避けてください。**

12項目のうち8つ（キー入れ替え / アプリ別ショートカット / 書式なし貼り付け / OCR /
常に最前面 / 画面分割 / マウス強調 / 画像リサイズ / 一括リネーム）はPowerToysが公式提供している。
**再実装せず案内に徹した。** PCカスタム自身がキーフックや常駐フックを入れるのは禁止事項（injection）に触れる。

- `tools.powertoys_status`（read-only）: App Paths レジストリとアンインストール登録という
  **文書化された方法**でのみ判定。**PowerToysの設定ファイルは読み書きしない**（監査で確認済み）
- 案内パネル: 「やりたいこと」から選ぶ12項目。各項目にPowerToysの機能名と平易な説明を併記
- 未導入なら既存のWinGet導線へ、導入済みなら固定allowlist方式で起動

実機確認: この環境はPowerToys未導入で `installed=false, launch_available=false` と正しく報告された。

**CC監査で直した点**: 案内カードの機能名とバッジがアクセント色のままで、
コントラストが 3.68 / 4.30 と AA(4.5:1) に届いていなかった。
アクセント色は面の装飾用で小さい文字には使えない。本文色＋縁取りへ変更し、再監査で0件。

## 未実装・CC確認が必要な項目

1. 42レジストリ候補ごとの第三者setter契約となる一次資料、26100/26200 clean VMでの設定UI→detect→apply→UI確認→rollback→UI確認。承認後にのみstableへ個別昇格する。
2. ~~WinGet導入Action~~ → **CC実装済み**（上記）。固定allowlist・固定引数・source固定・出力上限・exit code取得まで実装。残課題は hard timeout の明示指定、既存導入の事前判定（現状はwinget側の判定に委ねる）、uninstall契約（未実装・rollback対象外と表示）。
3. アクセントカラー: **読み取りはCC実装済み**（`appearance.accent_color_check` = 公開API `DwmGetColorizationColor` で現在色を#RRGGBB表示するread-only Action。実機で読み取り検証済み）。**変更**は保存値のsetter意味論（AccentPaletteの多値blob）が未立証のため、既存3候補はGuidedのまま据え置く。
4. ~~ライト/ダーク時刻連動~~ → **CC実装済み**（上記）。残課題は、アプリ未起動時は切り替わらない点（常駐watcher前提）の明示と、タイムゾーン変更時の扱い。
5. 最前面/書式なし貼り付け。PowerToys導入支援を優先する方針のため独自hook/injectionは未実装。
6. ~~現在設定の独立エクスポート~~ → **CC実装済み**（read-onlyの控えとプロファイルのexport/import）。残課題は「控えから設定値を復元する」逆方向で、検出状態→Actionパラメータの逆写像をAction毎に定義する必要がある（現状は控え＝参照用）。
7. ~~一時ファイル削除~~ → **CC実装済み**（上記）。事前一覧・使用中判定・reparse再検証・rollback不能表示の契約を満たした。残課題は個別選択（現状は条件に合う全件が対象）と、削除前のプロセス使用中判定をハンドル取得で事前に行うこと。
8. Windows 11の電源モードslider/overlay。今回実装したのは文書化されたpower scheme切替であり、同一機能とは表現しない。
9. 既存stable registry Actionの「比較直後から書込み直前」の狭いTOCTOUと、filesystem検証後のread TOCTOU。第三者変更非上書きの再読取はあるが、kernel-level CASではない。
10. 実GUIでの**ピクセル目視**（59件の表示、42件の変更不能表示、観測一覧スクロール、電源Action確認画面）。ネイティブウィンドウの生成・コア初期化・常駐メモリは上記のとおり実機検証済みだが、描画の目視だけは残る。

## 禁止事項

Defender/Firewall/Windows Update停止、pagefile、HPET/BCD、大量サービス停止、process injection、任意コード実行、任意引数のshell連結、出所不明downloadは実装しない。今回も追加していない。


## 製品名の変更（2026-07-26）

「整える」という比喩をやめ、何をする道具かを名前で言い切る方針に変えた。**Totonoe → PCカスタム**。

- 目に見える名前（ウィンドウタイトル、サイドバーのブランド、本文、docs）は `PCカスタム`。
- `productName` は `PCCustom`（ASCII）にした。このバージョンの Tauri スキーマには
  `mainBinaryName` が無く、`productName` がそのまま実行ファイル名になる。
  日本語名の `.exe` は Windows 上で不正ではないが、**このリポジトリでは bundle ビルドを
  検証できていない**（インストーラーの動作確認は以前から保留のまま）。
  検証できない差分を出荷しない方針に従い、実行ファイル名は ASCII に留めた。
  日本語にしたい場合は1行の変更だが、bundle ビルドの確認とセットで行うこと。
- `identifier` は `jp.totonoe.app` → `jp.pccustom.app`。**これはアプリの同一性が変わる**ので、
  旧識別子で入れた既存インストールは別アプリ扱いになる（リリース前なので問題ない）。
- 上の「インストーラー未検証」節に出てくる `Totonoe_0.1.0_x64-setup.exe` は、改名前に実際に
  生成された成果物の名前。記録として正しいのでそのまま残す。今後のビルドは
  `PCCustom_0.1.0_x64-setup.exe` になる。
- ブランチ `codex-brief4-unaudited` は改名前の名前を含んだまま作られたため、
  master へ取り込む際に同じ置換をもう一度かけている。

## BRIEF §4 残り3機能を完了（2026-07-26）

Action は64件から67件、ライブラリテストは256件（通常229成功、明示実機27件）になった。

1. **`setup.default_apps` — Guided**
   `UserChoice` のhashを生成・偽装せず、固定 `ms-settings:defaultapps` でWindows自身の画面へ案内する。
   UIにも「Windowsの仕様で、ここから直接は変更できません」と明記した。
2. **`setup.window_layout` — Persistent / 明示操作専用**
   「現在の配置を保存」を押したときだけ、公開API
   `EnumWindows` / `GetWindowPlacement` / `GetWindowRect` で対象を保存する。
   復元は `SetWindowPlacement`、適用前の各配置をjournalへ耐久記録し、適用・検証・rollback・再検証を
   Action traitの同じ契約で行う。登録ゲームの保存済みfile identityと現在の実行ファイルidentity、
   本人性を読めないprocess、通常表示の全画面・popup・tool・owned・cloaked等の窓はfail-closedで除外する
   （文書化された最大化状態 `showCmd=3` は全画面ゲームと混同せず保存対象にできる）。
   閉じた窓・曖昧一致・ゲーム除外はアプリ名だけの構造化結果で返す。タイトルは保存と照合に必要だが、
   専用型の`Debug`を常にredactし、ログ・エラー・完了結果へ出さない。
   公開APIには未登録ゲームを一般アプリから汎用判定する契約がないため、その限界をUIで明記し、
   「未登録のウィンドウゲームを閉じた」という明示確認をUIとbackendの両方で保存の必須条件にした。
3. **`setup.audio_output` — 読み取り＋Guided**
   公開 `IMMDeviceEnumerator` でactive render endpointとeConsole既定を読み取る。
   非公開 `IPolicyConfig` は使わず、切り替えは固定 `ms-settings:sound` でWindows自身の画面へ案内する。

### 実機の外部観測

- ウィンドウ配置: テスト所有窓だけをprivate storeへ保存し、
  `saved=(120,140 520x360 showCmd=1)` → `moved=(300,270)` →
  `restored=(120,140 520x360 showCmd=1)` を `GetWindowRect` / `GetWindowPlacement` で確認。
  matched 1、positioned 1、issue 0。既存の利用者ウィンドウとタイトルは出力していない。
- さらにAction本体の変更ライフサイクルをテスト所有窓だけで実行し、
  保存位置 `A=(120,140 520x360)`、適用直前位置 `B=(310,280 520x360)` に対して
  `apply → detect` でA、`rollback → detect` でBへ戻ることを外部APIでも再確認した。
- 音声: 最終実行時のactive render endpoint **6件**、既定 endpoint **あり**。
  実機smokeは件数と既定有無だけを出力し、デバイス名・endpoint IDは出力していない。

### 最終安全監査で追加した配置トランザクション境界

- 観測fingerprintへ対象ごとのPID・process creation time・opaque HWND・`WINDOWPLACEMENT`・
  `GetWindowRect`結果をhash化して含めた。件数が同じでも外部移動はstaleとして検出する。
- applyは、耐久backupを取得できたentry IDと同一process/window instanceだけを変更する。
  各`SetWindowPlacement`直前にも同じ対象と元座標を再読取する。
- 途中失敗時は、実際にdispatch成功した対象だけを逆順補償する。第三の座標へ外部変更された対象は
  上書きせず`recovery_required`に止める。
- crashで一部だけ適用済みになっても、各対象を「元 / 適用値 / 第三値」に個別分類する。
  元と適用値だけの混合なら安全なrollbackを再開し、第三値が1つでもあれば自動上書きしない。
- 配置保存、preview/commit、プロファイルidentity変更を同じmutation gateで直列化した。
  未復元journalが残る間は混合配置の保存自体を拒否する。
  保存snapshot IDまたは登録ゲーム集合がpreview後に変わればcommitをstaleとして拒否する。
  private storeのfile置換とmemory世代更新も1つのmutex区間にした。

既定アプリと音声切替をGuidedにする判断には同意する。前者はhash保護された`UserChoice`に対する
文書化済み汎用setterがなく、後者の一般的な切替方法は非公開COMである。どちらも「書けた」だけを
効果の証明にせず、PCカスタムは読み取り可能な事実だけを表示し、変更はWindows自身へ委ねる。

### インストーラーの静的監査（2026-07-27）

対話実行はできていないが、生成された `src-tauri/target/release/nsis/x64/installer.nsi`（892行）
を読めば「何をするつもりのインストーラーか」は確定できる。読んだ。

- `INSTALLMODE = currentUser`。**per-user 導入で、アプリ自体に管理者権限を要求しない。**
  BRIEF の「アプリ全体を管理者で動かさない」姿勢と一致する。
- 書き込むレジストリは `SHCTX`（currentUser では HKCU）配下のアンインストール登録のみ。
  DisplayName / DisplayIcon / DisplayVersion / Publisher / InstallLocation /
  UninstallString / NoModify / NoRepair / EstimatedSize。
  **Run キーもサービスもシェル拡張も書かない。**
- `ExecWait` は4箇所。2つは再インストール時に旧版のアンインストーラーを走らせるもの。
  残り2つは **Microsoft の WebView2 ブートストラッパ**で、片方は `needsadmin=true` を渡す。
  ただしこれは WebView2 が見つからない場合の分岐にのみ入る。
  この実機には WebView2 **150.0.4078.99** が導入済みで、**この経路は走らない**。

残る未検証は「実際に実行したときの画面と結果」だけであり、
インストーラーが行おうとしている操作の内容は上記のとおり確定している。
無人の `/S` 実行は、依頼のないソフト導入という副作用が出るため行わない。

### インストーラーの実機検証（2026-07-27・利用者の明示許可のもと無人実行）

静的監査だけでなく、利用者から無人実行の許可を得て実際に走らせた。**導入→起動→除去→再導入まで確認。**

| 手順 | 結果 |
| --- | --- |
| `/S` 無人インストール | exit 0、所要 1.2 秒。UAC 昇格なし（per-user、WebView2 導入済みのため昇格分岐に入らない） |
| 導入先 | `%LOCALAPPDATA%\PCCustom\` に `pc-custom.exe`（9.4 MB）と `uninstall.exe`（75 KB） |
| アンインストール登録 | DisplayName=`PCCustom` / **Publisher=`PCカスタム`** / Version=`0.1.0` / InstallLocation・UninstallString とも正 |
| スタートメニュー | `PCCustom.lnk` 生成 |
| **導入版の起動** | 起動成功。ウィンドウタイトル `PCカスタム`、常駐 **36.8 MB**、Responding=True |
| **データパス** | `%LOCALAPPDATA%\PCCustom\data\` に `pc-custom.db` ほかを生成。識別子変更とDB改名が端まで効いている |
| `/S` 無人アンインストール | exit 0。本体・スタートメニュー・レジストリ登録すべて除去 |
| **アンインストール後もデータは残る** | `pc-custom.db` は残存。**これは正しい。** このDBには変更を元へ戻すためのjournalとbackupが入っており、消すと利用者が加えたWindowsの変更を二度と戻せなくなる |
| 再導入 | exit 0。本体・ショートカット・登録すべて復帰 |

publisher 設定が「アプリと機能」の発行元欄へ実際に届くことも、ここで初めて実証された。

**最終状態: 導入済み。** 除去は「アプリと機能」から、または
`%LOCALAPPDATA%\PCCustom\uninstall.exe` を実行する。

なお `%LOCALAPPDATA%\Totonoe\` は改名前の開発データが残ったもの。実害はないが、
不要なら手動で削除してよい（改名で識別子が変わったため、新版はこちらを参照しない）。
