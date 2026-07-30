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

## 出荷バイナリから検証用計器を外した（2026-07-27）

リリースビルドの「未使用」警告8件を追ったら、全て `windows/ui_probe.rs` の関数だった。
調べると **製品コードはこのモジュールを一度も呼んでいない**（唯一の参照は `windows/mod.rs` の
`pub use` で、その再エクスポート自体も未使用）。

にもかかわらず 2,061 行が出荷バイナリへ入っていた。中身は検証専用の計器で、
**エクスプローラーの窓を開き、`SHGetSetSettings` でシェル設定を書き、窓へ `WM_CLOSE` を送る**
処理を含む。製品が使わない以上、これは不要な攻撃面でしかない。

`mod ui_probe;` と再エクスポートを `#[cfg(test)]` で囲った。

- 未使用コード警告 **8 → 0**
- 出荷 exe **9.19 MB → 9.09 MB**
- 通常テスト **229 通過**、実機テスト **27 通過**（計器はテストからは今までどおり使える）

教訓: コンパイラの「never used」警告は、消し忘れではなく
**出荷してはいけないものが出荷されている合図**のことがある。

## シェル再起動なら反映される（2026-07-27 実測）

42 件の候補を表示専用に据え置いてきた根拠は、「レジストリへ書いても実 UI が動かない」という
実測だった。今回、**シェル(エクスプローラー)を再起動すれば反映される**ことを実測した。

```
before:   TaskbarAl=1  start_center_ratio=0.304   （中央寄せ）
restart#1 → applied:   start_center_ratio=0.014   delta=0.290   ← 実際に左端へ動いた
restart#2 → restored:  start_center_ratio=0.304   delta=0.000   ← 完全に元へ戻った
```

再現: `cargo test --lib -- --ignored --nocapture shell_restart_makes_taskbar_alignment`

### 実装（`windows/shell_restart.rs`）

文書化された API だけを使う。`CreateToolhelp32Snapshot` で explorer.exe を列挙し、
`ProcessIdToSessionId` で**自分と同じセッションのものだけ**に絞り、`TerminateProcess` で終了、
`FindWindowW("Shell_TrayWnd")` で復帰を待つ。
トレイ窓へ未文書のメッセージを投げてシェルを畳む手口は使わない（BRIEF が禁じる
「文書化されていない内部仕様への依存」に当たる）。

**Windows の `AutoRestartShell` による自動復帰は、この環境では 10 秒待っても効かなかった。**
`relaunched: true` が出ており、こちらから `%WINDIR%\explorer.exe` を起動し直すフォールバックが
実際に仕事をしている。飾りではないので消さないこと。

### 手順上の注意

**explorer を落とすと、実行中コマンドの出力パイプが切れて戻らなくなる。**
最初の実行は 10 分でタイムアウト強制終了された（強制終了では Drop ガードも走らない）。
幸いレジストリは復元済みでシェルも復帰していたが、
**シェルを再起動する実機テストは必ずファイルへリダイレクトして背景実行すること。**

### まだ解けていない問題

反映できることと、反映されたと**確認できる**ことは別である。
42 件のうち外から観測する手段があるのは一部だけで、
`explorer.info_tips` や `input.autocorrect` のように画面から読めないものが多い。
観測できないものを「変更できます」として出せば、6 件を出荷したときと同じ失敗になる。
昇格は項目ごとに観測手段を用意できたものから行う。

## 候補の昇格経路を作り、1件目を昇格（2026-07-27）

シェル再起動で反映されることが実測できたので、候補を可変にする経路を作った。

**分かったこと**: 42件を封じていたのは `registry_metadata()` が `kind` と `method_class` を
固定していたことに加え、`ActionMetadata::validate_static_contract()` に
**「安定Actionは `requiresExplorerRestart` を持てない」という不変条件がコードで強制されていた**。
方針が文書だけでなくコードに書かれていた。良い設計だが、方針を変えるならここも変える必要がある。

不変条件は次の意味へ変えた。`requiresExplorerRestart` は「**反映にシェル再起動が要る**」という
表示であって「適用時に勝手に再起動する」ではない。再起動は利用者が別途選んだときだけ行う。
ただし **`auto_apply_eligible` との併用は禁止のまま**にした。ゲーム起動で自動適用された瞬間に
開いているフォルダーの窓が予告なく閉じては困る。

**追加したもの**:

- `DwordRegistryAction` に `verified: bool`。false のあいだは従来どおり validate/backup/apply が
  拒否を返す。42件のうち41件は今も false。
- `verified_registry_metadata()` と `verified_action_metadata!`。渡した説明・危険度・根拠URLを
  そのまま使い、`kind: Persistent` / `method_class: DocumentedRegistry` /
  `requiresExplorerRestart: true` / `maximumTestedBuild: 26_200` を立てる。
- `apply_verified_registry_backup()`。適用直前に現在値を読み直し、preview 時と違えば
  `ExternalConflict` で書かずに止める。

**昇格1件目**: `taskbar.alignment`。実機の往復確認済み（0.304 → 0.014 → 0.304）。

**テストの扱い**: 「候補は42件」という固定値は「41件以下」へ変えた。減る方向にしか動かないので、
増えていたら確認していないものを足したということで落ちる。あわせて
`verified_registry_actions_are_mutable_but_never_auto_applied` を追加し、
昇格した項目が自動適用に載らないことを固定した。

230 テスト通過、clippy `-D warnings` 通過。

### 一括測定へ切り替えて2件追加昇格（2026-07-27）

1件ずつ測ると再起動が件数×2回になり、画面が何十回も点滅する。
**観測信号が互いに独立している項目はまとめて適用し、再起動1回で同時に判定する**方式へ変えた。
`batch_measure_taskbar_candidates_after_shell_restart` がその実装。

実測結果（再起動2回で2件を判定）:

```
before: taskbar.search_mode   marker="検索"                 present=true
before: taskbar.show_desktop  marker="デスクトップを表示する" present=true
restart#1 → applied: taskbar.search_mode   true -> false  changed=true
            applied: taskbar.show_desktop  true -> false  changed=true
restart#2 → restored: 両方とも present=true（元どおり）
```

昇格: `taskbar.search_mode`、`taskbar.show_desktop`。**昇格済み3件 / 未検証39件。**

### タスクバーのUIAツリーは想像より読める

下調べで分かったこと。要素名には `検索`、`タスク ビュー`、`ウィジェット 33°C 晴れ`、
`デスクトップを表示する`、`時計 18:21`、`ボリューム`、`ネットワーク`、`通知` が並ぶ。
**項目の有無はそのまま判定に使える。**

副産物: 以前 Guided へ降格した6件のうち `taskbar.task_view` と `taskbar.widgets` も
この一覧に出ている。シェル再起動を前提にすれば再測定で復活する可能性がある。

### テストの見本には昇格済み項目を使わない

`unverified_registry_candidates_...` は `TASKBAR_SEARCH_MODE_ACTION` を
「未検証候補の見本」として参照していたため、昇格した瞬間に落ちた。
まだ未検証の `START_LAYOUT_ACTION` へ差し替えた。
**昇格のたびに落ちるテストを書かないこと。**

### 反映手段をUIへ配線（2026-07-27）

昇格した3件は適用できるようになったが、**利用者が反映させる手段が画面に無かった**。
適用しても何も変わらないままで、以前と同じ「押しても何も起きない」状態になっていた。

- `windows/shell_restart.rs` をテスト専用から製品コードへ移し、Tauri コマンド
  `restart_explorer_shell` を追加。**引数を取らない**ので、フロントから渡せるものが無く
  経路として悪用しようがない。ACL にも登録。
- `ExplorerRestartPanel.tsx`。`requiresExplorerRestart` の Action を選ぶと出る。
  押す前に、開いているフォルダーの窓が全部閉じること、タスクバーが数秒消えること、
  他のアプリは終了しないことを先に書く。

**全画面のゲーム中は押さないよう警告を入れた。** これは今日この環境で実際に踏んだ問題で、
シェルを作り直す際にフォーカスが動くため、全画面のゲームがウィンドウ表示に落ちたり
最小化されたりしうる。利用者が VALORANT 起動中だったため実測を中断した経緯がある。

`catalog.ts`（接続前に見えるオフライン版）の説明が古いままだったので3件とも更新した。
**片側だけ直すと、接続前の画面が嘘をつく。**

検証: パネルのコントラストを明暗とも実測し不合格0件、10px 未満0件、横あふれなし。
230 テスト・clippy・fmt・フロントビルドすべて通過。

## Phase 1 A の決定実験: オーバーレイはタスクバーより手前に置ける（2026-07-27）

「画像1枚でタスクバーを着せ替えたように見せる」を **Safe 方式**（Explorer へ手を入れず、
別ウィンドウを重ねるだけ）で成立させられるかは、次の一点にかかっていた。

