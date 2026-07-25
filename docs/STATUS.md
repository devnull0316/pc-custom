# Totonoe — 現況と再開手順（CC記録 2026-07-24）

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

- 安定Actionは値の欠如/型/raw bytesを型付きbackupへ保存し、rollbackは現在値がTotonoeの適用値と一致する場合だけ元状態へ戻す。
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
| **ネイティブウィンドウの生成** | `tasklist /FI "WINDOWTITLE eq Totonoe"` が一致。`tasklist /V` の Title 列も `Totonoe` → WebView2ウィンドウが実際に生成されている |
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

### 出荷中のActionを同じ方法で検査したら、2件に実害が見つかった

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

## 42候補の行き止まりを解消（CC 2026-07-25）

実測でUIへ反映されないと分かった以上、これらを「表示するだけ」で終わらせない。**Windows自身の設定画面へ案内する導線**を追加した（`src/settings_link.rs`）。

- ActionId → `ms-settings:` ページの**固定表**のみ。任意URL・パス・コマンドは受け付けない。
- 起動は `ShellExecuteW` に定数文字列を渡すだけで、シェルを経由しない。
- 設定アプリに該当ページが無いもの（Explorer系はフォルダーオプション側）は**案内を出さない**。嘘の導線を作らないため。
- UIには「Windowsの設定を開く」ボタンと、「この項目はWindowsの設定画面から変更できます。Totonoeは現在値の表示だけを行います」の一文を出す。
- テスト4件: 全マッピングが `ms-settings:` スキームで空白・引数連結・引用符を含まないこと、Explorer系は案内なし、主要候補には必ず導線があること、表示情報に `settingsPage` が載ること。

これで42候補は「変えられないと言われて終わり」ではなく、「ここで変えられます」と示す状態になった。

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
