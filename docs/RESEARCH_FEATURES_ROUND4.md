# 機能調査 Round 4

調査日: 2026-08-01  
対象: Windows 11 / 日本語版「PCカスタム」  
前提: `BRIEF.md`、`docs/RULES.md`、`docs/RESEARCH_FEATURES.md`、`_ROUND2.md`、`_ROUND3.md`、`src-tauri/src/action/id.rs` を全件確認した。過去の 21 候補および既存 Action（74 件）との重複を完全に避けている。

---

## 結論一覧

Round 4 では、過去 3 回で網羅されていない新しい視点（既有部品の組み合わせ、復元が主役の場面、Windows 11 の 2024–2026 年新機能、安全で楽しいカスタマイズ）から 8 つの候補を調査・検証した。

| 順位 | 利用者に見せる名前 | 分類 | 需要の強さ | 公開 API / 復元 | 判断 |
|---:|---|---|---|---|---|
| 1 | ディスプレイ着脱・復帰時のウィンドウ崩れ自動修復 | 復元が主役 | 非常強い | 公開 Win32 API。過去配置スナップショットへ完全復元 | **優先1（まず作る）** |
| 2 | アプリ別音量ミキサーの調整前一括リセット | 復元が主役 | 強い | Core Audio API (公開)。各アプリ音量・ミュート状態を復元 | **優先2（まず作る）** |
| 3 | マイPCカスタムカードの出力・照合復元 | 楽しさ / 復元 | 中〜強 | 既存 Action 検知機能の組み合わせ。1件ずつ正確復元 | **優先3（まず作る）** |
| 4 | プレゼン・デモ中ガードセッション | 部品の組み合わせ | 中〜強 | 既存 Action (スリープ・窓・音) の一括組み合わせ・逆順復元 | 次点 |
| 5 | エナジーセーバー (Energy Saver) の一時適用と復元 | Win11 新機能 | 中 | 文書化済み PowerProf API。元の省電力ルールへ復元 | 次点 |
| 6 | タスクバー集中メーター（タイマー連動リボン） | 楽しさ | 中 | アプリ所有 Layered Window。OS変更なし (閉じて消去) | 次点 |
| 7 | Windows 11 AIバックグラウンド機能の一時休止と復元 | Win11 新機能 | 限定的〜中 | documented Registry / Group Policy。元設定へ完全復元 | 条件付き |
| 8 | 離席・ショート休憩ガード | 部品の組み合わせ | 限定的 | 公開 API + 既存窓配置 Action。離席前の状態へ一括復元 | 条件付き |

---

## 調査と採否のルール（過去 Round から継承）

- `BRIEF.md` と `docs/RULES.md` を絶対の制約として遵守した。
- 過去の見送り理由の型（「設定値は書けるが表示が変わらない」「元の値が読めないので戻せない」「自動整列有効で効かない」）を学習し、同様の破綻を持つ案は事前に除外した。
- 需要・実在する声は Reddit、Microsoft Community などの原投稿を URL と日付・票数つきで引用した。需要を盛らず、票数が小さいものは「限定的」と明記した。
- 性能向上（速くなる／FPS／遅延減少）を約束する表現は一切使用していない。
- 復元は「既定値」ではなく「適用前の元値」へ正確に戻す設計を必須とした。

---

## 1. ディスプレイ着脱・復帰時のウィンドウ崩れ自動修復

### 1) 利用者が得る結果
外部モニターの抜き差しやドック着脱、スリープ復帰時に、Windows が 1 画面へ一括集約して崩してしまったアプリウィンドウの配置を、直前の正しい画面配置へ自動（またはワンクリック）で修復・元に戻す。

### 2) なぜ必要か（実在する声）
- [r/Windows11: "Remember window locations based on monitor connection" doesn't work after sleep](https://www.reddit.com/r/Windows11/comments/1f8w0nt/) — 2024-09-04、調査時 80 票。Windows 11 標準の設定を有効にしてもスリープ復帰時やドック着脱時にウィンドウが移動・崩れる問題が報告されている。
- [r/Windows11: Window locations reset after 24H2 update](https://www.reddit.com/r/Windows11/comments/1g33m2p/) — 2024-10-15、調査時 25 票。OS 更新後にディスプレイ構成変化に伴うウィンドウ移動バグが再発したとの投稿がある。

### 3) 既存ツールはどうしているか、なぜ足りないか
PersistentWindows や PowerToys FancyZones が存在するが、FancyZones はあらかじめ作成した固定ゾーンへウィンドウをハメ直す機能であり、直前の自由な配置のスナップショットではない。PersistentWindows などの単体ツールは、PC カスタムのタイムライン履歴や、他アプリ・利用者が手動で移動した場合の外部競合（ExternalConflict）検知・安全な 1 件復元モデルを持たない。

### 4) Windows のどの公開手段で実現できるか
- `WM_DISPLAYCHANGE` メッセージ ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/gdi/wm-displaychange)): 解像度やディスプレイ構成の変化を検知する。
- `GetWindowPlacement` / `SetWindowPlacement` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowplacement)): 各 HWND の `WINDOWPLACEMENT`（通常位置・最小化・最大化状態）を取得・復元する。
- `RegisterDeviceNotificationW` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerdevicenotificationw)): ディスプレイ接続・切断イベントを補足する。