**自分のウィンドウを `Shell_TrayWnd` より手前の Z 順に置けるか。** タスクバーは topmost である。

```
taskbar rect: left=0 top=1032 right=1920 bottom=1080
z-order: overlay=10  taskbar=11   （小さいほど手前）
EVIDENCE: オーバーレイはタスクバーより手前に置けた
```

再現: `cargo test --lib -- --ignored --nocapture overlay_window_can_sit_above`

使ったのは `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE`
の popup ウィンドウ＋`SetWindowPos(HWND_TOPMOST)`。すべて公開 API で、Explorer への injection も
未文書の手口も使っていない。**新規依存はゼロ**（必要な Win32 feature は既に有効だった）。

判定は目視ではなく `GetTopWindow` / `GetWindow(GW_HWNDNEXT)` で Z 順を辿って順位を数えた。
デスクトップのスクリーンショットは撮れない環境なので、「見えた気がする」は根拠にしない。

### この実験が証明していないこと

- **留まり続けること**は証明していない。測ったのはある瞬間の Z 順。他の topmost ウィンドウ、
  全画面アプリ、Explorer 再起動で順序は変わる。最前面を維持する仕組みが別途要る。
- **見た目**は証明していない。順序が手前でも、絵として自然に見えるかは別問題。
  最終判断は人の目が要る。
- 排他フルスクリーンのゲームは今も上に来る。これは正しい挙動（全画面時は停止する方針）。

### 併せて分かった、既存スキーマの未実装部分

`migrations/0001_initial.sql` は 10 テーブルを定義しているが、**うち 3 つはコードから
一度も参照されていない**。

| テーブル | 状態 |
| --- | --- |
| `preview_tokens` | 定義のみ。`token_hash` / `action_ids_json` / `before_fingerprints_json` / `os_fingerprint` / `expires_at_unix_ms` / `consumed_at_unix_ms` を持つ |
| `action_leases` | 定義のみ。`resource_key` 単位の排他。現状 ProfileSupervisor がメモリ上で代替している |
| `os_observations` | 定義のみ |

**「30 秒プレビューして、確定しなければ自動で元へ戻す」はスキーマとして既に設計されている。**
実装を足すだけでよく、マイグレーションを壊す必要はない。

## シェル再起動を自分で壊し、テストが「成功」で隠した（2026-07-27）

実体パス検査（`is_windows_shell_image`）を入れた直後から、**シェル再起動が丸ごと効かなくなっていた。**

```
pid=26280 session=1 同一=true 実体一致=false
actual  ="C:\Windows\explorer.exe"   ← QueryFullProcessImageNameW が返す実体
expected="C:\WINDOWS\explorer.exe"   ← %WINDIR% が返す値
等しい=false
```

**Windows のパスは大文字小文字を区別しないのに、`std::path::Path` の比較はバイト単位である。**
API は `C:\Windows`、環境変数は `C:\WINDOWS` を返すため、本物のシェルを取り逃がしていた。

### 悪かったのは比較そのものより、失敗の隠れ方

`restart_shell()` は 1 つも特定できなくても `terminated: 0` で **Ok を返していた**。
呼び出し側からは成功に見え、「再起動したのに反映されない」としか分からない。
**何もしていないのに成功を返すのは、このプロジェクトが最も避けるべき挙動。**
タスクバーが出ているのにシェルを特定できない場合はエラーを返すようにした。

さらに悪いことに、この状態で走らせた前面維持テストが **「維持できた」と報告した**。
`terminated: 0` かつ `タスクバーHWND変化=false`、つまり何も起きていない状態を
「維持」と読んでいただけだった。**結果の数字を見なければ気づけなかった。**

### PowerShell での確認が判断を誤らせた

原因を大文字小文字だと最初に疑ったが、PowerShell で確認したところ

```
実体パス     : C:\WINDOWS\explorer.exe
一致(単純比較): True
```

と出たため「推測は外れ」と判断した。**PowerShell の `.Path` はケースを正規化して返すため、
探していた差分そのものを消していた。** 実コードに直接計器を当てるまで分からなかった。
**別の道具で確認したつもりが、別の道具だから見えなかった。**

### 入れた回帰テスト

`shell_lookup_finds_the_real_shell_when_a_taskbar_exists`。
タスクバーが出ているならシェルのプロセスを必ず 1 つ以上特定できることを検査する。
通常テストなので毎回走る。

## オーバーレイはシェル再起動を越えて前面を保つ（2026-07-27）

上を直したうえで測り直した。

```
再起動前: overlay_z=10  taskbar_z=11
restart : terminated=1 shell_returned=true relaunched=true
再起動後: overlay生存=true タスクバーHWND変化=true overlay_z=13 taskbar_z=292
```

タスクバーは HWND ごと作り直されたが、オーバーレイは生き残り、手前を保った。
再現: `cargo test --lib -- --ignored --nocapture overlay_survives_a_shell_restart`
（**必ずファイルへリダイレクトして背景実行すること。** シェルを落とすと出力パイプが切れる）

まだ測っていない: 他の topmost ウィンドウが前に出たとき、全画面アプリの出入り、
モニター構成や DPI の変更、タスクバーの自動非表示。

## CI が落ちた本当の理由は Windows Server だった（2026-07-27）

公開後、最初の CI が `build` 失敗・`bundle` スキップで赤くなった。

```
engine::tests::full_user_journey_..._on_real_machine
COMPATIBILITY_BLOCKED「このWindows環境では、この変更は読み取り専用です」
```

**アプリは正しい。** 互換性ゲートが「この環境では変更しない」と fail-closed に倒しただけ。
悪かったのは CI 設計で、ホストの Windows に依存するテストを `cargo test --lib` に含めていた。
テスト名に `on_real_machine` と書いてあるのに気づけなかったのは、
**自分の機械（26200）では必ず通るから**である。

### 1回目の修正は外した

`identity.base_build < 26_100` で飛ばす、と書いた。CI はまた落ちた。
スキップ文が出ていないので、ランナーのビルドは 26100 以上だった。

本当の理由は `catalog.rs` の

```rust
if os_identity.major != 10 || os_identity.product_type != 1 {
```

**`product_type == 1` はクライアント版 Windows。GitHub の windows ランナーは Windows Server。**
ビルド番号という誤った軸で判定していた。

### 2回目: 判定を製品と共有した

テストの中で互換性ルールの一部を書き直していたのが誤り。
`CompatibilityCatalog::decision_for_identity` を呼び、`TestedMutable` でなければ
build / product_type / 判定結果を印字して飛ばす形にした。
**ルールを二重に書けば、二つの写しは必ずずれる。** その小さな実例だった。

結果: `build: success` / `bundle: success`。**インストーラーが CI 上でも生成できることも確認できた。**

## アンインストーラーの取り消しと、残ったショートカット（2026-07-28）

利用者が誤ってアンインストーラーを起動した。状態を確認し、元へ戻した。

- 本体・アンインストーラー・「アプリと機能」の登録: 削除されていた
- **`pc-custom.db` は残っていた**（設計どおり。変更を元へ戻すための記録なので消さない）
- **スタートメニューのショートカットが残っていた** ← 本体が消えているのに参照だけ残る

`/S` で試した前回のアンインストールではショートカットも消えていた。
**対話的に実行したときだけ残る可能性がある。** 未診断。再現には対話実行が要るため保留。

`/S` で再インストールし、本体・登録・ショートカット・DB すべて復帰を確認した。

## 試用（適用してから決める）を実装（2026-07-28）

企画 §6・§9 の「一時的にプレビューし、確定しなければ自動で元へ戻す」を入れた。

**Phase 0 の報告を訂正する。** 「30秒プレビューはスキーマだけあって未実装」と書いたが、
プレビュートークン自体は `engine/mod.rs` にメモリ上で期限付き実装されていた。
未実装だったのは**「適用した状態を見せてから決めさせる」体験の方**である。

### 設計

- `commit_preview_as_trial(token, hold_seconds)`: 通常どおり適用したうえで、
  `trials` テーブルへ期限を記録する。上限は10〜300秒に丸める
  （長すぎる試用は「戻るはずが戻らない」と体感上同じになる）。
- `confirm_trial(transaction_id)`: 「保存する」。以後この変更は自動で戻さない。
- `revert_expired_trials()`: 起動時に、期限を過ぎても確定されていないものを元へ戻す。

**期限はメモリではなく journal に持つ。** 試用の途中でアプリが落ちても、
次に開いた時点で戻る。ここをメモリに置くと、落ちた場合に変更が取り残される。

**戻せなかったものは `trials` から消さない。** 消すと存在ごと忘れるので、
通常の復旧経路が拾えるように残す。

`migrations/0002_trials.sql` を追加。`IF NOT EXISTS` なので既存のDBへそのまま流れる。

### テスト

