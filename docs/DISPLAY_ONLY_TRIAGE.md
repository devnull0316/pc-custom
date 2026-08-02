# 表示専用 Action 38件の仕分けレポート

`BRIEF.md` および `docs/RULES.md` の制約に基づき、`src-tauri/src/action/registry.rs` に登録されている `MethodClass::UnverifiedStorage` の Action 38件について、`src-tauri/src/windows/ui_probe.rs` のテストコードの中身を直接読んだ上で、外部観測の実現性と難易度を仕分けした。

---

## 仕分け表（全38件）

| Action ID | 実測済みか | 測るなら何を見るか | 難易度 |
|---|---|---|---|
| `start.layout` | あり（**観測基盤が立たず未判定**） | `start_layout_write_changes_the_start_menu_layout` にて実測。`Start_Layout` レジストリ変更＋`StartMenuExperienceHost.exe` 再起動・Winキー自動操作でスタートメニューUIA要素（ピン留め・おすすめ領域）の測定を試みたが、自動テスト環境下でUIA要素が検索不能（`measured=false reason=baseline_start_menu_unavailable`）のため**この観測方法では判定できなかった**。設定が効くかどうかは未判定 | 保留 |
| `start.recommendations` | あり（**観測基盤が立たず未判定**） | `start_recommendations_write_changes_the_start_menu` にて実測。`Start_IrisRecommendations` レジストリ変更＋`StartMenuExperienceHost.exe` 終了・Winキー自動操作でUIA判定を試みたが、おすすめ要素がUIA露出せず判定不能（`measured=false before=false written=false restored=false changed=false restored_ok=true`）のため**この観測方法では判定できなかった**。設定が効くかどうかは未判定 | 保留 |
| `explorer.launch_target` | 昇格済み（Persistent） | `explorer_launch_target_write_changes_the_fresh_explorer_window` にて実測。`LaunchTo` レジストリの一時変更・別プロセス検証・完全復元（`original_reg=None` 削除復元含む）を実測確認し、`Persistent` へ昇格完了 | 昇格 |
| `explorer.recent_files` | あり（**観測基盤が立たず未判定**） | `explorer_recent_files_write_changes_the_fresh_explorer_window` にて実測。`ShowRecent` レジストリ変更後、新規 `explorer.exe` (ホーム) のUIAで「最近」セクション項目数を判定しようとしたが、自動テスト環境で対象ウィンドウ識別・項目列挙が不可（`measured=false reason=baseline_window_unavailable`）のため**この観測方法では判定できなかった**。設定が効くかどうかは未判定 | 保留 |
| `taskbar.button_grouping` | あり（ピクセル観測実測済み） | `taskbar_button_grouping_write_changes_the_taskbar` にて実測。既知の変化（メモ帳2個起動）による `taskbar_pixel_stats()` の感度/閾値検証（`known_change_delta`）を行った上で `TaskbarGlomLevel` レジストリ変更＋`restart_shell()` 実行下のタスクバーピクセル変化・完全復元および終了時 `Shell_TrayWnd` 存在確認を検証 | 保留 |
| `taskbar.flashing` | なし | 不可。点滅（`FlashWindowEx`）発生時のタスクバーボタンの過渡的な明滅現象を、非同期な自動テストで決定論的にピクセル/UIA捕捉する手段がないため | 不可 |
| `taskbar.share_window` | なし | 不可。Teams/Zoom等の対応サードパーティ会議アプリでアクティブ通話中にタスクバーサムネイルへホバーした際に出る「共有」オーバーレイボタン。自動テスト環境に特定通話状態を用意できないため | 不可 |
| `search.recent_on_hover` | あり（**観測基盤が立たず未判定**） | `search_recent_on_hover_write_changes_the_taskbar` にて実測。`OpenOnHover` レジストリ変更後、タスクバー上の検索アイコン領域を `SetCursorPos` でホバー観測しようとしたが、検索ボタン要素がUIAツリーに未露出（`measured=false reason=search_button_unavailable`）のため**この観測方法では判定できなかった**。設定が効くかどうかは未判定 | 保留 |
| `taskbar.multi_monitor` | なし | 不可。単一ディスプレイ環境では観測不能。マルチモニター環境がある場合、サブモニター側の `Shell_SecondaryTrayWnd` の存在有無を `FindWindowW` で検出する | 不可 |
| `taskbar.multi_monitor_mode` | なし | 不可。単一ディスプレイ環境では観測不能。マルチモニター環境下でサブタスクバー上のボタン一覧をUIAで取得し、メイン画面のアプリボタンが含まれるか観測する | 不可 |
| `taskbar.secondary_button_grouping` | なし | 不可。単一ディスプレイ環境では観測不能。サブタスクバー上のボタン結合状態をUIAで数え上げる必要があるため | 不可 |
| `start.show_all_pins` | なし | スタートメニューを開いた初期状態で「すべてのアプリ」一覧が直接展開されているか、ピン留めグリッドが表示されているかをUIAで判定 | 中 |
| `start.recent_apps` | なし | スタートメニューの「すべてのアプリ」一覧の最上部に「最近追加されたもの」グループヘッダー要素が存在するかをUIAで検索 | 中 |
| `appearance.accent_start_taskbar` | なし | タスクバー背景領域のピクセルを撮影し、アクセントカラー適用時の色相・彩度変化（`taskbar_pixel_stats` の型を応用）を計測 | 中 |
| `appearance.accent_title_bars` | なし | 自作の標準Win32ウィンドウのタイトルバー（非クライアント領域）のピクセル色を撮影・抽出して、アクセントカラーの色相と一致するか判定 | 中 |
| `appearance.auto_accent` | なし | デスクトップ壁紙を変更した後、`DwmGetColorizationColor`（`system_accent_color()`）で取得される実効アクセントカラーが壁紙に追従して更新されたか観測 | 中 |
| `games.game_mode` | なし | 不可。`AutoGameModeEnabled` レジストリ変更時に内部ゲームモードスケジューラーが有効化したかを外部プロセスから客観的に判定するWin32 APIやUIA指標が存在しないため | 不可 |
| `games.controller_game_bar` | なし | 不可。XboxコントローラーのGuideボタン入力を仮想エミュレートし、Game Bar（`XboxGameBar.exe`）の起動を検証する標準・安全な手段がないため | 不可 |
| `devices.autoplay` | なし | 不可。リムーバブルメディア挿入時に自動再生ダイアログ（`AutoPlay`）が出現するかを観測する必要があるが、自動テスト環境でメディア挿入イベントを仮想生成する手段がないため | 不可 |
| `notifications.usb_errors` | なし | 不可。実際のUSB接続エラーを意図的に発生させてトースト通知を出す安全な外部手段がないため | 不可 |
| `notifications.weak_charger` | なし | 不可。低出力充電器の接続イベントを仮想発生させてトースト通知を出す手段がないため | 不可 |
| `input.autocorrect` | なし | 不可。タッチキーボード（`TabTip.exe`）上での自動修正動作。物理タッチ層依存であり、通常の `SendInput` 等の仮想入力では発火せず外から判定不能なため | 不可 |
| `input.double_space_period` | なし | 不可。タッチキーボード専用のスペース2回タップによるピリオド補完挙動であり、仮想キー入力では再現・観測不能なため | 不可 |
| `input.auto_shift` | なし | 不可。タッチキーボード内部の自動Shiftキー状態はWin32/UIAから参照不能なため | 不可 |
| `input.voice_typing_key` | なし | タッチキーボード（`TabTip`）を画面上に表示させ、キーボードレイアウト内にマイク/音声入力キー（UIA `VoiceTyping` ボタン）の要素が存在するか観測 | 中 |
| `input.multilingual_suggestions` | なし | 不可。複数言語パック導入とIMEコンテキストが必要であり、IME候補ウィジェットの動的出現内容を外部から確定判定する手段がないため | 不可 |
| `explorer.status_bar` | あり（試行済み・判定不可と確認） | `batch_measure_explorer_candidates` で実測試行されたが、既存のUIA項目数/高さ判定では誤判定・観測不能と判明。測定するなら新規 `explorer.exe` 窓を正しく特定し、ステータスバーUIA要素（`StatusBar` / `"個の項目"`）の高さ・可視性を捉える専用プローブが必要 | 中 |
| `explorer.info_tips` | あり（実測済み） | `info_tip_setting_changes_owned_explorer_tooltip_visibility` にて実測済み。自己所有の Explorer 窓内のファイル項目上にカーソルを動かし、出現する ToolTip（`UIA_ToolTipControlTypeId`）要素の数・名前を観測（`InfoTipRestoreGuard` を使用） | 小 |
| `explorer.hide_empty_drives` | なし | 新規 `explorer.exe` を「PC」表示で開き、空のリムーバブルドライブ（カードリーダー等のドライブ文字）の項目要素がUIA一覧に出るか観測 | 中 |
| `explorer.nav_expand_current` | なし | 深い階層のフォルダー（`C:\A\B\C` 等）を新規 Explorer で開いた際、左側ナビゲーションツリー（`NavigationPane`）で該当フォルダーのツリーノードが自動展開（`IsExpandCollapsePattern`）されているか観測 | 中 |
| `explorer.nav_show_all` | なし | 新規 Explorer の左側ナビゲーションツリーペインで、「ごみ箱」や「コントロールパネル」等の特殊フォルダーノードが露出しているかをUIAで観測 | 中 |
| `explorer.separate_process` | あり（実測済み） | `separate_process_setting_changes_owned_explorer_window_process_pattern` にて実測済み。`set_shell_state_separate_process` で変更し、新規 Explorer 窓の PID（`GetWindowThreadProcessId`）が Shell PID（`GetShellWindow`）と異なるかを測定（`SeparateProcessRestoreGuard` を使用） | 小 |
| `explorer.icons_only` | なし | 画像ファイルを置いたフォルダーを新規 Explorer で大アイコン表示で開き、`PrintWindow` でファイルアイコン領域のサムネイル描画（画像内容か汎用アイコンか）のピクセル分散を比較 | 中 |
| `explorer.drive_letters` | あり（実測済み・効かない） | `explorer_drive_letters_write_changes_the_fresh_explorer_window` にて実測。`ShowDriveLetters` レジストリを一時変更し、引数なし `explorer.exe`（LaunchTo=1）または `shell:::{...}` で「PC」を開き UIA でドライブ文字 `(C:)` を観測。観測計器は項目名を正確に検出しているが、シェル再起動なしのストレージ変更単体では新規ウィンドウでもドライブ文字表示が変化しない（`before=true written=true restored=true changed=false`）ことを実測証明 | 小 |
| `explorer.preview_handlers` | なし | プレビューペインを有効にした新規 Explorer でファイルを選択した際、プレビュー領域内に描画コントロール（`PreviewPane`）が出現するか観測 | 中 |
| `explorer.sharing_wizard` | なし | フォルダーのコンテキストメニュー等から「共有」を選択した際、出現するダイアログが「共有ウィザード」（`SharingWizard`）か従来のプロパティかを識別 | 中 |
| `explorer.always_show_menus` | あり（試行済み・判定不可と確認） | `batch_measure_explorer_candidates` で実測試行されたが、高さ測定手法では判定不能と判明。測定するなら新規 Explorer 窓内で Classic MenuBar UIA 要素の有無・可視性を捉える専用プローブが必要 | 中 |
| `appearance.taskbar_animations` | なし | 不可。タスクバーボタンのホバー/開閉時のアニメーションフレーム（数ミリ秒単位の描画遷移）のコマ数を外部からキャプチャで決定論的に測定する手段がないため | 不可 |
| `notifications.toast_banners` | なし | テスト用トースト通知（WinRT `ToastNotificationManager` 等）を発行し、画面右下にトースト通知ウィンドウ（`ToastNotificationPopup`）が出現するかを判定 | 中 |