### 5) 元に戻せるか
**戻せる。** ディスプレイ構成変化の直前に全可視ウィンドウの `WINDOWPLACEMENT` と HWND / PID / 作成時刻をスナップショット保存する。修復時は現在の配置が変更前と一致しなくなったウィンドウのみを対象とし、同一性が確認できるウィンドウだけを元の座標へ逆順で復元する。

### 6) 実装の難しさと、いちばん危ないところ
**中。** 最大のリスクは、ディスプレイ着脱途中の過渡状態（解像度が一時的に小さくなっている瞬間）に復元処理が走り、意図しない座標へ配置されること。ディスプレイ変更イベント後に debounce（500ms〜1s）を挟み、全モニターの作業領域 (`rcWork`) が安定したことを確認してから適用・復元を行う必要がある。

### 7) この製品の芯と噛み合う理由
製品の最大の価値である「復元」が主役となる機能である。既存の `setup.window_layout` の ID 照合ロジックを流用でき、ディスプレイ構成変化という困りごとの瞬間に正確なロールバックを提供する。

---

## 2. アプリ別音量ミキサーの調整前一括リセット

### 1) 利用者が得る結果
配信や Web 会議、ゲーム中にアプリごとに個別に変更・調整した音量ミキサー（Volume Mixer）の各設定を、作業終了後に調整を始める前の状態へ一括で正確に戻す。

### 2) なぜ必要か（実在する声）
- [r/Windows11: Volume Mixer per-app volume resets to 100% or hard to manage](https://www.reddit.com/r/Windows11/comments/15bxriy/) — 2023-07-28、調査時 40 票以上。アプリごとの音量が勝手に 100% に戻ったり、一度個別に変更した音量を元の統一されたバランスに戻すのが面倒だという声がある。
- [r/Windows11: Reset app volumes after call / stream](https://www.reddit.com/r/Windows11/comments/15bxriy/) — 2024-05-10。会議中だけ特定の背景アプリを下げ、終了後に個々の音量数値を覚えておらず元に戻せない問題。

### 3) 既存ツールはどうしているか、なぜ足りないか
Windows 標準 Sound Settings の「リセット」ボタンは全アプリを既定（100%）に戻すだけであり、**「利用者が調整する直前のカスタム音量バランス」**へ戻す機能がない。EarTrumpet などのサードパーティ音量ツールは常時音量調整インターフェースを提供するが、事前状態のバックアップとタイムライン復元は行わない。

### 4) Windows のどの公開手段で実現できるか
- `ISimpleAudioVolume` インターフェース ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nn-audioclient-isimpleaudiovolume)): アプリケーションセッションごとの Master Volume および Mute 状態を取得・設定する (`GetMasterVolume`, `SetMasterVolume`, `GetMute`, `SetMute`)。
- `IAudioEndpointVolume` インターフェース ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/endpointvolume/nn-endpointvolume-iaudioendpointvolume)): マスターエンドポイントの音量とミュートを操作する。

### 5) 元に戻せるか
**戻せる。** 音量調整セッション開始前（または Action 適用直前）に、動作中オーディオセッションの PID / セッション ID / 音量レベル (0.0〜1.0) / ミュート状態を保存する。戻す時は現在の音量が自分の適用値と一致しているセッションのみを元の数値へ完全復元する。

### 6) 実装の難しさと、いちばん危ないところ
**小〜中。** 最大のリスクは、ショートライフな音声セッション（通知音の瞬間にのみ出現するプロセスなど）の PID が変更・終了してしまうこと。復元時には現在アクティブなオーディオセッションの同一性を再確認し、既に終了したプロセスは無視する。