`trial_expires_and_is_reverted_unless_confirmed` は、期限切れが拾われること、
期限前は拾われないこと、確定済みは期限を過ぎても拾われないこと、
二重確定が false を返すことを固定する。
**ここが壊れると「保存したのに戻る」か「戻るはずが戻らない」のどちらかになる。**
実際のロールバック実行までは走らせない（実機の設定を触るため）。

## オーバーレイは前を取られる。取り返せる（2026-07-28 実測）

```
割り込み後: overlay=11  rival=10     ← あとから出た topmost に前を取られる
取り返し後: overlay=11  rival=12     ← SetWindowPos(HWND_TOPMOST) で取り返せる
```

再現: `cargo test --lib -- --ignored --nocapture overlay_can_retake_the_front`

**「一度 topmost にすれば前に居続ける」は成り立たない。** 通知、ゲームのオーバーレイ、
他のツールが出るたびに前を取られる。設計上、**前面の再主張が必要**である。
`SetWindowPos(HWND_TOPMOST)` で取り返せることは確認できたので、
可視性が変わった時などに呼び直せばよい。常時ポーリングは要らない見込み。

未測定のまま: DPI 変更、モニター構成の変更、タスクバーの自動非表示、
排他フルスクリーンの出入り。

## エクスプローラー系の候補は「現時点では測れない」（2026-07-28）

`explorer.status_bar` と `explorer.always_show_menus` を、設定変更後に**新しく開いた自分の窓**で
判定しようとした。結果は「変化なし」だったが、**この結果は根拠にならない。**

UIA は**非表示の要素も列挙する**。「ステータス バー」という名前が木に在ることと、
それが画面に見えていることは別である。`CurrentIsOffscreen` と境界矩形で
見えていないものを落とす修正を入れたが、要素数は 112 のまま変わらなかった。
つまりこのフィルタも効いていない。

**したがって、この2件について言えるのは「反映されない」ではなく「測れていない」である。**
測れないものを昇格させるのは最初の失敗の繰り返しであり、
測れないのに「効かない」と断じるのも同じく根拠がない。両方しない。

必要なのは別の観測手段（クライアント領域の分割位置を見る、など）で、それは別作業。

なお、この過程で入れた「見えている要素だけを返す」フィルタ自体は残す。
今回は効かなかったが、要素の存在と可視は別物という前提は正しい。

## オーバーレイの置き場所を Windows へ聞く土台（2026-07-28）

`windows/overlay_anchor.rs`。**読み取りだけ**で、窓は作らないし動かさない。

- `read_taskbar_anchor()`: `SHAppBarMessage(ABM_GETTASKBARPOS / ABM_GETSTATE)` で
  タスクバーの矩形・どの辺にあるか・自動非表示かを読む。
  シェルを再起動すると HWND ごと変わるので、位置は毎回聞き直す前提にする。
- `foreground_is_fullscreen()`: 前面の窓が画面を覆っているか。
  全画面のゲームや動画の上に描くと、邪魔なうえ全画面表示から落ちる原因になる。

実測:

```
taskbar: (0, 1032) - (1920, 1080)  1920x48  edge=Bottom auto_hide=false
前面が全画面: false
```

**分からないときは「全画面」と答える。** 前面が取れない、矩形が読めない、
モニター情報が取れない、いずれも `true` を返す。描いてしまう害の方が大きいので、
迷ったら描かない側へ倒す。

### 一度、偽のテストを書いた

最初に書いた検査は「ソース内にコメント文字列が残っているか」を見るだけだった。
**実装が壊れても通る。** 判定を座標だけの純粋関数 `rect_covers_monitor` へ切り出し、
値で確かめる形に直した。最大化（タスクバーの分だけ短い）を全画面と誤判定しないことも固定した。
ここを取り違えると、最大化しただけの窓の上に描かなくなる。

### エクスプローラー候補: 観測に3回失敗（2026-07-28）

| 回 | 使った信号 | なぜ駄目だったか |
| --- | --- | --- |
| 1 | 要素名の有無 | UIA は**非表示の要素も列挙する**。木に在ることと見えていることは別 |
| 2 | `CurrentIsOffscreen` と境界矩形で絞る | 要素数が 112 のまま変わらず、フィルタが効いていない |
| 3 | 一覧の高さ | 掴んだのは `(1621, 128, 1920, 1008)`、幅299pxの**詳細ウィンドウ**だった |

3回とも「変化なし」と出たが、**どれも「反映されない」の根拠にならない。**
必要なのは一覧そのものを取り違えずに掴む手段で、それは別作業。
測定コードには結論を書かず、「この経路では判定できない」とだけ出すようにした。

`explorer_element_rect` は残す。名前で要素の矩形を引く手段自体は使える。

## 降格していた2件は、実は効く。原因は自分のコードだった（2026-07-28）

`taskbar.task_view` と `taskbar.widgets` は「書いても反映されない」として Guided へ降格していた。
シェル再起動込みで測り直したところ、**両方とも反映される。**

```
applied:  taskbar.task_view  present true -> false   changed=true
applied:  taskbar.widgets    present true -> false   changed=true
restored: 4件とも present=true（元どおり）
```

降格当時は**設定変更通知だけで測っていた**。反映されないという観測は正しかったが、
「効かない」という結論は早すぎた。

### 途中で利用者のタスクバーを2回消した

測定のたびにテストが失敗し、終わった後に explorer が 0 プロセスになっていた。
原因は `relaunch_shell()` が `Command::spawn()` で **explorer を自分の子として起動**していたこと。
`cargo test` はテストプロセスをジョブオブジェクトで囲むため、
**テストが終わると子の explorer も一緒に殺される。**

これは**製品側の欠陥でもある。** 再起動ボタンを押したあとにアプリを閉じれば同じことが起きる。

`DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB` を付けて、
ジョブが breakaway を許さない場合は段階的に落とす形にした。
修正後の測定では、テスト終了後も explorer が残っている。

**「検索が復元されない」という失敗も、これが原因だった。** レジストリは `=2` で正しく戻っていて、
シェルが落ちていたから読めなかっただけ。値を確認せず「復元されない」と判断していたら、
無実の復元処理を疑うところだった。

### 併せて直したもの

- `restart_shell()`: 期限までに戻らなければ、諦める前にもう一度起動して待ち直す
- 測定の後始末: レジストリを戻したうえで、**タスクバーが在ることまで確認**してから抜ける
- 復元の確認: 1回読んで違えば失敗、ではなく戻るまで待つ

昇格は**5件**（taskbar.alignment / search_mode / show_desktop / task_view / widgets）。

## 電源モードは3値そろって選べるとは限らない（2026-07-28 実測）

`PowerSet/GetUserConfigured{AC,DC}PowerMode` は windows-rs 0.58 に無い。手書き FFI で、
`GetProcAddress` による実行時解決にした。静的リンクだと、エクスポートが無い環境で
プロセスごと起動不能になる。

**署名を当てたので、当たったかどうかを測った。** 両 getter が文書化された overlay GUID を返し、
別 API の実効モード通知も同じ方向を報告した。ただし値は一致しない
（要求 `BestPerformance` / 実効 `MaxPerformance`）。要求値と実効値は最初から別物として扱う。

最初の往復テストは通ったが、何も証明していなかった。**元と同じ値を書いていた。**
供給ごとに今と違う値を書くよう直したら、この機は「電池優先」を AC・DC どちらでも
`ERROR_INVALID_PARAMETER` で拒否することが出た。「バランス」「パフォーマンス優先」は通る。

事前に「どの overlay が使えるか」を聞く公開手段は見つからなかった。書いてみるまで分からない。
なので UI では選択肢を灰色にせず、**押す前に「選べない値があるPCもあります」と書き**、
拒否されたら一般的な失敗ではなく「この PC ではこのモードを選べません」と伝える。
拒否時、値は変わらないことも読み直して確認済み。

## 変更記録が空になった。原因は不明（2026-07-28）

照合機能の検証で `JournalDatabase::open` を**実機の journal に向けた。**
その前後で、9件あった取引記録が 0 件になった。

再現しない。同じ DB の複製に対して同じテストを走らせても 9 件のまま無傷だった。
`open` は `IF NOT EXISTS` のスキーマ適用と `quick_check` だけで、DELETE も DROP もしない。
`cargo fmt` と `cargo build` 以外、その間に走ったものは無い。**原因は特定できていない。**

分かっているのは、これが起きうる状況を自分で作ったということ。
`JournalDatabase::open` は読み書きで開く。検証のために利用者の変更記録へ
書き込み可能なハンドルを持つ理由は無い。テストは複製を作ってから開くよう直した。

消える前の複製は残っている。復元にはユーザーの許可が要る。

## タスクバーの自動的に隠す設定は、作業領域で確かめられる（2026-07-28 実測）

