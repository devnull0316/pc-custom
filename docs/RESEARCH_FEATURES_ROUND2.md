# 機能調査 Round 2

調査日: 2026-07-29  
対象: Windows 11 / PC Custom  
前提: `BRIEF.md`、`docs/STATUS.md`、`docs/RESEARCH_FEATURES.md`、`tasks/TASK_FEATURE_RESEARCH_ROUND2.md` の順に確認した。

## 結論

Round 2 では、Round 1 と重ならない7候補を残した。4件は「触って楽しい／自分のPCになった感じ」、3件は実用側である。実用側では、現在「表示のみ」の `explorer.separate_process` と `explorer.info_tips` に、Microsoft が取得・設定を明記した公開 API があることを確認した。ただし、文書上の setter の存在と、現行 Windows 11 で外から効果を確認できることは別なので、いきなり mutable に昇格させない。

票数は需要の規模そのものではない。下記は各ページで調査時に見えたスコアまたは reaction 数であり、Reddit のスコアは変動し、GitHub の reactions が非表示のページもある。直接その機能を求める声と、隣接する要望は区別した。性能・FPS 向上はどの候補でも約束しない。

| 候補 | 種類 | 需要の強さ | 公開 API / 復元 | 判断 |
|---|---|---:|---|---|
| 既定の通話マイクをシステム側でミュート | 実用 | 中〜強・直接 | Get/Set あり。端末ID単位で復元可能 | 優先1 |
| タスクバー上のモードリボン | 楽しい | 中・隣接 | PC Custom 所有ウィンドウだけ。閉じれば消える | 優先2 |
| Explorer を別プロセスで開く設定の昇格 | 実用 | 小〜中・直接 | Get/Set あり。現行 Windows 11 の外部検証が必要 | 優先3、proof-gated |
| 安全なホットコーナー | 楽しい | 中・直接 | 読み取り API とアプリ内設定。変更は確認画面経由 | 次点 |
| 配色シーン | 楽しい | 中・隣接 | 既存の検証済み Action だけを合成 | 次点 |
| Explorer の情報ツールチップ設定の昇格 | 実用 | 小・直接 | Get/Set あり。外部 UI 検証が必要 | 低優先、proof-gated |
| 対応 RGB をモード色にする | 楽しい | 強・隣接 | 色 setter はあるが現在色 getter がない | 保留 |

---

## 候補1: 既定の通話マイクをシステム側でミュート

### 1. ユーザー語の成果

「会議前に、アプリごとのミュート状態を探し回らず、Windows の既定の通話マイクを確実にミュートしたい。」

初版の表示名は「すべてのマイクをミュート」ではなく、実際の作用範囲どおり「既定の通話マイクをミュート」にする。Discord 等が別の入力端末を明示選択している場合や、排他モードでは効かない場合があるため、「全アプリ保証」とは言わない。

### 2. 実在する声と必要性