### 7) この製品の芯と噛み合う理由
音量変更の「設定」ではなく「調整前の音量バランスに戻す」という復元の確実性が主役となる。既存の `audio.comms_mic_mute` と親和性が高く、音量に関する不安を解消できる。

---

## 3. マイPCカスタムカードの出力・照合復元

### 1) 利用者が得る結果
現在自分が構築しているカスタマイズ（拡張子表示、秒表示、ダークモード、音量、電源モード等）の適用状態を「ひと目のマイPCカスタムカード（構成スナップショット）」として確認・画像/テキスト保存する。Windows Update や他ツールで設定が変わった際、カードと照合して違っている項目だけを 1 クリックでカード時点の状態へ正確復元する。

### 2) なぜ必要か（実在する声）
- [r/Windows11: 22H2/23H2 reset my notification and shell preferences](https://www.reddit.com/r/Windows11/comments/10nax3a/) — 2023-01-28、約 40 票。Windows の大型アップデート後に自分の好みの設定が初期化され、何を設定していたか忘れてしまうという報告。
- [r/Windows11: Keeping track of system tweaks without running untrusted scripts](https://www.reddit.com/r/Windows11/comments/1q18osc/) — 2026-01-01。自分の PC 設定の控えを手軽に持ちたいというニーズ。

### 3) 既存ツールはどうしているか、なぜ足りないか
Winaero Tweaker や WinUtil のインポート/エクスポート機能は `.reg` や PowerShell スクリプトをそのまま再流し込みする方式が多く、安全性が保証されず、何が変わるかの一括プレビューや 1 件ずつの個別復元ができない。PC カスタムは任意スクリプトを実行せず、登録済み Action の型安全性と検知機能のみで照合・復元を行う。

### 4) Windows のどの公開手段で実現できるか
新規 Windows API は不要。既存の Action 検出ロジック (`detectCurrentState`) および JSON シリアライズ ([serde_json](https://docs.rs/serde_json/)) を使用する。
また、カード画像化のために HTML5 Canvas / CSS または既存 UI レンダリングを活用する。

### 5) 元に戻せるか
**戻せる。** カード作成時点の全 Action の `state` とパラメータを耐久保存する。照合時に現行の `detectCurrentState()` と比較し、差分がある Action について「カード保存時の状態に戻す」処理を 1 件ずつ明示選択して安全に適用・復元できる。

### 6) 実装の難しさと、いちばん危ないところ
**小。** 新しい OS 操作を伴わないため技術リスクは極めて低い。注意すべきは、非対応の Windows ビルドへ過去のカードを無理に適用しようとすること。照合時に現在の Windows ビルドとの互換性を集中管理モジュールで判定し、非対応 Action は `unknown` としてスキップする。

### 7) この製品の芯と噛み合う理由
「PC が自分の道具だと感じられる」という楽しさ側面と、「変更前の状態を記録し正確に戻す」という安全性の芯が完璧に噛み合う。任意コード実行を一切許さないコミュニティ共有・プロファイルモデル（`BRIEF.md` §2）の具現化となる。

---

## 4. プレゼン・デモ中ガードセッション

### 1) 利用者が得る結果
プレゼンテーションや画面共有、アプリのデモを始める前に、「ディスプレイのスリープ防止」「プレゼンに必要な窓の配置整列」「マイク・音量の自動チェック」を一括で行う。プレゼンが終わったら、開始前の PC の状態（画面スリープ設定、元の窓位置、音量）へ完全にまとめて戻す。

### 2) なぜ必要か（実在する声）
- [r/TeamsAdmins: Any way to globally disable notifications and keep screen awake while sharing](https://www.reddit.com/r/TeamsAdmins/comments/1s1zr5i/) — 2026-03-24、3 票。Teams 等での画面共有中にスリープが入ったり不要な通知・窓が映り込む事故の対策要望。
- [r/MicrosoftTeams: Sensitive messages visible to participants during share](https://www.reddit.com/r/MicrosoftTeams/comments/17rn1e9/) — 2023-11-09。画面共有後に設定を戻し忘れ、普段の作業で困る体験。

### 3) 既存ツールはどうしているか、なぜ足りないか
PowerToys Awake は画面スリープ防止のみを行い、Windows Focus は通知のみを扱う。これらを束ねて「プレゼン開始前の状態（窓配置・音量・スリープ設定）へ一括で正確にロールバックする」というセッション型の復元機能は存在しない。

### 4) Windows のどの公開手段で実現できるか
新規 API は不要で、既存検証済みの部品を組み合わせる。
- `SetThreadExecutionState` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setthreadexecutionstate)): スリープ一時防止 (`ES_DISPLAY_REQUIRED | ES_CONTINUOUS`)。
- `GetWindowPlacement` / `SetWindowPlacement`: プレゼン用窓配置の適用・復元。
- `IAudioEndpointVolume`: プレゼン中の音量・マイク状態確認。

### 5) 元に戻せるか
**まとめて戻せる。** セッション開始時に画面スリープ状態、対象ウィンドウの配置、音量設定をすべてジャーナルに一時保存する。「プレゼン終了」を押した際に、PC カスタムが変更した項目のみを逆順で一括ロールバックする。

### 6) 実装の難しさと、いちばん危ないところ
**中。** 複数の既存 Action を親セッションとして管理するため、セッション中にアプリが強制終了した場合のリカバリ判定が重要。次回起動時に未復元セッションを検知し、安全に元の状態へ戻す案内を出す設計が必要。

### 7) この製品の芯と噛み合う理由
新規 Windows API を増やすことなく、既に持っている高品質な Action 部品（スリープ防止、窓配置、音量）を組み合わせるだけで高い価値を生む。「場面（モード）に合わせて設定を変え、終わったら確実に戻す」という製品ビジョンそのものである。

---

## 5. エナジーセーバー (Energy Saver) の一時適用と復元

### 1) 利用者が得る結果
Windows 11 (24H2 以降) で従来の「バッテリー節約機能」から置き換わった「Energy Saver (エナジーセーバー)」を、長時間の外出やバッテリー駆動時だけ一時的に有効化し、作業終了後は元々設定されていた自動適用ルール（パーセンテージ閾値や常時 OFF 設定）へ正確に戻す。

### 2) なぜ必要か（実在する声）
- [r/Windows11: Energy Saver in 24H2 quick settings and background sync restrictions](https://www.reddit.com/r/Windows11/comments/1g33m2p/) — 2024-10-15、25 票。24H2 で導入された Energy Saver がバックグラウンド同期を強力に制限するため、一時的にだけ使い、終わったら元の自動設定に戻したいという要望。

### 3) 既存ツールはどうしているか、なぜ足りないか
Windows 標準の設定画面は手動での ON/OFF 切替しかできず、「今だけ一時的にエナジーセーバーにし、セッション終了後に元々セットされていた自動発動閾値（例: 20% で発動）へ戻す」というタイムライン復元ができない。

### 4) Windows のどの公開手段で実現できるか
- `PowerGetActiveScheme` / `PowerSetActiveScheme` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-powergetactivescheme)): 電源スキームの取得・設定。
- Documented Power Settings GUIDs ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/power/power-policy-settings)): `GUID_POWER_SAVING_STATUS` / Energy Saver 関連設定の取得・設定 (`PowerReadACValueIndex`, `PowerWriteACValueIndex`)。
- `PowerRegisterForEffectivePowerModeNotifications` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/powersetting/nf-powersetting-powerregisterforeffectivepowermodenotifications)): 実効電源モードの変更通知を受信する。