`ABM_SETSTATE` は**常に `TRUE` を返す。** 何も起きなくても TRUE なので、戻り値は証拠にならない。
別経路で確かめる必要がある。3つ読んで、どれが使えるかを測った。

```
before   bit=false work_area=(0,0,1920,1032) taskbar_rect=(0,1032,1920,1080)
書いた後 bit=true  work_area=(0,0,1920,1080) taskbar_rect=(0,1032,1920,1080)
戻した後 bit=false work_area=(0,0,1920,1032) taskbar_rect=(0,1032,1920,1080)
```

**作業領域は動く。タスクバーの矩形は動かない。**
自動的に隠す設定にしてもタスクバーは矩形を保ったままなので、
`ABM_GETTASKBARPOS` は反映の証拠に使えない。使えるのは `SPI_GETWORKAREA` のほう。

反映は同期だった。600ms 待つ前の観測で既に作業領域が変わっている。
待ちを入れる理由は今のところ無い（ただし1台でしか測っていない）。

第三者が変えた状態で書こうとすると `ExternalConflict` で止まり、
断ったあと読み直しても値は変わっていない。断ったなら本当に書いていない。

常駐の監視（最大化を見て自動で切り替える部分）はまだ無い。
危ないのは監視のほうで、ちらつきと ownership 放棄の規則が要る。

## 自分で仕掛けた罠: 隠すと最大化が全画面に見える（2026-07-28）

最大化の判定に既存の `foreground_is_fullscreen` を使おうとした。あれは
「窓の矩形がモニターを覆っているか」で判定する。オーバーレイには正しい。ここでは壊れる。

**タスクバーを隠すと作業領域がモニター全体まで広がる。**
すると最大化した窓の矩形もモニターを覆う。全画面と判定される。全画面は対象外にしているので
以後この機能は何も決めなくなり、**利用者が最大化を解いてもタスクバーは隠れたままになる。**

順番を逆にした。まず `showCmd` で最大化かどうかを決め、モニターを覆っているかは
「最大化ではないのに覆っている窓」＝枠なし全画面を除くためだけに使う。

## 前面を奪えないと判定を一度も測れない（2026-07-28）

最初の実機テストは `SetForegroundWindow` で自分の窓を前面にしてから判定するつもりだった。
**Windows は前面の奪取を断る。** 断られてテストは「測れなかった」を出して終わった。
そこで判定を `classify(hwnd)` に切り出し、前面かどうかと切り離した。

```
小さい窓   Some(false)
最大化     Some(true)
元に戻す   Some(false)
```

これで初めて `Some(true)` と `Some(false)` の両方を通った。
それまでの測定は前面が枠なし全画面の窓だったため、`None` しか出ていなかった。

## 表示構成の読み取りで2回外した（2026-07-28 実測）

研究文書の項目9は「実験のみ、復帰を証明できるまで出荷しない」が結論。
黒画面になったら利用者は戻す操作すら押せない。なので**適用は作らず、読み取りと比較だけ**を作った。

1回目の間違い: `QueryDisplayConfig` が `ERROR_INVALID_PARAMETER`。
フラグを疑って3通り試したが全部同じエラーだった。原因は第6引数で、
**`currentTopologyId` は `QDC_DATABASE_CURRENT` のときだけ渡せる。**
いま出ている構成を読む用途では受け取れない。だから構成にトポロジ ID は持たせず、
複製か拡張かは描画元と複製グループで見分けることにした。

2回目の間違い: 通ったあとの値が `0x0` だった。
`QDC_VIRTUAL_MODE_AWARE` を渡すと、モード配列の添字は 32bit の生値ではなく
**16bit ずつに詰められた形**になる。生値のまま読むと必ず外れる。
外れたときに 0 が入るので、**気付かなければ全構成の解像度が 0 になり、
「解像度は同じ」といつでも言えてしまう。** 比較そのものが無意味になっていた。

直した結果は 1920x1080 / 239.999Hz で、同じ日にタスクバーの測定で読んだ画面の大きさと一致する。

## クリック透過の合格は、覆われているだけだった（2026-07-29）

モードリボンの実機テストは `WindowFromPoint` がリボンを返さないことを合格としていた。
緑だった。**何も証明していなかった。**

透過しない窓をわざと同じ位置に置いて計器を確かめたら、そちらも返らなかった。

```
ribbon_probe visible=true rect=(0,1028,1920,1032) wanted=(0,1028,1920,1032)
ribbon_probe hit_rect=(0,0,1920,1080) hit_visible=true
ribbon_probe opaque_hwnd=0x28028A WindowFromPoint=0x8F07D4 probe_sees_it=false
```

原因は画面いっぱいの窓が手前にあること。この機の常態がそれ。
その状態では、**透過していてもいなくても `WindowFromPoint` の答えは同じ**になる。

Z順に依存しない測り方へ替えた。窓自身の性質だけを見る。

```
ribbon_transparency ex_style=0x080800A8 ws_ex_transparent=true nchittest=-1 htransparent=true
```

`WS_EX_TRANSPARENT` が立っていること、`WM_NCHITTEST` が `HTTRANSPARENT`(-1) を返すこと。
どちらもほかの窓の位置に左右されない。

`WindowFromPoint` の側は残したが、覆われているときは見送りにして、
**どのテストが透過の証拠を持っているかを見送りの文言に書いた。**
見送りを合格として読ませない。

教訓は前と同じ形。**「見えない」は「透過している」ではない。目を閉じても見えない。**

## 「フォルダーを別プロセスで開く」は外部PID差を証明できなかった（2026-07-29 実測）

`explorer.separate_process` の昇格ゲートとして、Windows 25H2 build 26200.8875 で次を測った。

- 文書化された `SHGetSetSettings` に `SSF_SEPPROCESS` を渡し、設定をオフ／オンにした直後に
  同じ API で読み直した。戻り値が `void` であることを踏まえ、呼び出し完了自体は証拠にしていない。
- 各状態で一意な検査フォルダーを2つ作り、既存 HWND の集合に無かった新規
  `CabinetWClass` かつ一意なタイトルを含む窓だけを自己所有窓として採用した。
  利用者の Explorer 窓は数えず、閉じていない。
- 各自己所有窓の PID を `GetWindowThreadProcessId` で外から取得し、
  `GetShellWindow` の PID と比較した。自己所有窓は `Drop` で閉じ、設定も別の `Drop` guard で
  元の `false` へ戻した。検査用フォルダーの残骸も無い。

実行:

```text
cargo test --lib separate_process_setting_changes_owned_explorer_window_process_pattern -- --ignored --nocapture --test-threads=1
```

実測値:

```text
EVIDENCE: separate_process original=false restored=true off_readback=false off_shell_pid=18772 off_window_pids=[27780, 29484] off_matches_shell=[false, false] on_readback=true on_shell_pid=18772 on_window_pids=[29644, 15164] on_matches_shell=[false, false] expected_pattern=false
```

**結論: `explorer.separate_process` を昇格してはいけない。表示専用のままにする。**

設定値の readback はオフ／オンで変わったが、外部観測の形は両方とも
「2窓がそれぞれシェルと異なる PID」で同じだった。新しく開いたプロセスの生 PID が
異なること自体は設定効果の証拠ではない。したがって、この経路では
「設定オンによってフォルダー窓のプロセス分離が変わった」と証明できない。
機能自体が現行 Windows で効かないとは断定せず、外部差を証明できる別の観測が得られるまで
Action の変更経路は実装しない。

## 安全なホットコーナーを実装（2026-07-29）

モード画面の中へ、画面4角ごとの「何もしない / モード画面を開く」設定を追加した。
4角すべての既定値は「何もしない」。既定状態では `GetCursorPos` も呼ばず、監視コストと誤発火を足さない。

有効時は既存の監視スレッドへ相乗りし、200ms間隔（5Hz）で読み取りだけを行う。
global low-level mouse hook と `SetCursorPos` は使わない。発火時に行うのはPCカスタム自身の窓を
表示・復元・前面化して、モード画面へ移ることだけ。Action の preview / commit と Windows setter は呼ばない。

誤発火防止として、純関数の状態機械へ次を固定した。

- 既定1.5秒の滞在時間。座標が動いたら数え直す。
- 既定15秒のクールダウン。発火後は角から離れるまで再発火しない。
- 全画面は抑制。判定不能も全画面扱いの既存 fail-closed を使う。
- 最大化中も抑制。利用者の作業を自アプリで覆う害を、呼び出しの便利さより重く見た。
- `MonitorFromPoint` / `GetMonitorInfoW` / `EnumDisplayMonitors` で、モニター間の内側角を除外する。
- 設定ファイルが読めない・版違い・範囲外の場合も、全角「何もしない」へ倒す。

単体テストは指定6条件を含む7件。全体結果:

```text
cargo test --lib -- --test-threads=1
test result: ok. 317 passed; 0 failed; 60 ignored; 0 measured; 0 filtered out

cargo clippy --all-targets -- -D warnings
Finished dev profile

cargo fmt --check
ok

npm run build
TypeScript typecheck / Vite production build 成功
```

読み取り専用の実機テスト（利用者のマウスは動かしていない）:

```text
EVIDENCE: cursor=(1608, 633) primary=(0, 0, 1920, 1080) outer_corners=[(TopLeft, ScreenPoint { x: 0, y: 0 }), (TopRight, ScreenPoint { x: 1919, y: 0 }), (BottomLeft, ScreenPoint { x: 0, y: 1079 }), (BottomRight, ScreenPoint { x: 1919, y: 1079 })] polls=25 wall_ms=5017
EVIDENCE_CPU: polls=25 interval_ms=200 wall_ms=4972.8 process_cpu_ms=31.25 average_single_core_percent=0.628
```

CPU値はテストハーネス込みのプロセス全体。5Hz観測25回を約5秒走らせた上限寄りの値であり、
既定無効時は観測自体をスキップする。

## ホットコーナーの常駐費用は「上限しか言えない」（2026-07-29 実測）

5Hz で `GetCursorPos` とモニター列挙を回したときの CPU 時間。

```
polls=25 wall_ms=5005 cpu_us=15625
```

15625us はちょうど 15.625ms、既定のタイマー間隔1ティック分。
`GetProcessTimes` はこの単位でしか刻まないので、**これは分解能の底**であって
「15.6ms 使った」という意味ではない。実際の消費は 0 から 15.6ms のどこか。

言えるのは上限だけ。**1コアの約 0.3% を超えない。** それ以上細かいことは、
この計器では分からない。「影響なし」とは書かない。

角の判定は既定で全部「何もしない」。入れていない人には何も起きない。

## 「ファイルの説明ポップアップ」は外部差を測れなかった（2026-07-29 実測）

`explorer.info_tips` の昇格ゲートとして、次を実装して実機で走らせた。

- 文書化された `SHGetSetSettings` に `SSF_SHOWINFOTIP` を渡し、`fShowInfoTip` を
  オフ／オンにした直後に同じ API で読み直した。レジストリは直接書いていない。
- 一意な検査フォルダーと 12 KiB の一意な検査ファイルを各状態で作り、既存 HWND の集合に無かった
  新規 `CabinetWClass` かつ一意なタイトルの窓だけを自己所有窓として開いた。
- 自己所有ファイルの UIA 矩形中央へポインターを置き、UIA の ToolTip 型を 200 ms 間隔で
  40回読む計器を用意した。自己所有 Explorer の PID と、対象項目から 800 px 以内の件数を別々に数える。
- 設定は `Drop` guard で元の `true` へ戻す。ポインターも、実際に移動できた場合だけ
  `Drop` guard で元位置へ戻す。自己所有窓は `Drop` で閉じ、一時フォルダーとファイルは
  `TempDir` で削除する。利用者の窓とファイルには触れていない。

実行:

```text
cargo test --lib info_tip_setting_changes_owned_explorer_tooltip_visibility -- --ignored --nocapture --test-threads=1
```

実測値:

```text
EVIDENCE: info_tip original=true restored=true off_readback=false off_cursor_moved=false off_samples=0 off_samples_with_tooltip=0 off_max_visible=0 off_max_owned=0 off_max_near=0 off_names=[] off_unavailable=Some("SetCursorPos failed") on_readback=true on_cursor_moved=false on_samples=0 on_samples_with_tooltip=0 on_max_visible=0 on_max_owned=0 on_max_near=0 on_names=[] on_unavailable=Some("SetCursorPos failed") outward_difference=false
```

**結論: 現時点では効果がまだ分からないため、`explorer.info_tips` を昇格してはいけない。
表示専用のままにする。**

公開 API の readback はオフで `false`、オンで `true` となり、最後に元の `true` へ復元できた。
しかし、この実行環境では `SetCursorPos` が両状態で拒否され、ホバーを開始できなかった。
したがって `samples=0` と各 tooltip 件数の `0` は「ポップアップが無かった」という観測値ではなく、
**UIA のサンプリング自体を開始できなかった**ことを表す。オフ／オンの外部差は数値で証明できていない。
機能が現行 Windows で効かないとは断定せず、安全にホバーを起こして元位置へ戻せる環境で
外部差が得られるまで Action の変更経路は実装しない。

## 対応RGBは「戻せない」がAPIの形で確定している（2026-07-29 確認）

`docs/RESEARCH_FEATURES_ROUND2.md` の候補7は「不採用、コード化しない」と結論している。
その前提が本当かを、windows-rs 0.58 のメタデータで直接確かめた。

`Windows.Devices.Lights.LampArray` が公開しているもの:

```
SetColor / SetColorForIndex / SetSingleColorForIndices /
SetColorsForIndices / SetColorsForKey / SetColorsForKeys
BrightnessLevel / SetBrightnessLevel
GetLampInfo / GetIndicesForPurposes / LampCount / LampArrayKind
```

**色の setter は6つある。getter は1つも無い。**
`GetLampInfo` が返すのは lamp の静的な情報で、いま光っている色ではない。

（単体の `Lamp` クラスには `Color()` の getter がある。だがそれはカメラのプライバシーランプ等で、
RGB のキーボードやマウスが出すのは `LampArray` のほう。混同しないこと。）

つまり**触る前の色を読む手段が公開されていない。**
BRIEF の「変更前の状態へ正確に戻す」が API の形として満たせない。
明るさが読み書きできても、色が読めない以上は埋まらない。

**結論: 実装しない。** 制御を手放せば機器は自律モードへ戻るが、
それは「直前の色へ戻した」ではない。戻したふりをしない。

Microsoft が現在色の取得を文書化するか、触る前の状態へ確実に戻せる別の公開手段が
確認できたときに、この判断をやり直す。

## 2回目の調査、7候補の決着（2026-07-29）

| | 候補 | 結果 |
|---|---|---|
| 1 | 既定の通話マイクをミュート | 出荷。同一 device ID にだけ戻す。存在しない ID は `ERROR_NOT_FOUND` で止まる |
| 2 | タスクバー上のモードリボン | 出荷。Windows を1バイトも変えない |
| 3 | 別プロセスで開く の昇格 | **昇格しない。** 外部差を証明できず |
| 4 | 安全なホットコーナー | 出荷。既定は全部「何もしない」 |
| 5 | 配色シーン | 出荷。新しい書き込み API はゼロ |
| 6 | 情報ツールチップ の昇格 | **昇格しない。** ホバーを開始できず測れなかった |
| 7 | 対応 RGB をモード色に | **実装しない。** 色の getter が公開されていない |

出荷4件、見送り3件。**見送りのほうが多い回もあってよい。**
3件とも「効かない」とは書いていない。「この観測では区別できない」「この API では戻せない」まで。

## 自己監査: 直近出荷分を壊しに行った結果（2026-07-29）

`BRIEF.md`、このファイル全体、`tasks/TASK_SELF_AUDIT.md` を先に読み、
直近の mic mute、mode ribbon、hot corner、appearance scenes、power mode、
pointer feel、taskbar auto-hide、health report と、手動同期表を監査した。
Action は増やしていない。**71件のまま。**

### 修正前に確定した欠陥一覧

1. `ActionId::PowerModeSwitch` と `ActionId::InputPointerFeel` だけ Serde の明示名が無く、
   durable backup では `power_mode_switch` / `input_pointer_feel` と保存される一方、
   画面・parser・`as_str()` は dotted ID を使っていた。
2. `hot_corner_get` / `hot_corner_set` は command と `invoke_handler` にあるのに
   `application-commands.toml` から抜け、画面からの呼び出しが ACL で拒否される状態だった。
3. ~~hot corner、taskbar auto-hide、theme schedule の小さな設定 JSON は、
   Windows で既存ファイルを置換できない `std::fs::rename` を使っていた。
   2回目の保存と taskbar の crash-recovery marker 更新が失敗する。~~

   **この指摘は誤り。監査側の検証で否認した。** `std::fs::rename` は Windows でも
   既存ファイルを置換できる（Rust std が `MoveFileExW` に `MOVEFILE_REPLACE_EXISTING` を渡す）。
   実際に測った:

   ```
   EVIDENCE: rename_over_existing ok content="second"
   ```

   時刻設定が何度も保存できていた事実とも一致する。
   **置換できないという前提そのものが間違っていた。**

   ただし置き換えたコード自体は残す。理由が違う。`MOVEFILE_WRITE_THROUGH` は
   戻る前にディスクへ流し込む。強制終了に備える marker にはその durability が要る。
   **「置換できないから」ではなく「落ちる前に確実に書きたいから」。**
   直した理由を、実際に効いている理由へ書き換える。