---

## 証拠の位置と既知の実測結果（`ui_probe.rs` / `STATUS.md`）

1. **`explorer.info_tips`**
   - **実測テスト**: `ui_probe.rs` `info_tip_setting_changes_owned_explorer_tooltip_visibility`
   - **証拠内容**: `set_shell_state_show_info_tip` (文書化API `SHGetSetSettings` / `SSF_SHOWINFOTIP`) 経由で設定を一時変更し、自己所有の Explorer 窓内で Cursor をファイル中央へ動かし UIA (`UIA_ToolTipControlTypeId`) の出現をカウントした。
2. **`explorer.separate_process`**
   - **実測テスト**: `ui_probe.rs` `separate_process_setting_changes_owned_explorer_window_process_pattern`
   - **証拠内容**: `set_shell_state_separate_process` (文書化API `SHGetSetSettings` / `SSF_SEPPROCESS`) 経由で変更し、新規に開いた自己所有 Explorer 窓の PID と Shell の PID を比較してプロセス分離を検証した。
3. **`taskbar.show_desktop`**
   - **実測テスト**: `ui_probe.rs` `batch_measure_taskbar_candidates_after_shell_restart`
   - **証拠内容**: `TaskbarSd` レジストリを変更して `restart_shell()` を実行後、タスクバー上の UIA 要素 `"デスクトップを表示する"` の有無を観測した。