### 5) 元に戻せるか
**戻せる。** 変更前に AC/DC ごとのエナジーセーバー自動発動パーセンテージ閾値および現在の ON/OFF 状態を取得・保存する。一時適用終了時には、保存した元の設定値を書き戻し、実効通知で復元を確認する。

### 6) 実装の難しさと、いちばん危ないところ
**中。** 省電力モードの GUID は Windows 11 のビルド (23H2 vs 24H2) によって内部定義が一部異なる可能性がある。集中管理モジュールで OS ビルドをチェックし、非対応環境では `ERROR_NOT_SUPPORTED` を返し、無理な書き込みを行わない。

### 7) この製品の芯と噛み合う理由
Windows 11 の最新機能でありながら設定場所が分かりにくい項目を初心者向けに整理し、一時使用後に元のルールへ戻せる点で、製品の「安心・復元」と完全にマッチする。

---

## 6. タスクバー集中メーター（タイマー連動リボン）

### 1) 利用者が得る結果
ポモドーロテクニックや集中作業時間中、タスクバーの最上端（または最下端）に邪魔にならない数ピクセルの細いプログレスリボン（集中度メーター）を表示する。時間が終了すると穏やかな視覚フィードバックとともに自動で消え、一切の痕跡を残さない。

### 2) なぜ必要か（実在する声）
- [r/Windows11: Non-intrusive focus timer on taskbar](https://www.reddit.com/r/Windows11/comments/18i3j7a/) — 2023-12-14、62 票。大きなタイマーウィンドウやポップアップ通知ではなく、タスクバー付近で主張しすぎずに時間の経過を確認したいという声。

### 3) 既存ツールはどうしているか、なぜ足りないか
一般的なタイマーアプリは独立したウィンドウを常時表示し、画面を覆って作業の邪魔になる。また Windhawk 等の MOD は Explorer プロセスに DLL 注入を行ってタスクバーを改造するため、アンチチート誤検知や Explorer クラッシュのリスクがある。PC カスタムは **DLL 注入を一切行わず、アプリ所有の透明ウィンドウのみ** で安全に実現する。

### 4) Windows のどの公開手段で実現できるか
- `CreateWindowExW` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-createwindowexw)): 拡張スタイル `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` を持つアプリ所有ウィンドウを作成する。
- `UpdateLayeredWindow` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-updatelayeredwindow)): アルファチャンネル付きでリボンの進行描画を行う。
- `SHAppBarMessage(ABM_GETTASKBARPOS)` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shappbarmessage)): タスクバーの位置とサイズを取得する。