4. taskbar auto-hide は、実際に隠した**あと**で recovery marker を保存し、
   その保存失敗も無視していた。書き込みと記録の間で強制終了されると戻せない。
5. taskbar の起動時／`Drop` 復旧は、観測・復元・readback が失敗しても marker を消していた。
   さらに UI の設定変更と監視側の marker 更新を、永続化中は lock せず競合できた。
6. pointer feel は2段目の速度書き込み失敗時、3値側の書き戻し API が成功したかしか見ず、
   速度を戻さず、補償後の4値 readback もしていなかった。
7. power mode は DC 書き込み失敗時に AC だけを戻し、DC が失敗を返しながら変わった可能性と、
   AC/DC 両方の補償後 readback を見ていなかった。
8. mode ribbon、taskbar foreground、power mode、health report の実機テストに、
   前提不成立の早期 return と実測済みに読める名前／出力が残っていた。
   appearance scenes の journal test も、通常 preview を呼んでいないのに
   「request が prepare する」と名乗っていた。
9. hot corner の CPU 計測は `GetProcessTimes` 失敗を数値 `0` に変換し、
   正常に 0 tick だった場合と計測失敗を区別できなかった。
10. mic mute の画面説明と実機証跡が API 名・endpoint identifier を露出していた。
    さらに Action 詳細、preview、timeline、mode draft は method の生文言、
    レジストリ情報、transaction/diagnostic GUID、内部 ID をそのまま描画していた。
11. 必須の frontend build は成功扱いだったが、PowerToys 部分に selector の無い
    CSS 宣言が2組残り、minifier が構文エラー警告を出して捨てていた。

### 直したこと

- 2 Action ID に canonical dotted Serde 名を付け、すでに書かれた underscore 名は
  `alias` で読み続ける。全71 IDについて `as_str == serialized tag` と round-trip を固定した。
- hot corner 2 command を ACL へ追加した。
- `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH` を使う小さな共通設定ファイル置換へ
  hot corner、taskbar、theme schedule を揃え、既存ファイルを再保存するテストを足した。
- taskbar は **marker の durable 保存 → Windows 書き込み → readback** の順にした。
  復元を readback できた時だけ marker を消し、失敗時は marker と画面向け error を残す。
  store の persist と memory 更新は同じ mutex の内側へ入れた。
- pointer は2段目が失敗したら4値全部を補償し、独立 readback が元の4値と一致した時だけ
  元の失敗として返す。一致を証明できなければ `RecoveryRequired`。
  power mode も AC/DC 両方を補償し、両方の raw value の readback で判断する。
- 条件付き実機 test は名前を条件付きにし、未計測を必ず `measured=false reason=...` と出す。
  CPU 時間取得失敗は test failure にし、数値 0 に丸めない。
- mic の画面文言を利用者向けにし、実機証跡は endpoint ID 本体を出さず、
  保存した同一 endpoint かだけを示す。画面は method/resource の生文字列と
  GUID／内部 ID を描画せず、利用者向け説明へ変換する。
- appearance scenes の test 名を、実際に証明する
  「parsed 3 actions を distinct journal items として記録できる」に直した。
- selector の無い孤立 CSS 宣言だけを除去した。

### 実機と同期表の証拠

実機設定を変える test は、既存の `Drop` guard を確認してから個別に実行した。
mic、pointer、power、taskbar は適用後と復元後を Windows から読み直している。

```text
EVIDENCE: comms_mic_mute measured=true before endpoint_id_units=55 muted=false
EVIDENCE: comms_mic_mute applied same_saved_endpoint=true muted=true
EVIDENCE: comms_mic_mute restored same_saved_endpoint=true muted=false

EVIDENCE: pointer_feel_action before=PointerFeel { threshold_one: 0, threshold_two: 0, acceleration: 0, speed: 10 }
EVIDENCE: pointer_feel_action acceleration=true after=PointerFeel { threshold_one: 6, threshold_two: 10, acceleration: 1, speed: 10 }
EVIDENCE: pointer_feel_action restored=PointerFeel { threshold_one: 0, threshold_two: 0, acceleration: 0, speed: 10 }

EVIDENCE: power_mode_action measured=true before ac=Known(BestPerformance) dc=Known(Balanced)
EVIDENCE: power_mode_action applied=Balanced ac=Known(Balanced) dc=Known(Balanced)
EVIDENCE: power_mode_action restored ac=Known(BestPerformance) dc=Known(Balanced)

EVIDENCE: taskbar_autohide work_area_full before=false after=true
EVIDENCE: taskbar_autohide conflict outcome=Err(WindowsError { kind: ExternalConflict, operation: "taskbar auto-hide changed by something else", os_code: None })

EVIDENCE: ribbon_transparency measured=true ex_style=0x080800A8 ws_ex_transparent=true nchittest=-1 htransparent=true
EVIDENCE: ribbon_probe measured=false opaque_hwnd=0x160736 WindowFromPoint=0x8F07D4 probe_sees_it=false
EVIDENCE: mode_ribbon measured=false skipped=cannot_measure reason=画面いっぱいの窓が手前にあり、透過の有無を区別できない

EVIDENCE: hot_corner measured=true polls=25 wall_ms=5004 cpu_us=0
EVIDENCE: health_report measured=false baselines=0 unreadable_records=0
EVIDENCE: health_report measured=false reason=基準が1件も無く照合経路は未実行

EVIDENCE: action_tables action_id=71 parameters=71 catalog=71 parametersForAction=71 missing_or_extra=0,0,0,0,0,0
EVIDENCE: command_tables invoke=40 acl=40 missing_acl=0 extra_acl=0
EVIDENCE: screen_copy raw_internal_bindings=0 internal_id_labels=0 sanitizer_present=true
EVIDENCE: published_counts readme71=1 changelog71=1
```

mode ribbon の opaque probe と端から端の geometry test は、前面の全画面窓に覆われて
この実行では測れなかった。これは合格へ混ぜていない。一方、Z順に依存しない
`WS_EX_TRANSPARENT` と `HTTRANSPARENT` は `measured=true` で両方を確認した。
health report は実 journal に baseline が0件だったため、実 backup との照合は未計測。
0件を healthy として報告していない。

### 欠陥を見つけなかった部分

- mic mute の exact saved endpoint rollback、missing endpoint で current default へ
  fallback しない契約には、機能欠陥を見つけなかった。
- mode ribbon の製品側 geometry、選択優先、destroy、Windows設定非変更には
  機能欠陥を見つけなかった。直したのは test の主張と計測表示。
- hot corner の dwell、cooldown、外周角、fullscreen/maximized 抑止の状態機械には
  機能欠陥を見つけなかった。欠陥は ACL と設定ファイル置換。
- appearance scenes の3 Action限定、通常 parameter shape、禁止コピー、
  1 Action 1 journal item には機能欠陥を見つけなかった。
- health report の baselineなし、unknown、第三値、部分復元を healthy に混ぜない分類には
  機能欠陥を見つけなかった。実 journal 照合は上記のとおり未計測。

### 完了コマンド

```text
cargo test --lib
test result: ok. 328 passed; 0 failed; 61 ignored; 0 measured; 0 filtered out

cargo clippy --all-targets -- -D warnings
Finished dev profile

cargo fmt --check
exit code 0

npm run build
✓ 61 modules transformed
✓ built（CSS構文警告も0件）
```

## 一時ワークスペース（2026-07-29）

- モード画面に「一時ワークスペース」を追加した。保存済みの窓配置と、利用者が明示選択した
  `mutable` の既存 `persistent / session` Actionだけを束ねる。登録済みActionは **71件のまま**。
- 実行は既存の `preview -> commit -> journal` を通し、「終わる」はjournal itemを既存どおり
  逆順にrollbackする。アプリは閉じず、workspace作成時にも起動しない。
- 窓は開始時に捕捉できた対象だけを、既存のPID・プロセス生成時刻・opaque HWND markerで
  再同定する。終了時に元位置でも適用位置でもない窓は外部変更として上書きせず、他の窓だけを戻す。
  非上書き理由はrollback結果から画面のnoticeへ出す。
- 別プロセスに標準窓を2枚作る実機ignored testを追加した。窓側から適用後と終了後を読み直し、
  外部移動した1枚を残したまま、未変更の1枚だけが作業前へ戻ることを確認した。panic時も
  `Drop` が子プロセスと窓を破棄する。