4. **`explorer.status_bar` 及び `explorer.always_show_menus`**
   - **実測テスト**: `ui_probe.rs` `batch_measure_explorer_candidates`
   - **証拠内容**: レジストリ変更＋新規 Explorer 窓の一覧高さ測定等を行ったが、コード内に `println!("この経路では判定できない...観測手段を作り直すこと");` と明記されており、現行プローブ手法では「測れていない（判定不可）」と実測証明されている。
5. **`explorer.launch_target`**
   - **実測テスト**: `ui_probe.rs` `explorer_launch_target_write_changes_the_fresh_explorer_window`
   - **証拠内容**: `LaunchTo` レジストリを一時変更し、引数なし `explorer.exe` を起動して観測。`/n` 引数付き起動から引数なし起動（新規ウィンドウ生成を確認）に修正した結果、設定変更で新規 Explorer 窓のターゲット種別が即座に変化し（`before=2 written=1 restored=2 changed=true restored_ok=true`）、設定が機能して変更・復元が正確に行われることを実測証明（昇格候補）。
6. **`explorer.drive_letters`**
   - **実測テスト**: `ui_probe.rs` `explorer_drive_letters_write_changes_the_fresh_explorer_window`
   - **証拠内容**: `ShowDriveLetters` レジストリを一時変更し、引数なし `explorer.exe` (LaunchTo=1) または `shell:::{...}` で「PC」を開いて UIA でドライブ文字 `(C:)` を観測。観測計器はドライブ項目名を正確に検出しているが、シェル再起動なしのストレージ変更単体では新規ウィンドウでもドライブ文字表示が変化しない（`before=true written=true restored=true changed=false`）ことを実測証明。