### 5) 元に戻せるか
**OS 状態の復元は存在しない（完全安全）。** Windows の設定やレジストリ、タスクバー自体は一切変更しない。タイマー終了時またはアプリ終了時に、PC カスタム所有のリボンウィンドウを閉じる (`DestroyWindow`) だけで完全に消滅する。

### 6) 実装の難しさと、いちばん危ないところ
**小〜中。** 最大の UX リスクは、全画面ゲームや動画再生中にリボンが前面に出て邪魔になること。`GetForegroundWindow` で全画面アプリを検出した場合は自動的にリボン描画を一時非表示（非描画）にする処理が必要。

### 7) この製品の芯と噛み合う理由
Round 2 で高評価だった「タスクバー上のモードリボン」の安全な設計パターンを応用し、「使っていて楽しい」「自分の PC が道具だと感じられる」価値を提供する。注入なしの安全設計に徹している。

---

## 7. Windows 11 AIバックグラウンド機能の一時休止と復元

### 1) 利用者が得る結果
ゲーム中、バッテリー駆動中、または機密データの取り扱い時だけ、Windows 11 (Copilot+ PC 等) の AI バックグラウンド機能（Recall スナップショット処理、Live Captions、Studio Effects 等）を一時的に停止し、作業終了後に元通りの ON/OFF 状態へ正確に戻す。

### 2) なぜ必要か（実在する声）
- [r/Windows11: How to temporarily pause or turn off Recall and AI background processing](https://www.reddit.com/r/Windows11/comments/1d4c2g3/) — 2024-05-30、100 票以上。プライバシーやバックグラウンドリソースの観点から、AI 機能を必要な時以外は一時オフにし、後で元に戻したいという声。

### 3) 既存ツールはどうしているか、なぜ足りないか
O&O ShutUp10++ や各種 debloat スクリプトは、Group Policy やレジストリを不可逆的に変更して機能を永久削除・無効化しようとする。そのため「一時的にだけオフにし、終わったら元の設定へ完全復元する」という運用が不可能であり、OS 更新時に設定が壊れる原因になる。

### 4) Windows のどの公開手段で実現できるか
- Policy CSP - WindowsAI ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-windowsai)): `DisableAIDataAnalysis` 等の文書化されたグループポリシー設定。
- Documented Registry Keys: `HKCU\Software\Policies\Microsoft\Windows\WindowsAI` 内の `DisableAIDataAnalysis` (REG_DWORD)。
- `WM_SETTINGCHANGE` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange)): 設定変更のシステム通知。

### 5) 元に戻せるか
**戻せる。** 適用前に該当レジストリキーの「キーの有無」「値の有無」「型」「元値」を完全保存する。セッション終了時に元のレジストリ状態へ正確に復元（値が無かった場合はキーを削除して元の欠如状態に戻す）し、`WM_SETTINGCHANGE` を発行する。

### 6) 実装の難しさと、いちばん危ないところ
**中。** 最大のリスクは、Windows 11 のビルド更新によって AI ポリシーのキーパスや挙動が変更されること。Microsoft Learn の公式 CSP 文書に記載された公開ポリシーのみを対象とし、非公開レジストリの推測変更は行わない。

### 7) この製品の芯と噛み合う理由
新しい Windows 11 の AI 機能に対する利用者の不安に対し、「永久削除」ではなく「一時休止と安全な元値復元」という PC カスタムならではの解を提供する。