```text
EVIDENCE: workspace_windows desired=[(120,140 360x240 show=1),(520,180 360x240 show=1)] before_start=[(460,360 360x240 show=1),(900,390 360x240 show=1)] applied=[(120,140 360x240 show=1),(520,180 360x240 show=1)] externally_changed=[(1180,650 360x240 show=1),(520,180 360x240 show=1)] after_finish=[(1180,650 360x240 show=1),(900,390 360x240 show=1)] details=["pc_custom_core-5d0942c7d3c97d46.exe（2）: セッション中に外部から移動されたため、その位置を上書きしませんでした"]

cargo test --lib
test result: ok. 328 passed; 0 failed; 63 ignored; 0 measured; 0 filtered out

cargo test --lib count_report -- --nocapture
総数=71 変更できる(persistent+session)=16 確認のみ=9 表示専用=39 設定案内=6

cargo clippy --all-targets -- -D warnings
Finished dev profile

cargo fmt --check
exit code 0

npm run build
✓ 61 modules transformed
✓ built（CSS構文警告0件）
```

## 画面共有の前と後（2026-07-29）

- モード画面に会議前後の専用パネルを追加した。項目は
  「このアプリが自動で確認・変更したもの」「利用者自身に確認してもらうもの」
  「確認できないもの」の3区分で、総合点や一括の完了表示は作っていない。
- 自動変更は既存の `setup.window_layout` と `session.prevent_sleep` の2件だけ。
  固定要求を既存の `preview -> commit -> journal` へ渡し、終了時は記録したitemを逆順に戻す。
  Actionは追加しておらず **71件のまま**。
- 実行中のitem参照は `share-session.json` へ上限付き・未知field拒否で耐久保存する。
  復元前にtransaction ID、item ID、Action IDの対応をjournalで再確認する。
  各itemの復元後に残りを保存するため、途中終了後も残件だけを続行できる。
- 会議中に外部移動された窓は既存の配置Actionが上書きせず、理由を終了結果へ出す。
  マイクと既定音声は既存の独立probeで現在値だけを表示し、変更しない。
  通知は固定のWindows設定ページへ案内し、Teams、Zoom、ブラウザーなどの状態は
  「確認できないもの」へ分離した。
- 画面文言の3区分と、誤解を招く指定語句が含まれないことを単体テストで固定した。

実機ignored test:

```text
EVIDENCE: share_session item=sleep measured=true during_active=true after_active=false reason=independent_lease_snapshot_before_and_after
EVIDENCE: share_session item=window_layout measured=true desired=[(120,140 360x240 show=1),(520,180 360x240 show=1)] before=[(460,360 360x240 show=1),(900,390 360x240 show=1)] applied=[(120,140 360x240 show=1),(520,180 360x240 show=1)] externally_changed=[(1180,650 360x240 show=1),(520,180 360x240 show=1)] after=[(1180,650 360x240 show=1),(900,390 360x240 show=1)] reason=coordinates_read_by_separate_process
EVIDENCE: share_session item=microphone measured=true muted=false reason=windows_default_comms_input_only_meeting_app_delivery_not_measured
EVIDENCE: share_session item=audio_output measured=true endpoints=8 default_exists=true reason=windows_default_output_only_meeting_app_route_not_measured
EVIDENCE: share_session item=notifications measured=false reason=no_general_probe_for_priority_or_app_notifications
```

完了コマンド:

```text
cargo test --lib -- --test-threads=1
test result: ok. 334 passed; 0 failed; 65 ignored; 0 measured; 0 filtered out

cargo test --lib presentation::count_report::dump_action_counts -- --exact --nocapture --test-threads=1
総数=71 変更できる(persistent+session)=16 確認のみ=9 表示専用=39 設定案内=6

cargo clippy --all-targets -- -D warnings
Finished dev profile

cargo fmt --check
exit code 0

npm run build
✓ 63 modules transformed
✓ built（CSS構文警告0件）
```

## 場面ごとの既定プリンター（2026-07-30）

- `session.default_printer` を session Action として追加した。登録 Action は **72件**、
  変更できる Action は **17件**。場面名と、`EnumPrintersW` で列挙した
  インストール済みプリンターを利用者が明示的に選ぶ。GPS・ネットワークによる
  場所推測、プリンター／ドライバー追加、印刷設定の一括変更、印刷は行わない。
- `GetDefaultPrinterW` で開始前の正確な名前を durable backup に保存し、
  `SetDefaultPrinterW` 後に再読する。終了時は現在値が適用値のままで、元の名前が
  まだ列挙できる場合だけ元へ戻す。途中の外部変更は `ExternalConflict`、
  元のプリンター消失は `RecoveryRequired` として上書きしない。
- Windows の「通常使うプリンターをWindowsで管理する」が有効な場合は、
  その設定を変更せず `PolicyManaged` として画面に理由を出す。
- プリンター名と場面名は `Debug` で常に redaction する。画面と durable backup
  以外の診断文へ入れず、実機 `EVIDENCE:` も同一／別の判定と件数だけを出す。
- 実機テストの適用後／復元後 readback は、別テストプロセスの
  `PRINTDLGEXW + PD_RETURNDEFAULT` を使う。印刷ジョブは作らない。

実機 ignored test:

```text
EVIDENCE: default_printer current_present=true installed_count=1 windows_managed=true
EVIDENCE: default_printer measured=false reason=windows_manages_default no_change=true
```

この実機は Windows 自動管理が有効だったため、タスク指定どおりそこで測定不能として
終了し、既定値を変更していない。

完了コマンド:

```text
cargo test --lib -- --test-threads=1
test result: ok. 340 passed; 0 failed; 67 ignored; 0 measured; 0 filtered out

cargo test --lib request_round_trip -- --test-threads=1
test result: ok. 2 passed; 0 failed

cargo test --lib category_contract -- --test-threads=1
test result: ok. 1 passed; 0 failed

cargo test --lib count_report -- --nocapture --test-threads=1
総数=72 変更できる(persistent+session)=17 確認のみ=9 表示専用=39 設定案内=6

cargo clippy --all-targets -- -D warnings
Finished dev profile

cargo fmt --check
exit code 0

npm run build
✓ 63 modules transformed
✓ built（CSS構文警告0件）
```

## モデル4種を同じ監査課題に当てた（2026-07-30 実測）

同じリポジトリ（`3e496f8`）、同じプロンプト、読み取り専用。並列実行。

| モデル | 指摘 | 本物 | 誤り | 備考 |
|---|---|---|---|---|
| codex gpt-5.6-sol high | 11 | 10 | 1 | 自分の書いたコードを自己監査 |
| gemini-3.1-pro-high（1回目） | 3 | 2 | 1 | `window_color` のロールバック分割を発見 |
| gemini-3.1-pro-high（2回目・修正後） | 0 | – | – | `NO FINDINGS` |
| claude-opus-4-6-thinking | 24 | 1 | 多数 | **引用行の多くが存在しない** |
| gpt-oss-120b-medium | 8 | 0 | 8 | **コードではなく STATUS.md の過去記録を読んで報告** |

### Opus の 24 件について

`src/catalog.ts:1498`（実際は1234行）、`src-tauri/src/presentation.rs:2789 / 2847`（実際は2191行）。
根拠にした `canChange` というフィールドも、`demoted_actions_refuse_to_mutate` というテストも
**このリポジトリに存在しない。** 同じ指摘の重複も5組あった。

ただし1件だけ実在の欠陥を当てた（下記）。**24件中1件。**

### gpt-oss の 8 件について

すべて `docs/STATUS.md` の行番号を引用していた。
過去に自分たちが記録した「実測して効かないと分かった」項目を、
そのまま新規の欠陥として報告し直したもの。**コードを読んでいない。**

### 結論

**数と当たりは比例しない。** 24件出したモデルの的中は1件、0件と答えたモデルは前回2件当てている。

有効だったのは「**別系統を当てると盲点が割れる**」ことのほう。
codex が自分のコードを監査して見つけられなかった `window_color` の分割ロールバックを Gemini が当て、
codex も Gemini も見つけられなかった補償の握り潰しを Opus が当てた。
どれも1件ずつだが、どれも戻せなさに直結する欠陥だった。

**引用行が実在するかを最初に確かめること。** 存在しない行を根拠にした指摘が、
今回いちばん多かった不良の形だった。

## 補償の結果を捨てていた（2026-07-30）

`window_color` の apply は2つのレジストリ値を順に書く。
2つ目で失敗したとき、1つ目を巻き戻していたが、その結果を `let _ =` で捨てていた。

巻き戻しに失敗しても呼び出し側には元のエラーだけが返る。
**「適用に失敗した」は「何も変わっていない」と読まれる。** 実際には片方が変わったまま残る。

今は書き戻したあと読み直し、元の値に戻ったことを確かめる。
確かめられなければ、失敗ではなく `RecoveryRequired` を返す。

今日 `power_mode` で同じ形を直したばかりだった。**同じ形は他にもある前提で探すべきだった。**

## 12/24時間表示は書けるが表示が動かない（2026-07-30 実測）

`docs/RESEARCH_FEATURES_ROUND3.md` の候補5を実装させ、実機で測って**出荷を取りやめた。**

短い時刻書式の値はレジストリに書けて、読み直しでも変わる。だが外から見た表示は動かない。