- [PowerToys: Video Conference Mute feature request](https://github.com/microsoft/PowerToys/issues/37218) — 2025-02-01、調査時 `+9` reactions。PowerToys 0.88 で Video Conference Mute が削除された後の復活要望で、コメントにはカメラ機能を要らないのでマイクだけ欲しいという声がある。専用の仮想カメラまで含めず、マイクだけに絞る根拠になる。
- [silence! — a simple FOSS Windows app to mute your mic globally](https://www.reddit.com/r/Windows11/comments/1q23eu3/silence_a_simple_foss_windows_app_to_mute_your/) — 2026-01-02、調査時 `+122`。専用アプリが反応を得ており、「Windows 全体で使えるミュート」への直接需要は確認できる。
- [silence! v2](https://www.reddit.com/r/Windows11/comments/1t4e5n6/silence_v2_mute_your_mic_with_even_more_features/) — 2026-05-05、調査時 `+56`。継続開発にも反応がある。ただし、これは投稿への評価であり、市場規模の推計には使わない。

### 3. 既存ツールと足りない点

Windows の `Win + Alt + K` は対応アプリ側の統合に依存する。会議アプリ自身のミュートは、そのアプリ内では最も状態が分かりやすい。一方、PowerToys の旧 Video Conference Mute は仮想カメラ等を伴い、保守コストに対して利用が少ないとして削除された。`silence!` のような専用アプリはホットキーや常時表示では PC Custom より機能が深い。

PC Custom が勝てる範囲は、専用ミュートアプリの代替ではなく、「会議モード」の一 Action として、変更前の端末IDと mute 状態をジャーナルに残し、ほかの登録済み Action と一緒に戻すことに限る。カメラ、仮想ドライバー、アプリプロセス操作には広げない。

### 4. 文書化された公開 API

- [IMMDeviceEnumerator::GetDefaultAudioEndpoint](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-getdefaultaudioendpoint) — `eCapture` と `eCommunications` を指定して、既定の通話用入力 endpoint を取得できる。
- [IAudioEndpointVolume::GetMute](https://learn.microsoft.com/en-us/windows/win32/api/endpointvolume/nf-endpointvolume-iaudioendpointvolume-getmute) — endpoint の現在の mute 状態を取得する。
- [IAudioEndpointVolume::SetMute](https://learn.microsoft.com/en-us/windows/win32/api/endpointvolume/nf-endpointvolume-iaudioendpointvolume-setmute) — endpoint の mute 状態を設定する。
- [IAudioEndpointVolume interface](https://learn.microsoft.com/en-us/windows/win32/api/endpointvolume/nn-endpointvolume-iaudioendpointvolume) — hardware mute の有無と、shared / exclusive mode での差を説明している。
- [EndpointVolume API](https://learn.microsoft.com/en-us/windows/win32/coreaudio/endpointvolume-api) — endpoint volume の不適切な利用はユーザーの system volume 設定を乱し得ると公式に警告している。

### 5. 復元の正直さ

適用直前に endpoint の device ID と `GetMute` の値を保存し、その同じ endpoint にだけ元値を戻す。復元時に既定端末が変わっていても、新しい既定端末を勝手に unmute しない。対象が一時的に外れている場合は「復元待ち」をジャーナルに残し、同じ device ID の再接続時に復元できる設計が必要である。

PC Custom 以外から mute 状態が変更された場合は競合として検出し、自動で上書きしない。外部変更がなければ完全復元、外部変更があれば「後から行われたユーザー操作を優先し、未復元として説明」が正直な挙動である。software mute を迂回する排他モードの端末では、適用結果を「設定値は mute だが排他ストリームへの実効性は保証外」と表示する。

### 6. 難易度とリスク

難易度は中、危険度は「注意」が妥当。管理者権限は不要。難所は COM endpoint の寿命、既定端末変更、抜き差し、外部変更、hardware mute の有無である。初版は `eCommunications` の既定 capture endpoint 1台だけに限定し、全 capture endpoint の一括操作は行わない。

検証は、適用前の device ID / mute、適用後の `GetMute=true`、復元後の元値一致の三点に加え、既定端末変更後に別端末を触っていないことを確認する。

### 7. PC Custom の中心価値との一致

公開 API、事前状態の取得、同一対象への exact rollback、Action とモードの組み合わせという中心価値に合う。性能改善ではなく、会議前の不安を減らす明確な成果である。Action ID として独立させれば、既存の手動プロファイルにも安全に組み込める。

---

## 候補2: タスクバー上のモードリボン

### 1. ユーザー語の成果

「今が普段・会議・ゲームのどのモードか、タスクバーを見るだけで分かり、PC の見た目にも自分らしい色を足したい。」

タスクバー自体を書き換える skin ではない。初版はプライマリタスクバーの直上に、数ピクセルの色または穏やかなパターンを出す、クリック透過の PC Custom 所有ウィンドウに限定する。

### 2. 実在する声と必要性

- [Windows 11 Taskbar Styler v1.2](https://www.reddit.com/r/Windows11/comments/18i3j7a) — 2023-12-14、調査時 `+62`。グラデーションや画像を使った taskbar style の共有に反応がある。
- [“how can i customize the task bar”](https://www.reddit.com/r/Windows11/comments/1mrzzn9/how_can_i_customize_the_task_bar_its_ugly_af/) — 2025-08-16、調査時 `+61`。多数の返信で Windhawk、StartAllBack、TranslucentTB が挙がる一方、anti-cheat、更新後の Explorer crash、無料ツールへの信頼を気にする声もある。

これは「モードを色で示す細いリボン」への直接投票ではなく、taskbar を安全に自分好みにしたいという隣接需要である。したがって需要を「強」とは評価しない。

### 3. 既存ツールと足りない点

[TranslucentTB](https://github.com/TranslucentTB/TranslucentTB) は taskbar の外観制御に特化しており、[Windhawk の Windows 11 taskbar styling guide](https://github.com/ramensoftware/windows-11-taskbar-styling-guide) はより深い styling を可能にする。深い customization ではそれらが上である。

ただし Windhawk は mod を対象プロセスへ注入する仕組みを持ち、[Injection targets and critical system processes](https://github.com/ramensoftware/windhawk/wiki/Injection-targets-and-critical-system-processes) でも対象指定を説明している。PC Custom の許容範囲では Explorer への injection や taskbar 内部要素の patch は不可である。モードリボンは taskbar の代替・改造ではなく、「PC Custom の状態表示を taskbar 近傍に置く」小さな価値に絞る。

### 4. 文書化された公開 API

- [ABM_GETTASKBARPOS](https://learn.microsoft.com/en-us/windows/win32/shell/abm-gettaskbarpos) — system taskbar の bounding rectangle と edge を取得する。
- [SHAppBarMessage](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shappbarmessage) — appbar message の公開入口。
- [CreateWindowExW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-createwindowexw) と [Extended Window Styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles) — tool window、no-activate、transparent 等を持つアプリ所有ウィンドウを作る。
- [UpdateLayeredWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-updatelayeredwindow) — layered window の位置、形状、透明度を更新する。
- [SetWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos) — アプリ所有ウィンドウの位置と Z order を制御する。

`docs/STATUS.md` に、アプリ所有 overlay が Explorer restart 後も再取得できること、fullscreen 抑制・taskbar geometry・DPI・monitor 処理が必要なことまで、既存実験の結果がある。新しい Windows 設定面を発見する候補ではなく、既に得た安全な土台の製品化候補である。

### 5. 復元の正直さ

Windows の taskbar 状態は変更しない。リボンを出した PC Custom 所有ウィンドウを閉じれば完全に消えるため、OS 状態の rollback は存在しない。プロファイル rollback 時は、リボンの表示状態と選択色を元のアプリ内状態へ戻す。

異常終了後に残る別プロセスや Shell patch を作らないことが重要である。PC Custom のプロセスが終われば overlay も消える構造にする。work area を予約してほかのウィンドウを押し上げる appbar 登録は初版ではしない。

### 6. 難易度とリスク

難易度は中、危険度は「安全」。OS 変更リスクは低いが、UX リスクはある。fullscreen ゲーム、動画、通知領域、auto-hide taskbar、DPI、Explorer restart、タスクバーが上・左右にある環境で邪魔になり得る。

初版はプライマリタスクバーだけ、既定で fullscreen 中は非表示、クリック透過、太さ上限あり、アニメーションなしまたは低頻度にする。FPS や latency への影響なしとは断言せず、常駐描画を最小にして測定する。

### 7. PC Custom の中心価値との一致

「今どのモードか分かる」と「自分のPC感」を同時に出せる。Action の効果を説明可能にし、モードの組み合わせを目で確認できる。Explorer 内部改造を避け、アプリ所有物だけで実現するため、信頼の中心線にも合う。

---

## 候補3: Explorer を別プロセスで開く設定を mutable に再判定

### 1. ユーザー語の成果

「フォルダーの窓を別の Explorer プロセスで開く設定を選び、1つの窓の問題がデスクトップ全体に波及しにくい構成を試したい。」

「クラッシュしなくなる」「メモリリークが直る」とは言わない。現行 Windows 11 のタブ付き Explorer が実際にどう分離するかを外から確認できた場合にだけ、この成果文を出す。

### 2. 実在する声と必要性

- [“Launch folder windows in a separate process”](https://www.reddit.com/r/TechNope/comments/sihx3z) — 2022-02-02、調査時 `+46`。コメントには、folder window の問題で shell 全体が再起動するのを避けたいという理解がある。
- [Windows 11 Explorer memory leak discussion](https://www.reddit.com/r/Windows11/comments/15bxriy) — 2023-07-28、調査時 `+8`。投稿者は separate process で症状が改善したと報告するが、単一環境の体験談であり、一般化できない。
- 反証として、[Insider build で separate process 有効時に Explorer が開かない報告](https://www.reddit.com/r/Windows11/comments/xci8g9) — 2022-09-12、調査時 `+18` がある。需要だけでなく、build 依存リスクも採用判断に含める。

### 3. 既存ツールと足りない点

Windows 自身の File Explorer Options に「Launch folder windows in a separate process」があるため、単独設定としての新規性は低い。一般的な tweaker でも同じ設定を扱える。

PC Custom に足りないのは setter そのものではなく、現在「表示のみ」になっている項目について、公開 API で apply → 外部確認 → 元値復元までを一つの証拠鎖にできるかである。Windows の checkbox は変更前 snapshot、モード単位の rollback、失敗時の説明をまとめて提供しない。

### 4. 文書化された公開 API

- [SHGetSetSettings](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetsetsettings) — Shell state settings を取得または設定する。`bSet=FALSE` が取得、`TRUE` が設定と明記されている。
- [SSF Constants](https://learn.microsoft.com/en-us/windows/win32/shell/ssf-constants) — `SSF_SEPPROCESS` が `SHELLSTATE.fSepProcess` を対象にすると明記されている。

これは private symbol、registry の推測、UI Automation による checkbox 操作ではない。ただし `SHGetSetSettings` は戻り値が `void` なので、setter 呼び出し完了だけでは成功証明にならない。

### 5. 復元の正直さ

適用前に同じ API で `fSepProcess` を取得し、ジャーナルに保存する。適用後は再取得だけでなく、新しく開いたテスト用 folder window がどの Explorer process に属するかを外から確認する。復元は元の `fSepProcess` を同じ API で設定し、新規テスト窓で再確認する。

既存のユーザー Explorer 窓は閉じない。設定の反映に新しい窓または Explorer 再起動が必要なら、その条件を UI に明記する。Windows build 上、設定値だけ変わり挙動が分離しない場合は「適用済み」とせず、表示のみへ戻す。

### 6. 難易度とリスク

難易度は中〜高、危険度は「注意」。API 呼び出し自体より、Windows 11 の tabbed Explorer、既存窓の再利用、shell process と folder process の識別、build 差の外部検証が難しい。Explorer restart を自動で強制しない初版が安全である。

まず対応 build を1つに固定した検証専用 spike とし、次を全て満たした場合だけ mutable に昇格する。

1. get → set → get が一致する。
2. PC Custom 所有の一意なテスト folder を新しく開き、process identity の差を観測できる。
3. rollback → get と外部観測が元に戻る。
4. 既存の Explorer 窓、desktop、taskbar を閉じたり壊したりしない。

### 7. PC Custom の中心価値との一致

39件ある表示のみ項目を「API がないから」と一括で諦めず、公開 setter と外部検証の両方で再評価する代表例になる。成功すれば exact rollback を伴う実用 Action が1件増える。失敗しても、表示のみの理由を API 不在ではなく「現行 build で効果を外部確認できない」と正確に説明できる。

---

## 候補4: 安全なホットコーナー

### 1. ユーザー語の成果

「画面の角にマウスを少し置くだけで、よく使うモードや Action の確認画面をすぐ開きたい。」

初版は corner 到達で Windows を黙って変更しない。読み取り Action は即表示できるが、mutable Action / profile は PC Custom の preview を開き、ユーザー確認後にだけ適用する。

### 2. 実在する声と必要性

- [PowerToys issue: Add hot corners functionality](https://github.com/microsoft/PowerToys/issues/1305) — 2020-02-17 に open、調査時も open。GitHub は reactions を「currently unavailable」と表示しており、票数は不明。macOS / Linux のような corner action を Windows に求める直接要望である。
- [r/Windows11 “Hot corners”](https://www.reddit.com/r/Windows11/comments/oe1u39) — 2021-07-05、調査時 `+11`。便利という声がある一方、無効化したいという反対意見もある。
- [Open-source Windows Hot Corners](https://www.reddit.com/r/Windows11/comments/1qelpcf/open_source_i_brought_the_linuxmacos_hot_corners/) — 2026-01-16、調査時 `+27`。Task View 等を corner で出す実装に反応がある。コメントには「Linux では最初に無効化する」という声もあり、opt-in と誤発火対策が必須である。

### 3. 既存ツールと足りない点

WinXCorners、HotCorners 系アプリは Task View、lock、任意アプリや script の実行まで提供する。PowerToys issue の提案にも PowerShell / cmd command の実行が含まれる。

PC Custom は任意 command、script、実行ファイル path を受け付けてはいけない。登録済み Action ID と profile ID だけを候補にし、危険度が「注意」「実験的」、管理者権限、再起動、不可逆の Action は corner へ割り当て不可にする。機能幅では既存専用ツールに負けるが、適用前 preview と rollback journal を強制できる点が差になる。

### 4. 文書化された公開 API

- [GetCursorPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getcursorpos) — cursor の screen coordinate を取得する。
- [MonitorFromPoint](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-monitorfrompoint) — point を含む monitor を得る。
- [GetMonitorInfo](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmonitorinfoa) — monitor rectangle と work area を得る。
- [GetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getforegroundwindow) と [GetWindowPlacement](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowplacement) — foreground window と maximized state の抑制判断に使える。

低頻度 polling で足り、global low-level mouse hook は不要である。fullscreen 判定は maximized だけでは不十分なので、foreground window rectangle と monitor rectangle の一致を含む実機検証が要る。

### 5. 復元の正直さ

corner の割当、dwell 時間、cooldown は PC Custom のアプリ内設定なので、無効化または元の割当に戻せる。corner 到達だけでは OS 状態を変えず、preview を閉じれば何も残らない。

preview 後に Action / profile を適用した場合の復元は、既存の Action journal が担う。corner 機能独自の hidden mutation は作らない。競合や部分失敗も通常の profile と同じ結果画面へ送る。

### 6. 難易度とリスク

難易度は中、危険度は初版「安全」。主なリスクは誤発火、fullscreen game 中の割込み、multi-monitor の内側 corner、auto-hide taskbar との競合、常駐 polling である。

既定 off、600ms 程度の dwell、発火後 cooldown、fullscreen / game 中は既定抑制、各 monitor の外周 corner だけ、pointer が corner を離れるまで再発火なし、を最低条件にする。即時 mutation は行わないので、便利さより信頼を優先した仕様になる。

### 7. PC Custom の中心価値との一致

既存の70 Action と profile の「組み合わせ価値」を、起動導線として強くできる。新しい危険な Windows tweak を増やさず、登録済み能力だけを再利用する。「自分のPCらしい操作感」が出る一方、preview を中心に据えることで PC Custom の説明・復元モデルを壊さない。

---

## 候補5: 配色シーン

### 1. ユーザー語の成果

「集中、夜、会議などの名前から選ぶだけで、明暗・透明効果・Windows のアクセント色を一まとまりで試し、気に入らなければ元へ戻したい。」

外部 `.theme` file の import や、未文書 registry key の大量変更ではない。既に mutable として実装・検証されている `theme.color_mode`、`appearance.transparency`、`appearance.window_color` の組み合わせを、視覚的な scene card として見せる候補である。

### 2. 実在する声と必要性

- [The state of automatic dark mode on Windows 11](https://www.reddit.com/r/Windows11/comments/12zyj1p) — 2023-04-26、調査時 `+52`。Auto Dark Mode 作者が theme switching の実装と Windows 側の制約を説明しており、時間や状況に応じた見た目変更への需要がある。
- [Windows 11 still lacks a true Black Dark Mode](https://www.reddit.com/r/Windows11/comments/1q18osc/windows_11_still_lacks_a_true_black_dark_mode/) — 2026-01-01、調査時 `+138`。より深い黒を求める反応は強いが、PC Custom が OS や第三者アプリを「true black」にできる根拠にはならない。

これらは「PC Custom の3 Action を scene card にする」直接需要ではなく、見た目を状況別に整えたい隣接需要である。`true black`、全アプリ統一、時間帯の完全自動化は約束しない。

### 3. 既存ツールと足りない点

Windows Themes と Auto Dark Mode は、theme / 時刻切替では PC Custom より成熟している。PC Custom 自身にも manual profile の組み合わせ機能があり、単に3 Action を束ねるだけなら既に可能である。

不足は新しい setter ではなく、ユーザー語の scene、適用前の色見本、含まれる3変更の明示、短い試用、まとめて rollback という発見性である。したがって独立 engine を作らず、既存 profile の curated template として実装する場合にだけ価値がある。

### 4. 文書化された公開 API

新しい Windows write API は追加しない。既存 Action の根拠をそのまま使う。

- [Windows 11 settings reference](https://learn.microsoft.com/en-us/windows/apps/develop/settings/settings-windows-11) — light / dark mode と transparency effects の Windows 設定面。
- [DwmGetColorizationColor](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmgetcolorizationcolor) —現在の DWM colorization color と opaque blend 状態を取得する。
- [WM_SETTINGCHANGE](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange) — system-wide setting change の通知。
- [Color in Windows apps](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color) — color が personalization と階層表現に使われる設計資料。

既存 Action の apply / verify 根拠を超える OS 全体の theme control は主張しない。

### 5. 復元の正直さ

scene 適用前に3 Action それぞれの現値を保存し、既存 journal で逆順に戻す。部分失敗時は成功した Action だけを元値へ戻す。scene 自体が別の隠れた状態を持たないようにする。

試用中にユーザーが Windows Settings から同じ値を変更した場合は競合を検出し、後のユーザー操作を勝手に上書きしない。preview の色見本は概算表示であり、全アプリが同じ色になるとは表示しない。

### 6. 難易度とリスク

難易度は低〜中、危険度は既存3 Action の最大値に合わせる。engine の新規リスクは小さいが、scene 名から効果を盛って見せるマーケティングリスクがある。

「夜＝目に優しい」「集中＝生産性向上」のような効果表現は避け、「暗い配色」「透明効果なし」「青いアクセント」のように観測できる内容を列挙する。自動スケジュールは初版に含めない。

### 7. PC Custom の中心価値との一致

Action の組み合わせをユーザー成果へ翻訳し、既存 journal を再利用できる。新規 Windows mutation を増やさず「自分のPC感」を足せる。ただし manual profile との差は presentation に留まるため、優先度は上位3件より低い。

---

## 候補6: Explorer の情報ツールチップ設定を mutable に再判定

### 1. ユーザー語の成果

「Explorer でファイルにマウスを置いたときの説明ポップアップを、邪魔なら消し、必要なときだけ戻したい。」

対象は folder / desktop item の info tip 設定であり、taskbar の thumbnail、アプリ独自 tooltip、すべての Windows tooltip を消す機能ではない。

### 2. 実在する声と必要性

- [Give us an option to turn off tooltip popups](https://www.reddit.com/r/Windows11/comments/1mmz5b1) — 2025-08-11、調査時 `+2`。Explorer の file tooltip を止めたいという直接要望。
- [Windows Help: tooltip delay](https://www.reddit.com/r/WindowsHelp/comments/o8wue4) — 2021-06-27、調査時 `+1`。tooltip が早すぎて邪魔という声。

直接需要はあるが、票数は小さい。大きな需要とは扱わず、「表示のみ Action を公開 API で再評価する小さな実用候補」と位置づける。

### 3. 既存ツールと足りない点

Windows の File Explorer Options には「Show pop-up description for folder and desktop items」があり、単発変更なら Windows UI で十分である。一般的な tweaker も同種の setting を扱う。

PC Custom の追加価値は、quiet な作業用 profile へ組み込み、変更前値を保存し、戻したことを外から確認する点だけである。native checkbox より分かりやすくできないなら採用しない。

### 4. 文書化された公開 API

- [SHGetSetSettings](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetsetsettings) — Shell state setting の get / set を公開している。
- [SSF Constants](https://learn.microsoft.com/en-us/windows/win32/shell/ssf-constants) — `SSF_SHOWINFOTIP` が `SHELLSTATE.fShowInfoTip` を対象にすると明記している。

`SHGetSetSettings` の戻り値は `void` なので、値の再取得だけでなく、実際の新規 Explorer window に tooltip が出るかという外部確認が必要である。

### 5. 復元の正直さ

適用前の `fShowInfoTip` を保存し、同じ API で元値へ戻す。検証は PC Custom が用意した一意な test folder と既知 metadata の test item だけを使い、既存のユーザー窓を閉じない。

新規 Explorer window で hover して tooltip の可視性を外から観測し、適用後と復元後を比較する。現行 build で API 値だけ変わり表示に反映されない、または UI Automation で対象 tooltip を一意に確認できない場合は mutable に昇格しない。

### 6. 難易度とリスク

難易度は中、危険度は「安全」寄りの「注意」。設定自体の影響は小さいが、検証の flaky さが問題になる。hover delay、既存 window の cache、item 種類、desktop と folder の差、tooltip window の識別がある。

自己所有 test folder、fresh window、timeout、適用・復元の対称試験を必須にする。ユーザーの pointer を奪う UI Automation や、既存 Explorer window の強制 close は不可とする。

### 7. PC Custom の中心価値との一致

公開 setter がある表示のみ候補を、値の round-trip だけでなく outward verification で評価する良い小規模例になる。ただし成果が小さく Windows UI でも変更できるため、需要と中心価値への寄与は限定的である。

---

## 候補7: 対応 RGB をモード色にする

### 1. ユーザー語の成果

「普段・集中・ゲームなど、選んだモードに合わせて対応キーボードやマウスのライト色も変えたい。」

対象は Windows Dynamic Lighting / HID LampArray 対応機器だけである。メーカー独自 SDK、driver、process、firmware には触らない。

### 2. 実在する声と必要性

- [Windows 11 will get an integrated RGB controller](https://www.reddit.com/r/Windows11/comments/10ybb9z) — 2023-02-10、調査時 `+333`。複数メーカーの RGB utility を一つにしたいという期待が強い。
- [Dynamic Lighting interferes with Logitech onboard memory](https://www.reddit.com/r/LogitechG/comments/1ac4ty9/new_windows_11_dynamic_lighting_feature/) — 2024-01-27、調査時 `+60`。便利さだけでなく、Windows 側制御が既存 onboard setting と競合する実例がある。コメントには per-game control が足りないという声もある。

RGB 統合への需要は確認できるが、「PC Custom のモード色」への直接票ではない。また対応機器の母数は全 Windows PC ではない。

### 3. 既存ツールと足りない点

Windows Settings の Dynamic Lighting は全体 brightness、effects、対応アプリ優先度を提供する。SignalRGB、OpenRGB、各メーカー utility は対応機器や effect の幅で優位である。

PC Custom が狙えるのは、対応 LampArray の単色を既存 profile と一緒に扱う狭い範囲だけである。しかしメーカー utility との競合、foreground / background priority、USB port 変更による別 device 扱いがあり、専用 RGB tool の代替にはならない。

### 4. 文書化された公開 API

- [Dynamic lighting](https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/lighting-dynamic-lamparray) — Windows.Devices.Lights、対応 device、foreground / ambient background control、app priority、autonomous mode、package identity 要件を説明している。
- [LampArray class](https://learn.microsoft.com/en-us/uwp/api/windows.devices.lights.lamparray?view=winrt-26100) — `BrightnessLevel` は get / set 可能で、色は `SetColor`、`SetColorForIndex`、`SetColorsForIndices` 等で設定できる。
- [LampArray.SetColorsForIndices](https://learn.microsoft.com/en-us/uwp/api/windows.devices.lights.lamparray.setcolorsforindices?view=winrt-26100) — lamp index ごとの色設定。

決定的な問題は、公開された LampArray の method / property 一覧に、各 lamp の現在色を取得する対称 getter がないことである。`GetLampInfo` は lamp の静的情報であり、現在表示色の snapshot ではない。

### 5. 復元の正直さ

完全復元に必要な「適用前の各 lamp 色」を公開 API で取得できない。PC Custom が適用中に自分で設定した色は覚えられるが、初回適用前に Windows、firmware、メーカー utility が出していた色を再構成できない。

制御を解放すれば device は autonomous mode に戻れるが、それは「直前の色へ exact rollback」ではない。brightness は get / set 可能でも、色の欠落を埋めない。したがって mutable Action としては採用不可であり、設定ページへの guided link または検証 experiment に留める。

### 6. 難易度とリスク

難易度は高、危険度は「実験的」。対応機器列挙、availability、app priority、ambient app の package identity、USB port 単位の identity、メーカー utility 競合がある。さらに中心要件である exact rollback が API 形状上成立しない。

実装を始める条件は、Microsoft が現在色の取得または prior controller state の復元を文書化するか、PC Custom が触る前の状態へ確実に戻れる別の公開 API が確認できること。それまではコード化しない。

### 7. PC Custom の中心価値との一致

「自分のPC感」と mode composition には非常によく合い、需要も候補中では強い。しかし、楽しさは exact rollback より優先できない。保留とすること自体が「できることだけを、根拠と復元付きで出す」という PC Custom の中心価値に合う。

---

## 却下・保留・研究キュー

### 保留

- **対応 RGB のモード色**: 候補7のとおり、色 setter に対する現在色 getter がない。autonomous mode への解放は exact rollback ではない。
- **Explorer `SSF_*` の一括 mutable 化**: `SSF_ICONSONLY`、`SSF_SHOWSTATUSBAR`、`SSF_AUTOCHECKSELECT` なども公式 constants にあるが、documented flag の存在だけで39件をまとめて昇格させない。各項目に直接需要、現行 Windows 11 での outward verification、復元試験が必要。
- **配色 scene の自動スケジュール**: Auto Dark Mode と競合し、外部変更の所有権が曖昧になる。まず手動 scene と exact rollback に限定する。

### 却下

- **taskbar の全面 skin / Explorer injection**: 需要はあるが、内部要素への injection や patch は禁止範囲。モードリボンのアプリ所有 overlay までに限定する。
- **hot corner から任意 command、PowerShell、script、実行ファイルを起動**: 任意実行面を増やし、Action ごとの説明・危険度・rollback を迂回する。登録済み ID 以外は受け付けない。
- **全 capture device を常時監視して強制 mute / unmute**: 新しく接続された device やユーザーの後操作を上書きしやすい。初版は既定の通話 endpoint 1台だけ。
- **“true black”、全アプリ共通 theme、FPS・latency 向上の約束**: 公開 API と検証で裏づけられない。見た目の構成を性能効果へ言い換えない。
- **Round 1 の再提案**: startup action、winget batch、desktop icon / auto-arrange、wallpaper、display profile 等は今回の候補数に含めない。特に desktop icon auto-arrange の復元不成立と display profile の read / compare 限定を、新しい候補として言い換えない。

## まず作るならこの3つ

### 1位: 既定の通話マイクをシステム側でミュート

直接需要が最も明確で、Microsoft の get / set が対称にあり、端末ID単位の snapshot と復元を設計できる。初版を既定の通話 endpoint 1台に限定すれば、作用範囲も説明しやすい。実装前の acceptance は、同一 device ID で mute 値が apply / rollback し、既定端末を途中で変えても新端末を触らないこと。

### 2位: タスクバー上のモードリボン

Round 2 が求める「自分のPC感」を最も安全に出せる。Windows の taskbar state を変えず、既存の overlay 実験もある。初版は primary taskbar、click-through、fullscreen 時非表示、work area 非予約に絞る。深い taskbar styling ではなく、PC Custom の active mode indicator として作る。

### 3位: Explorer を別プロセスで開く設定の昇格

現在の39件の表示のみ項目を見直す、最も価値のある proof case である。公開 API は get / set を明記しているが、現行 Windows 11 の外部効果は未証明なので、最初の成果物は Action 本体ではなく検証 gate とする。get → set → 新規 test folder の process identity 確認 → rollback → 再確認を通った build だけで mutable に昇格させる。

この順なら、1件目で新しい実用 Action、2件目で安全な楽しさ、3件目で display-only catalog の質的見直しを進められる。いずれも Defender、firewall、Windows Update、pagefile、BCD、service 一括停止、process injection、game file / process、任意 script / binary には触れない。