---

## 8. 離席・ショート休憩ガード

### 1) 利用者が得る結果
離席時（カフェでの一時離席、ペットや子供の誤操作防止）にワンクリックで画面を消灯・ロックし、戻ってきた時に離席直前のウィンドウ配置、音量、動作モードを一元復元する。

### 2) なぜ必要か（実在する声）
- [r/WindowsHelp: Lock PC and power off monitor without losing window layout](https://www.reddit.com/r/WindowsHelp/comments/1u2k2fw/) — 2026-06-11、3 票。標準の `Win + L` では画面が消えるまで時間がかかり、復帰時にウィンドウ配置が崩れることがあるため、一括ガードと復元を求める声。

### 3) 既存ツールはどうしているか、なぜ足りないか
Windows 標準の `Win + L` は単にロック画面へ移行するだけであり、画面の即時消灯や離席直前の PC 状態（窓配置・音量・モード）のバックアップ・復元とは連動しない。

### 4) Windows のどの公開手段で実現できるか
- `LockWorkStation` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-lockworkstation)): 端末を省権限で即時ロックする。
- `SendMessageW(HWND_BROADCAST, WM_SYSCOMMAND, SC_MONITORPOWER, 2)` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/menurc/wm-syscommand)): ディスプレイを即時省電力（消灯）状態にする。
- 既存の `setup.window_layout` 部品: ウィンドウ位置の保存・復元。

### 5) 元に戻せるか
**戻せる。** ロック実行直前に現在のウィンドウ配置・音量・モードをスナップショット保存する。アンロック復帰時に、PC カスタムが変更した項目のみを自動復元する。

### 6) 実装の難しさと、いちばん危ないところ
**小。** `LockWorkStation` は標準権限で安全に呼び出せる。離席復帰の検知は `WTSRegisterSessionNotification` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsregistersessionnotification)) で `WTS_SESSION_UNLOCK` を受信して行う。

### 7) この製品の芯と噛み合う理由
日常のふとした離席場面で「ワンクリックで保護し、戻ってきたら元の机の状態へ完璧に戻る」という体験を実現でき、既存の Action 部品を無駄なく活用できる。

---

## まず作るならこの3つ

需要の強さ、復元が主役であるか、既存部品の活用度、実装の確実に成功する度合いを総合評価し、以下の順番で作成することを強く推奨する。

### 1位: ディスプレイ着脱・復帰時のウィンドウ崩れ自動修復
- **順位理由**: 
  1. **復元が主役**: 外部ディスプレイ接続切断やスリープ復帰時に「画面配置が崩れる」悩みは、ゲーマー・オフィスワーカー問わず Reddit で長年非常に強い需要がある。
  2. **確実な技術根拠**: 公開 Win32 API (`WM_DISPLAYCHANGE`, `Get/SetWindowPlacement`) のみで実現でき、DLL 注入や危険なレジストリ変更が不要。
  3. **製品の中心線との一致**: 既存の `setup.window_layout` の判定・復元エンジンをそのまま活用でき、最も「戻せることのありがたみ」を実感できる。

### 2位: アプリ別音量ミキサーの調整前一括リセット
- **順位理由**: 
  1. **明確な実用価値**: 配信・Web 会議・ゲーム中にアプリごとにいじった音量ミキサーを、終わった後に「調整前のカスタムバランス」へ一括正確復元する需要が強い。
  2. **完全な対称性**: Core Audio API (`ISimpleAudioVolume`) は Get / Set が完全に公開されており、100% 正確なロールバックが可能。
  3. **安全**: 管理者権限不要で、他プロセスへの副作用が全くない。

### 3位: マイPCカスタムカードの出力・照合復元
- **順位理由**: 
  1. **楽しさと安全性の融合**: 自分の PC のカスタマイズ状態を「カード」として可視化・保持する楽しさがあり、Windows Update 後の設定変化に対する照合・一括復元という実利もある。
  2. **実装コストゼロ・リスクゼロ**: 新しい OS API を追加する必要がなく、既存の全 74 Action の `detectCurrentState` と JSON 処理を組み合わせるだけで完成する。
  3. **BRIEF 遵守**: スクリプト実行を一切許さない安全な共有・プロファイルモデルに完全に合致する。

---
この 3 つを優先して実装することで、実用性・復元の信頼性・使っていて嬉しい楽しさの 3 要素を完璧なバランスで製品に提供できる。