```
before   short_time=H:mm      child_probe=13:05:00  taskbar=時計 17:33
applied  short_time=tt h:mm   child_probe=13:05:00  taskbar=時計 17:33
```

12時間表記を適用した直後のタスクバーが `17:33`。**24時間のまま。**
別プロセスで固定時刻 13:05 を整形しても `13:05:00` のまま。

`separate_process`、`info_tips` と同じ形。**設定値の readback は効果の証拠にならない。**

### 危なかったところ

最初の実装は、この3状態すべてで `child_probe=13:05:00` と並べたうえで**緑で「成功」と報告**していた。
値が動かない観測を証拠として並べても、何も証明しない。

さらに `GetTimeFormatEx` が失敗したとき、フォールバックで `"13:05"` という
**本物らしい文字列**を返していた。整形できなかったことが「24時間表記だった」という
観測に化ける。呼び出し側は文字列を比べるので区別がつかない。

自分で入れた最初の assert も甘かった。`taskbar_clock_measured`——つまり
**「時計の要素を読めたか」で判定していて、「変わったか」を見ていなかった。**
24時間表記の `時計 17:32` を読めただけで合格していた。
適用前のタスクバーも取って差分で判定したら、正しく落ちた。

**「読めた」は「変わった」ではない。** 今日この形で2回間違えている。

## コントラストテーマ試用は exact rollback できず取りやめ（2026-07-30 実測）

`tasks/TASK_CONTRAST_TRIAL.md` の指示どおり、Action より先に
`src-tauri/src/windows/ui_probe.rs` へ実機計器だけを作った。

- 別テストプロセスに標準 Win32 の button / edit / command-link を出す。
- 自己所有の topmost 窓だけを `BitBlt` で撮り、コントロールの実矩形内で
  最頻の面ピクセルと最も比が高い前景ピクセルを採る。
- `HIGHCONTRASTW` は `cbSize`、全 flags、scheme の NULL / 空文字 / 文字列を区別して保存する。
- `SPI_SETHIGHCONTRAST` 後の同じ HWND と、復元後に新しく作った別プロセス窓を測る。
- 適用後は Windows の遅延正規化が落ち着いてから自己適用 snapshot を確定し、
  その snapshot と現在値が一致する場合だけ復元する。

最初の実行では、開始前は `flags=126`、scheme は `NULL` だった。適用すると
`flags=127` になり、Windows は scheme を最終的に `ハイコントラスト 黒` へ正規化した。
適用直後の実画面は遷移中の白一色で、同じ窓の有効な前景ピクセルをまだ採れなかった。

```text
EVIDENCE: contrast_trial measured=false reason=no_round_trip_scheme flags=126
EVIDENCE: contrast_trial measured=false reason=applied_pixels_unavailable before="background=#FFFFFF button=#F0F0F0 input=#FFFFFF foreground=#000000 contrast_ratio=18.427" spi_enabled=true detail=button foreground was not visible in screenshot: ratio=1.000 surface=#FFFFFF foreground=#FFFFFF window_rect=RECT { left: 180, top: 140, right: 820, bottom: 500 } button_rect=RECT { left: 220, top: 215, right: 400, bottom: 257 }
EVIDENCE: contrast_trial emergency_restored=HighContrastSnapshot { structure_size: 16, flags: 126, scheme_name: Some("ハイコントラスト 黒") }
EVIDENCE: contrast_trial current_read_only ok=True enabled=false flags=126 scheme=ハイコントラスト 黒
```

high-contrast の有効 flag は元の無効状態へ戻った。しかし scheme は `NULL` へ戻らず、
`ハイコントラスト 黒` が残った。つまり **開始前に保存した全フィールドと scheme 名へ戻す**
という必須条件を満たさない。同じ機械で再実行すると、汚染後の
`Some("ハイコントラスト 黒")` を開始値にして見かけ上 round-trip する可能性があるため、
その値を成功証拠にはしない。

**結論: 測れなかった。Action は実装・登録しない。登録数は72件のまま。**
設定値の readback や `SPI_GETHIGHCONTRAST` の enable flag は成功判定にしていない。
初回状態を完全復元できる公式経路と、遷移後の同一窓ピクセルを安定して採れる環境の
両方が得られるまで不採用とする。

完了コマンド:

```text
cargo test --lib -- --test-threads=1
test result: ok. 340 passed; 0 failed; 69 ignored; 0 measured; 0 filtered out

cargo clippy --all-targets -- -D warnings
Finished dev profile

cargo fmt --check
exit code 0

npm run build
✓ 63 modules transformed
✓ built（CSS構文警告0件）
```

## 高コントラストは「比が上がる」わけではない（2026-07-30 実測）

候補7の測定を作って実機で当てた。**外から確かめられる。** 今日取りやめた3件と違う。

```
before   background=#FFFFFF foreground=#000000  ratio=18.427  spi_flags=126
applied  background=#202020 foreground=#FFFFFF  ratio=16.293  spi_flags=127
restored background=#FFFFFF foreground=#000000  ratio=18.427  spi_flags=126
```

別プロセスの窓のピクセルが動き、戻すと**元の値へ正確に戻る**。`SPI_GETHIGHCONTRAST` の
フラグも 126→127→126 と別経路で一致する。設定値の readback ではなく、見た目の観測。

### 名前に引きずられて合格条件を間違えた

最初の判定は「コントラスト比が上がること」を合格条件にしていた。**測ったら下がった。**

白地に黒（18.427）のほうが、黒地に白（16.293）より数値上のコントラストは高い。
「高コントラスト」という名前から上がると思い込んでいたが、逆だった。
そのため**正しく動いている機能が不合格になっていた。**

合格条件は「ピクセルが変わること」と「元へ正確に戻ること」に直した。
比は数値として出すだけで、向きを判定に使わない。

**この機能が約束できるのは「見え方が変わる」と「正確に戻る」まで。**
読みやすくなるかは本人にしか分からない。研究文書にもそう書いてある。

## コントラストテーマ30秒試用を出荷可能にした（2026-07-30）

- `appearance.high_contrast_trial` を登録した。登録 Action は **73件**、
  変更できる Action は **18件**。
- 製品側も `SystemParametersInfoW` だけを使い、`HIGHCONTRASTW` の構造体サイズ、
  全 flags、scheme の NULL / 空文字 / 文字列を区別して durable backup に保存する。
  適用後は Windows の scheme 正規化が落ち着いてから実状態の fingerprint を記録する。
- 復元時は現在の全フィールドが自分の適用 fingerprint と一致する場合だけ開始前の
  snapshot を書く。違えば `ExternalConflict` とし、復元後の全フィールドが一致しなければ
  `RecoveryRequired` とする。
- 開始時点で既に有効なら書かず、画面に「出番なし」と出す。無効かつ scheme が
  NULL / 空文字の場合は、過去の実測で exact rollback できなかったため一度も書かない。
- 30秒の期限は既存の preview → commit → journal に保存する。ネイティブ側が500ms間隔で
  期限切れ journal を復元し、画面側も期限到達時に同じ固定コマンドを呼ぶ。
  この Action を含む trial はコア側でも確定保存を拒否する。
- 出荷文言と静的カタログの両方に、未測定の効果を表す7語の禁止語テストを置いた。
- 既存計器は判定内容を変えていない。復元直後のテーマ遷移中に青いボタン面を拾うことが
  2回続いたため、適用側と同じ1.5秒の安定待ちだけを復元側にも置いた。最終実測:

```text
EVIDENCE: contrast_trial measured=true reason=separate_process_pixels_changed_and_restored
before  background=#FFFFFF foreground=#000000 contrast_ratio=18.427
applied background=#202020 foreground=#FFFFFF contrast_ratio=16.293
restored background=#FFFFFF foreground=#000000 contrast_ratio=18.427
spi flags=126 -> 127 -> 126
scheme=Some("ハイコントラスト 黒") -> Some("ハイコントラスト 黒") -> Some("ハイコントラスト 黒")
```

完了コマンド:

```text
cargo test --lib -- --test-threads=1
test result: ok. 344 passed; 0 failed; 69 ignored; 0 measured; 0 filtered out

cargo test --lib request_round_trip -- --test-threads=1
test result: ok. 2 passed; 0 failed

cargo test --lib category_contract -- --test-threads=1
test result: ok. 1 passed; 0 failed

cargo test --lib count_report -- --nocapture --test-threads=1
総数=73 変更できる(persistent+session)=18 確認のみ=9 表示専用=39 設定案内=6 一方向=1

cargo test --lib high_contrast_changes_separate_process_pixels_and_restores -- --ignored --nocapture --test-threads=1
test result: ok. 1 passed; 0 failed

cargo clippy --all-targets -- -D warnings
Finished dev profile

cargo fmt --check
exit code 0

npm run build
✓ 63 modules transformed
✓ built（CSS構文警告0件）
```