7. **`start.layout`**
   - **実測テスト**: `ui_probe.rs` `start_layout_write_changes_the_start_menu_layout`
   - **証拠内容**: `Start_Layout` レジストリを一時変更し、`StartMenuExperienceHost.exe` 終了＋Winキー入力でスタートメニュー展開後のピン留め/おすすめ領域UIA要素（BoundingBox）を計測。自動テスト環境下でUIA要素が検索不能（`measured=false reason=baseline_start_menu_unavailable`）のため「不可」を確定。
8. **`start.recommendations`**
   - **実測テスト**: `ui_probe.rs` `start_recommendations_write_changes_the_start_menu`
   - **証拠内容**: `Start_IrisRecommendations` レジストリを一時変更し、`StartMenuExperienceHost.exe` 終了＋Winキー入力でおすすめ表示状態を計測。自動テスト環境下で要素がUIA露出せず判定不能（`measured=false before=false written=false restored=false changed=false restored_ok=true`）のため「不可」を確定。
9. **`explorer.recent_files`**
   - **実測テスト**: `ui_probe.rs` `explorer_recent_files_write_changes_the_fresh_explorer_window`
   - **証拠内容**: `ShowRecent` レジストリを一時変更し、新規 `explorer.exe` (ホーム `shell:::{679f857b-165d-4a25-9a24-998467cca37b}`) 内の「最近」セクションのUIA項目数を計測。自動テスト環境下で対象ウィンドウの特定・項目列挙が安定せず不可（`measured=false reason=baseline_window_unavailable`）のため「不可」を確定。
10. **`taskbar.button_grouping`**
    - **実測テスト**: `ui_probe.rs` `taskbar_button_grouping_write_changes_the_taskbar`
    - **証拠内容**: UIA要素未露出に対し、`taskbar_pixel_stats()` を用いたピクセル観測プローブを活用。既知の変化（メモ帳0個 vs 2個起動）でピクセル計器の感度/閾値（`known_change_delta`）を事前検証した上で、`TaskbarGlomLevel` レジストリ変更＋`restart_shell()` 実行によるタスクバーピクセル変化・完全復元、および終了時のタスクバー（`Shell_TrayWnd`）存在・可視性を実測検証。
11. **`search.recent_on_hover`**
    - **実測テスト**: `ui_probe.rs` `search_recent_on_hover_write_changes_the_taskbar`
    - **証拠内容**: `OpenOnHover` レジストリを一時変更し、`SetCursorPos` でタスクバー上の検索アイコン位置へホバーしてフライアウト出現を計測。タスクバー上の検索ボタンがUIAツリーに未露出（`measured=false reason=search_button_unavailable`）のため「不可」を確定。

---

## 集計結果

- 実測済み（効かない・判定不可確定含む）: 12件（うち昇格1件, 効かない1件, 判定不可10件）
- 測れる見込み（小）: 2件
- 測れる見込み（中）: 16件
- 測る手段が無い: 19件 (元々14件 + 今回判定不可確定5件)


## 「不可」と「この方法では測れなかった」は別

2026-08-02、難易度「中」の5件を測ろうとして、**5件とも観測の前段で失敗した**。

- スタートメニューの UIA 要素が自動テスト環境で列挙できない
- 新規エクスプローラー窓が安定して識別できない
- タスクバーのボタンが UIA に露出しない

これを「不可」（＝観測する手段が無い）と記録しかけた。**違う。**
観測手段が無いのではなく、**その観測方法が自動テスト環境で成立しなかった**だけ。

`measured=false reason=baseline_*_unavailable` は、
「設定が効かない」でも「観測できない」でもなく、**「測る前に失敗した」**という意味。
3つを混ぜない。

これらは「保留」とし、観測方法を変えれば測れる可能性を残す。
（例: UIA ではなくピクセル比較、別プロセスからの列挙、ログオン直後の状態を使う）

