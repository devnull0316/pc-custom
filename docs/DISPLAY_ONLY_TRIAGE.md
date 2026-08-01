# 表示専用 Action 39件の仕分けレポート

`BRIEF.md` および `docs/RULES.md` の制約に基づき、`src-tauri/src/action/registry.rs` に登録されている `MethodClass::UnverifiedStorage` の Action 39件について、`src-tauri/src/windows/ui_probe.rs` のテストコードの中身を直接読んだ上で、外部観測の実現性と難易度を仕分けした。

---

## 仕分け表（全39件）

| Action ID | 実測済みか | 測るなら何を見るか | 難易度 |
|---|---|---|---|
| `start.layout` | なし | スタートメニューを展開し、UIAでピン留め領域とおすすめ領域の高さ/バウンディングボックス比率を観測 | 中 |
| `start.recommendations` | なし | スタートメニューを展開し、おすすめ領域内のリスト項目要素の個数または表示状態をUIAで観測 | 中 |
| `explorer.launch_target` | なし | 新規 `explorer.exe` を引数なしで起動し、最初に開いたウィンドウのタイトル（「ホーム」「PC」「ダウンロード」等）またはアドレスバーのパスを観測 | 小 |
| `explorer.recent_files` | なし | 新規 `explorer.exe` を「ホーム」表示で開き、UIAで「最近」セクションのリスト項目数が0か1以上かを観測 | 中 |
| `taskbar.button_grouping` | なし | 同一アプリのウィンドウを複数起動した状態で、タスクバー（`MSTaskListWClass`）上の該当アプリボタンが1個に結合されているか個別表示されているかをUIAで数え上げ | 中 |
| `taskbar.flashing` | なし | 不可。点滅（`FlashWindowEx`）発生時のタスクバーボタンの過渡的な明滅現象を、非同期な自動テストで決定論的にピクセル/UIA捕捉する手段がないため | 不可 |
| `taskbar.share_window` | なし | 不可。Teams/Zoom等の対応サードパーティ会議アプリでアクティブ通話中にタスクバーサムネイルへホバーした際に出る「共有」オーバーレイボタン。自動テスト環境に特定通話状態を用意できないため | 不可 |
| `search.recent_on_hover` | なし | タスクバーの検索アイコン領域にカーソルを移動（`SetCursorPos`）させ、検索フライアウト画面（`SearchPane`）が自動ポップアップするかをウィンドウ/UIAで検出 | 中 |
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
| `explorer.drive_letters` | なし | 新規 Explorer で「PC」を開き、ドライブ表示要素のUIA名前文字列が「ローカル ディスク (C:)」等のドライブ文字付与パターンを満たしているか判定 | 小 |
| `explorer.preview_handlers` | なし | プレビューペインを有効にした新規 Explorer でファイルを選択した際、プレビュー領域内に描画コントロール（`PreviewPane`）が出現するか観測 | 中 |
| `explorer.sharing_wizard` | なし | フォルダーのコンテキストメニュー等から「共有」を選択した際、出現するダイアログが「共有ウィザード」（`SharingWizard`）か従来のプロパティかを識別 | 中 |
| `explorer.always_show_menus` | あり（試行済み・判定不可と確認） | `batch_measure_explorer_candidates` で実測試行されたが、高さ測定手法では判定不能と判明。測定するなら新規 Explorer 窓内で Classic MenuBar UIA 要素の有無・可視性を捉える専用プローブが必要 | 中 |
| `appearance.taskbar_animations` | なし | 不可。タスクバーボタンのホバー/開閉時のアニメーションフレーム（数ミリ秒単位の描画遷移）のコマ数を外部からキャプチャで決定論的に測定する手段がないため | 不可 |
| `notifications.toast_banners` | なし | テスト用トースト通知（WinRT `ToastNotificationManager` 等）を発行し、画面右下にトースト通知ウィンドウ（`ToastNotificationPopup`）が出現するかを判定 | 中 |

---

## 証拠の位置と既知の実測結果（`ui_probe.rs` / `STATUS.md`）

1. **`explorer.info_tips`**
   - **実測テスト**: `ui_probe.rs:L1483` `info_tip_setting_changes_owned_explorer_tooltip_visibility`
   - **証拠内容**: `set_shell_state_show_info_tip` (文書化API `SHGetSetSettings` / `SSF_SHOWINFOTIP`) 経由で設定を一時変更し、自己所有の Explorer 窓内で Cursor をファイル中央へ動かし UIA (`UIA_ToolTipControlTypeId`) の出現をカウントした。
2. **`explorer.separate_process`**
   - **実測テスト**: `ui_probe.rs:L1626` `separate_process_setting_changes_owned_explorer_window_process_pattern`
   - **証拠内容**: `set_shell_state_separate_process` (文書化API `SHGetSetSettings` / `SSF_SEPPROCESS`) 経由で変更し、新規に開いた自己所有 Explorer 窓の PID と Shell の PID を比較してプロセス分離を検証した。
3. **`taskbar.show_desktop`**
   - **実測テスト**: `ui_probe.rs:L2920` `batch_measure_taskbar_candidates_after_shell_restart`
   - **証拠内容**: `TaskbarSd` レジストリを変更して `restart_shell()` を実行後、タスクバー上の UIA 要素 `"デスクトップを表示する"` の有無を観測した。
4. **`explorer.status_bar` 及び `explorer.always_show_menus`**
   - **実測テスト**: `ui_probe.rs:L2422` `batch_measure_explorer_candidates`
   - **証拠内容**: レジストリ変更＋新規 Explorer 窓の一覧高さ測定等を行ったが、コード内に `println!("この経路では判定できない...観測手段を作り直すこと");` と明記されており、現行プローブ手法では「測れていない（判定不可）」と実測証明されている。

---

## 集計結果

- 実測済み（効かない・判定不可確定含む）: 5件
- 測れる見込み（小）: 4件 ← **次に測る候補**
- 測れる見込み（中）: 21件
- 測る手段が無い: 14件
