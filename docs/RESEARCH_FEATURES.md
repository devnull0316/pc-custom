# 追加機能リサーチ

調査日: 2026-07-28  
対象: Windows 11 / 日本語版「PCカスタム」

## 結論

需要、既存手段との差、Microsoft が文書化した公開手段、正確な復元、実機で効果を別経路から確かめられるかを基準に、次の順を推奨する。

| 優先 | 利用者が得る結果 | 需要 | 既存手段との差 | 公開手段と復元 | 難しさ | 判定 |
|---:|---|---|---|---|---|---|
| 1 | ゲーム中に Shift を連打・長押ししても、補助機能の確認画面に割り込まれない | 非常に強い | Windows の深い手動設定を、ゲーム中だけ安全に使う | 公開 API、対象設定の退避・復元可 | 小〜中 | **先に作る** |
| 2 | 画面外へ行ったウィンドウを、選んで今の画面へ戻す | 強い | PowerToys の要望が長期未充足。保存済み配置も不要 | 公開 API、1窓単位で復元可 | 小〜中 | **先に作る** |
| 3 | Windows 更新後、好みの設定が残っているかを確認し、必要なものだけ戻す | 強い | 一括 import や shell patch ではなく、出所・競合・不明を1件ずつ表示 | 読み取り中心。再適用も既存の安全な Action 限定 | 中 | **先に作る** |
| 4 | 電池使用時と電源接続時で「電池優先／バランス／性能優先」を使い分ける | 中〜強 | 旧来の電源プランと Windows 11 の電源モードを混同しない | 2025年更新の公式文書にある API。要求値と実効値を別表示 | 中 | 次段 |
| 5 | ゲームと作業でマウスポインターの加速・速さを切り替え、終了後に戻す | 反復あり（強度不明） | 固定の「最適化」ではなく用途別。効かないゲームも明示 | 公開 API、完全退避・復元可 | 小〜中 | 次段 |
| 6 | デスクトップアイコン配置を机・ドック構成ごとに保存し、崩れたら戻す | 反復あり（中） | 専用競合は既にあるが、窓配置やモードと同じ履歴で戻せる | 公開 Shell API、部分復元可 | 中 | 次段 |
| 7 | 横画面と縦画面で別々のローカル写真を巡回する | 反復あり（限定的） | 高機能な壁紙製品より狭く、ダウンロードなし、復元重視 | 公開 API。ただし開始状態の識別に残件あり | 中 | 研究枠 |
| 8 | 最大化中だけタスクバーを隠し、デスクトップでは表示する | 強い | 注入型 mod を使わず、Windows の公開手段だけに限定 | 公開 API。ただし全画面共通で、反映確認が必要 | 中 | 条件付き |
| 9 | 外出時は60Hz、電源接続・ゲーム時は選んだ高い更新頻度にし、必ず元へ戻す | 反復あり（限定的） | 既存の総合 display profile より対象を狭くする | 公開 API はあるが、黒画面リスクが大きい | 大 | 実験のみ |

ここでの「需要」は市場規模ではない。調査時点で確認した、互いに別の利用者投稿・Issue の反復度を示す。大きな票数の投稿が提案した機能そのものではなく周辺設定への関心を示すだけの場合は、強度を「不明」とした。票数は調査時点の概数であり、community 間で単純比較せず、製品判断も票数だけで行っていない。

## 調査と採否のルール

- [BRIEF](../BRIEF.md)、[README](../README.md)、[STATUS](STATUS.md) を契約・現状として扱った。現行67項目と同じ結果しか出さない案は除外した。
- 利用者の困りごとは Reddit、Microsoft Community、既存ツールの Issue などの原投稿で確認した。ツールの機能は、可能な限り公式ドキュメント・公式 repository・公式 changelog で確認した。
- Microsoft Learn に setter があるだけでは出荷可とはしない。`apply` と同じ保存領域を読み直すだけの緑のテストは、Windows 上の効果を証明しない。
- 各候補は、テスト所有の対象で、setter とは別の getter、UI Automation、画面上の矩形、実効通知などから結果を観測できることを出荷条件にする。観測できなければ表示は「不明」のままにする。
- 復元は、存在有無・型・raw 値・対象 identity を含む適用直前状態を耐久保存する。現在値が自分の適用値と一致するときだけ戻し、利用者や他アプリによる第三の変更は上書きしない。
- 管理者権限で常駐しない。他 process への注入、非公開 shell hook、pattern / symbol 依存、任意スクリプト、出所不明バイナリは候補に含めない。

## 1. ゲーム中の Shift 割り込みガード

**判定: 先に作る。** 初版は Sticky Keys と Filter Keys の「起動用ショートカット」だけを対象にし、補助機能そのものは無効化しない。

1. **何ができるようになるか（利用者の言葉）**  
   登録したゲーム中だけ、Shift の5回連打や右 Shift の長押しで確認画面が現れないようにする。ゲームを終えたら、ゲーム前の状態へ戻る。普段その補助機能を使っている人には、既定では適用しない。

2. **なぜ必要か**  
   r/windows では「ゲームで Shift を多用すると確認画面に割り込まれる。設定のさらに奥に別のショートカット設定があると気づかなかった」という報告がある（[2024-05-26](https://www.reddit.com/r/windows/comments/1d1551n/is_there_a_way_to_disable_sticky_keys_entirely/)）。右 Shift 長押しで Filter Keys の画面が出てゲームを中断する別報告もある（[r/techsupport, 2024-05-20](https://www.reddit.com/r/techsupport/comments/1cwiu00/slight_annoyance_with_sticky_keys_windows_11/)）。同種の投稿は r/pcmasterrace で調査時約5,600票を集め、更新後に設定が戻ったというコメントもある（[2026-06-23](https://www.reddit.com/r/pcmasterrace/comments/1udqbmh/no_one_in_the_history_of_pc_gaming_ever_activated/)）。

3. **既存ツールはどうしているか、なぜ足りないか**  
   Windows 自身に手動設定はあり、Microsoft も Sticky Keys と Filter Keys を「設定 > アクセシビリティ > キーボード」で調整すると案内している（[Microsoft Support](https://support.microsoft.com/en-US/accessibility/windows/make-your-mouse-keyboard-and-other-input-devices-easier-to-use)）。不足しているのは機能の存在ではなく、ゲーム前に深い設定を探し、終了後に元のアクセシビリティ設定へ戻す手間である。PCカスタムには既に実行ファイル identity を確認するゲームプロファイルと、終了時にそのプロファイルが変えた項目だけ戻す仕組みがある。

4. **Windows のどの公開手段で実現できるか**  
   文書化済みの `SystemParametersInfoW` に `SPI_GET/SETSTICKYKEYS` と `SPI_GET/SETFILTERKEYS` がある（[Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow)）。[STICKYKEYS](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-stickykeys) と [FILTERKEYS](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-filterkeys) の全構造を読み、`SKF_HOTKEYACTIVE` / `FKF_HOTKEYACTIVE` だけを一時解除する。確認なしで機能を起動し得るため `CONFIRMHOTKEY` だけを外す実装にはせず、機能本体の on/off bit にも触れない。管理者権限・Explorer 再起動は不要。

5. **元に戻せるか**  
   戻せる。両構造の GET 結果は全 field を適用直前に保存するが、比較・復元の所有対象は setter が制御できる安定した flag / parameter だけにする。Sticky Keys の latch / lock bit は setter が無視すると公式文書にあるため、保存は診断用とし、設定値として replay しない。ゲーム終了時に「現在の所有対象 = 自分の適用値」を確認して元の安定設定を戻し、第三の変更があれば止める。出荷前にはテスト用ユーザー環境で Shift 5回・右 Shift 長押しを実際に発生させ、適用中は確認 UI が出ないこと、元の hotkey が有効だった場合は復元後に再び出ることを UI Automation でも確認する。構造の GET/SET round-trip だけでは合格にしない。

6. **実装の難しさと、いちばん危ないところ**  
   **小〜中。** 最大の危険は、アクセシビリティ機能を必要とする人の入力経路を奪うこと。Sticky / Filter Keys の機能本体が on、Sticky Keys の latch / lock bit が残っている、または状態取得不能なら適用しない。利用者属性を推測せず全員に明示 opt-in、変更内容の事前表示、キーボードだけで使える緊急解除を必須にする。

7. **この製品の芯と噛み合う理由**  
   「補助機能名を知る」ではなく「ゲームを中断されたくない」という結果から選べる。ゲーム中だけ適用し、1件の履歴として正確に戻せるため、既存のプロファイル・lease・競合検知をそのまま生かせる。

## 2. 見失ったウィンドウを今の画面へ戻す

**判定: 先に作る。** 保存済みレイアウトの汎用化ではなく、「今、画面外にいる1窓を救う」に絞る。

1. **何ができるようになるか（利用者の言葉）**  
   ドックを外した後などに見えなくなったアプリを一覧から選び、「この画面へ戻す」を押す。どの窓をどこへ動かすかを先に表示し、アプリの再起動や暗記したキー操作を求めない。

2. **なぜ必要か**  
   PowerToys の「Move Hidden Windows to active screen」は、外部画面切断・負座標・電源を切った画面に窓が残る具体例とともに2019年から未解決である（[#557](https://github.com/microsoft/PowerToys/issues/557)）。5画面環境で dock/undock のたびに3〜5分かけて再配置する要望もある（[#261](https://github.com/microsoft/PowerToys/issues/261)）。r/Windows11 でも、回復に `Alt+Space` などの手順を覚える必要があるという投稿（[2022-07-24](https://www.reddit.com/r/Windows11/comments/w6udp1/)）や、設定画面自身まで画面外へ行き「単純なクリックで戻したい」という投稿（[2022-10-19](https://www.reddit.com/r/Windows11/comments/y7q1xl/)）がある。

3. **既存ツールはどうしているか、なぜ足りないか**  
   [FancyZones](https://learn.microsoft.com/en-us/windows/powertoys/fancyzones) は窓を定義済み zone へ配置し、[PowerToys Workspaces](https://learn.microsoft.com/en-us/windows/powertoys/workspaces) はアプリを起動・移動して保存配置を再現する。しかし Workspaces は起動後に窓を移動するため動きが見え、昇格アプリの再配置や snap 状態にも制約がある。現行の `setup.window_layout` も「先に保存した配置の復元」であり、既に見失った窓を保存なしで救う用途ではない。

4. **Windows のどの公開手段で実現できるか**  
   `EnumWindows`、`GetWindowPlacement`、`GetWindowRect`、`MonitorFromRect` / `MonitorFromWindow`、`GetMonitorInfo`、`SetWindowPlacement` という文書化済み API で構成できる。Microsoft も複数画面での位置確認と保存・復元にこれらを使うよう説明している（[Positioning Objects on Multiple Display Monitors](https://learn.microsoft.com/en-us/windows/win32/gdi/positioning-objects-on-multiple-display-monitors)）。現行 `setup.window_layout` の identity・除外規則も再利用できる。

5. **元に戻せるか**  
   戻せる。選んだ窓について PID、process creation time、HWND、完全な `WINDOWPLACEMENT` と矩形を直前保存し、現在の monitor work area に必要最小限だけ入る位置へ動かす。「画面外」は `MonitorFromWindow(...NEAREST)` の結果では判定せず、窓矩形と全 monitor の `rcWork` の交差が最小可視幅・高さを満たさないことを確認する。`MONITOR_DEFAULTTONULL` も補助判定に使う。「現在 = 移動後」かつ同じ process/window instance のときだけ元座標へ戻し、テスト所有窓の UI Automation bounding rectangle と `rcWork` の交差から実際の可視化を確認する。

6. **実装の難しさと、いちばん危ないところ**  
   **小〜中。** 最大の危険は HWND 再利用や曖昧な列挙で別の窓を動かすこと。昇格窓、system UI、cloaked / tool / owned / popup、全画面、identity を読めない窓、現在移動中の窓は fail-closed で除外する。最大化・最小化状態は矩形だけで上書きしない。

7. **この製品の芯と噛み合う理由**  
   API 名ではなく「見えない窓を戻す」という明確な結果で、変更対象は利用者が選んだ1窓だけ。適用前状態をその場で確保でき、既存の厳密なウィンドウ identity と1件 rollback が差別化になる。

## 3. Windows 更新後のカスタマイズ健全性レポート

**判定: 先に作る。** 第1段階は完全 read-only。第2段階でも、検証済み Action を利用者が1件ずつ明示再適用するだけにする。

1. **何ができるようになるか（利用者の言葉）**  
   Windows の更新後に「基準どおり」「基準との差分あり（原因は不明）」「競合の可能性」「この版では確認できない」を一覧で見る。戻したい項目だけ選び、今の状態を採用する項目はそのままにできる。

2. **なぜ必要か**  
   Windows 11 22H2 後に通知領域の好みが戻り、別の利用者も file association などが戻ったと報告した投稿がある（[r/Windows11, 2023-01-28、約40票](https://www.reddit.com/r/Windows11/comments/10nax3a/22h2_reset_my_notification_area_preferences/)）。更新は shell 改変ツールにも繰り返し影響する。ExplorerPatcher では KB5089573 後に Explorer が繰り返し crash し、ExplorerPatcher を無効化すると再現しないという報告がある（[#4987](https://github.com/valinet/ExplorerPatcher/issues/4987)）。[ExplorerPatcher の公式 releases](https://github.com/valinet/ExplorerPatcher/releases) も特定 build の crash 修正や将来更新前の更新要求を継続的に記録している。StartAllBack も 24H2 対応や月例更新との互換修正を changelog に重ねている（[公式 changelog](https://www.startallback.com/)）。

3. **既存ツールはどうしているか、なぜ足りないか**  
   [Winaero Tweaker](https://winaero.com/winaero-tweaker/) は import/export と数百規模の tweak を持つ一方、公式発信でも新しい Windows 11 で古い項目を隠す修正や、XMouse を元に戻せない不具合修正が記録されている（[Winaero 公式 channel](https://t.me/s/winaero/8497)）。ExplorerPatcher、StartAllBack、Windhawk は shell の機能幅が広いが、更新 build・hook・symbol への追随が必要になる。[Windhawk の公式 troubleshooting](https://github.com/ramensoftware/windhawk/wiki/Troubleshooting) も、Windows 版、debug symbol、AV、mod の相性を不動理由として挙げる。PCカスタムの現行「現在設定の控え」は read-only JSON で、差分からの1件再適用はまだない。ここでは機能幅で競わず、provenance と破損半径の小ささで差別化する。

4. **Windows のどの公開手段で実現できるか**  
   比較基準は、利用者が明示選択した既存 config snapshot、または Action ごとの直近成功適用値だけにする。基準には取得時刻、OS build、Action version を保存し、旧 version の値を現 version へ逆写像できない項目は `unknown` とする。OS build の変化と各 Action の既存 detector を使い、別系統の effective-state probe がある項目だけ画面効果まで判定する。probe がない項目は configured-only と表示する。更新履歴との時系列表示には、更新イベント履歴を読み取る文書化済み `IUpdateSearcher::QueryHistory` を使える（[Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nf-wuapi-iupdatesearcher-queryhistory)）。これは履歴を読むだけで、Windows Update の検索・停止・設定変更には使わない。再適用は、その build で互換性確認済みの既存公開 setter だけに限定する。

5. **元に戻せるか**  
   レポートだけなら Windows を変更しない。再適用時は「更新後に今ある値」を新しい適用直前値として journal に保存し、その再適用だけを1件 rollback できる。以前の古い backup を無条件で流し込まない。「現在 = 再適用値」のときだけ更新後状態へ戻し、第三値・unknown build・測定不能では自動処理しない。

6. **実装の難しさと、いちばん危ないところ**  
   **中。** 最大の危険は、更新履歴との時間的な近さを「Windows Update が原因」と断定することと、registry 値の一致を画面効果と誤認すること。表示は常に「更新後に見つかった差分」とし、因果は主張しない。effective probe がない項目は「設定値は一致／見た目は測定不能」と分ける。

7. **この製品の芯と噛み合う理由**  
   exact backup、1件履歴、第三者変更を上書きしない規則、unknown を unknown のまま出す姿勢が、そのまま利用者価値になる。広範な shell patch より「何が今どうなっていて、どれだけ安全に戻せるか」を売りにできる。

## 4. Windows 11 の電源モードを用途別に使い分ける

**判定: 次段。** 旧来の電源プラン切替とは別 Action にし、要求したモードと実際に有効なモードを混同しない。初版は AC/DC の明示設定と利用者が押す一時モードだけに限定し、ゲーム起動との自動連動は対象外とする。

1. **何ができるようになるか（利用者の言葉）**  
   電池使用時は「電池優先」、電源接続時は「バランス」など、Windows 11 の3つの電源モードを分かりやすく選ぶ。利用者が手動で始める一時モードだけ、終わったら元へ戻すこともできる。ただし性能向上や静音化を保証しない。

2. **なぜ必要か**  
   Windows 10 のように電源接続／電池で自動的に使い分けたいが、Windows 11 では毎回設定を開く必要があるという投稿がある（[r/Windows11, 2022-03-31](https://www.reddit.com/r/Windows11/comments/tsqmbs/w11_power_mode_automation/)）。「旧来の電源プラン」と「新しい電源モード」のどちらが使われるのか分からないという投稿は調査時64票で、ゲームと browsing の切替を quick setting に欲しいという声もある（[2023-05-22](https://www.reddit.com/r/Windows11/comments/13oqmha/windows11_has_two_power_options_one_is_in_the/)）。Microsoft 自身も、作業内容に合わせ、電源接続時と電池時に別の mode を選ぶものとして案内している（[Microsoft Support](https://support.microsoft.com/en-au/windows/change-the-power-mode-for-your-windows-pc-c2aff038-22c9-f46d-5ca0-78696fdf2de8)）。

3. **既存ツールはどうしているか、なぜ足りないか**  
   現在の Windows 設定は AC/DC を手動で別々に選べるため、単なる設定画面の複製には価値が薄い。[PowerPlanSwitcher](https://github.com/SebastianBecker2/PowerPlanSwitcher) と現行 `power.active_scheme_switch` は主に旧来の power plan / scheme を切り替える。足す価値があるのは、modern standby 機を含む Windows 11 の「mode vote」を plan と分けて説明し、実効状態まで確認し、PCカスタムの一時モードに安全に組み込む部分である。

4. **Windows のどの公開手段で実現できるか**  
   2025-02-28 更新の公式文書に、AC 用 `PowerGet/SetUserConfiguredACPowerMode` と DC 用 `PowerGet/SetUserConfiguredDCPowerMode` が公開されている（[AC getter](https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-powergetuserconfiguredacpowermode)、[AC setter](https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-powersetuserconfiguredacpowermode)、[DC getter](https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-powergetuserconfigureddcpowermode)、[DC setter](https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-powersetuserconfigureddcpowermode)）。受け付けるのは「Best Power Efficiency / Balanced / Best Performance」の3値だけで、Windows 11 が最小要件である。実際の有効 mode は `PowerRegisterForEffectivePowerModeNotifications` で別に観測する（[Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/powersetting/nf-powersetting-powerregisterforeffectivepowermodenotifications)）。公式文書に昇格や再起動の要件はないが、標準ユーザー実機で feature probe と error 伝播を確認してから有効化する。

**実機での追加所見（2026-07-28、build 26200 のデスクトップ1台）**  
`PowerGet/SetUserConfigured{AC,DC}PowerMode` は windows-rs 0.58 のメタデータに無く、手書き FFI になる。
`GetProcAddress` で実行時に引くこと。静的リンクは、エクスポートが無い環境でプロセスごと起動不能にする。

署名は当てずに測った。両 getter は文書化された overlay GUID を返し、別 API の
`PowerRegisterForEffectivePowerModeNotifications` も同じ方向を報告した。ただし
**値は一致しない**（要求 `BestPerformance` / 実効 `MaxPerformance`）。要求値と実効値を1つの
「現在のモード」に混ぜてはいけない、という設計判断はこの実測で裏が取れている。

**3値すべてが書けるとは限らない。** この機は AC・DC どちらでも「電池優先」を
`ERROR_INVALID_PARAMETER` で拒否し、「バランス」「パフォーマンス優先」は書けた。
最初の往復テストは元と同じ値を書いていたため、これを success として通してしまった。
供給ごとに**必ず今と違う値**を書いて読み直すまで、拒否は見えない。
実装では、選べない値を事前に断定せず、拒否されたら「この PC ではこのモードを選べません」と
そのまま伝えること。書き込み前に「選べる」と約束しない。

5. **元に戻せるか**  
   戻せる。AC と DC の元 GUID を別々に保存し、変更した側ごとに現在の user-configured value が自分の適用値なら元へ戻す。active power plan はこの Action では変更しない。Microsoft は user-configured mode を「他の system signal に上書きされ得る vote」と明記しているため、getter の要求値と effective notification の値を別表示する。登録直後に複数 callback が来る可能性を考慮して debounce 後の最終値を採用し、未着は `unknown`、不一致時は「要求値は保存済み／Windows 報告の実効 mode は不一致（理由不明）」とする。

6. **実装の難しさと、いちばん危ないところ**  
   **中。** 最大の危険は、API 成功を性能・消費電力への実効反映と誤表示すること。custom power plan、OEM utility、Battery Saver、Game Mode などが優先する場合がある。API 不在、未対応機、Balanced 以外の plan で mode が利用不能、実効通知が得られない場合は変更しない。

7. **この製品の芯と噛み合う理由**  
   「高速化」ではなく「電池優先／バランス／性能優先を用途で使い分ける」という結果から選べる。現行の電源プランとの違いを初心者へ説明でき、要求値と実効値を正直に分け、モード終了時に正確に戻せる。

## 5. ゲーム／作業ごとのマウスの感触

**判定: 次段。** 「照準が良くなる」「FPS が上がる」とは一切表現せず、Windows ポインター設定を用途別に戻す機能とする。

1. **何ができるようになるか（利用者の言葉）**  
   ゲームでは加速なし、普段の作業では加速あり、という自分の好みをプロファイルに保存する。ゲーム終了時には、速度と加速をゲーム前の値へ戻す。

2. **なぜ必要か**  
   r/pcmasterrace の「新しいPCでは pointer precision を確認しよう」という投稿は調査時約30,000票を集めた一方、コメントには modern game の raw input では影響しないという重要な反論も多い（[2022-12-25](https://www.reddit.com/r/pcmasterrace/comments/zuzz2i/for_all_the_new_pc_gamers_this_year_a_friendly/)）。これは固定設定への関心の大きさは示すが、用途別 profile への直接需要の強さまでは示さない。直接確認できたのは、r/Windows11 で「ゲームでは off と言われるが、仕事では小さい UI や複数画面に on が使いやすい」という用途差（[2024-01-17](https://www.reddit.com/r/Windows11/comments/198wrn3/should_i_disable_mouse_acceleration/)）と、ログオン・他アプリ・再起動後に設定が戻り毎回直すという報告（[2022-11-20](https://www.reddit.com/r/pcmasterrace/comments/z02tuz/enhance_pointer_precision_in_widows_11/)）であり、profile 機能の需要強度は不明とする。

3. **既存ツールはどうしているか、なぜ足りないか**  
   Windows 11 の標準 Settings には pointer speed と Enhance pointer precision の手動設定がある（[Microsoft Support](https://support.microsoft.com/en-us/windows/hardware/input-devices/change-mouse-settings)）。PowerToys の [Mouse Utilities](https://learn.microsoft.com/en-us/windows/powertoys/mouse-utilities) は locator、crosshairs、jump などを提供するが、公開機能一覧には用途別の加速 profile と exact rollback はない。固定の「最適値」を押しつけず、PCカスタムのゲーム／手動モードで一時適用するところに差がある。

4. **Windows のどの公開手段で実現できるか**  
   `SystemParametersInfoW` の `SPI_GET/SETMOUSE` は2つの threshold と acceleration を3整数で、`SPI_GET/SETMOUSESPEED` は速度1〜20を取得・設定できる（[Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow)）。文書化済みで、管理者権限・再起動は不要。

5. **元に戻せるか**  
   戻せる。3整数と speed を raw のまま保存し、現在値が自分の適用値なら全て元へ戻す。他の mouse utility が途中で変えたら競合として止める。出荷試験では GET/SET 一致や synthetic input だけでなく、テスト専用 HID の同じ物理入力量と `GetCursorPos` で pointer displacement の差と復元を観測する。Microsoft は Raw Input の mouse event は Control Panel の mouse speed の影響を受けないと明記しているため（[RAWMOUSE](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rawmouse)）、raw input のゲームには「Windows 設定の対象外」と表示する。

6. **実装の難しさと、いちばん危ないところ**  
   **小〜中。** 最大の危険は、効かないゲームに効くと誤認させることと、vendor utility が同じ設定を上書きすること。適用対象は Windows pointer path に限る。ゲーム個別の raw input 利用有無は一般に断定できないため、未知は未知とする。

7. **この製品の芯と噛み合う理由**  
   好みが分かれる設定こそ「最適化」ではなく結果・用途で選び、終了後に戻す設計が合う。小さな公開 API Action としてゲームプロファイルへ自然に追加できる。

## 6. デスクトップアイコン配置の保存・復元

**判定: 次段。** 単体機能は競合が強いため、`setup.window_layout` と同じ机・ドック profile に統合できる場合に作る。

1. **何ができるようになるか（利用者の言葉）**  
   自宅の2画面、外出先の1画面など、使い方ごとにデスクトップアイコンの並びを保存する。更新・再起動・画面接続で崩れたら、移動内容を見てから元の並びへ戻す。「復元を取り消す」こともできる。

2. **なぜ必要か**  
   Windows 11 更新後、起動のたびにアイコンが動き、別利用者は第2画面から主画面へ毎日戻ると報告している（[r/Windows11, 2022-05-16](https://www.reddit.com/r/Windows11/comments/uqy82o/desktop_icons_keep_moving_after_i_organize_them/)）。Microsoft Community には再起動時に2画面の配置が片側へ集約する質問があり、20人以上が同じ質問として反応している（[2023-07-21](https://answers.microsoft.com/en-us/windows/forum/all/windows-11-desktop-icons-rearrange-and-re-sort/3122fff9-8c06-4336-9569-df9a5fd80591)）。PowerToys にも再起動をまたぐ位置・group 保存の要望が2020年から残る（[#1392](https://github.com/microsoft/PowerToys/issues/1392)）。

3. **既存ツールはどうしているか、なぜ足りないか**  
   [DesktopOK 公式](https://www.softwareok.com/?seite=Freeware%2FDesktopOK) は複数配置の保存・復元・自動保存を既に提供し、[DisplayFusion](https://www.displayfusion.com/HelpGuide/DisplayFusionBeginnersGuide/?PDF=1) も Desktop Icon Profiles を持つ。従って、単なる同等品は差別化にならない。PCカスタムで作る理由は、窓配置・他のモード項目と同じ preview / journal に載せ、復元直前の配置へ1件ずつ戻し、曖昧な icon を勝手に移動しない点に限られる。

4. **Windows のどの公開手段で実現できるか**  
   `IFolderView::GetItemPosition` と `IFolderView::SelectAndPositionItems` が文書化されている（[IFolderView](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ifolderview)）。Microsoft の Raymond Chen も、非文書化 ListView message は Windows 10 1809 で壊れ、supported API は `SelectAndPositionItems` だと説明している（[The Old New Thing](https://devblogs.microsoft.com/oldnewthing/20211122-00/?p=105948)）。非公開 Explorer 内部や injection は不要。

5. **元に戻せるか**  
   同じ desktop view と item identity を再確認できる範囲では戻せる。保存済み配置を適用する直前に現在配置も snapshot し、適用後位置と一致する item だけを直前位置へ戻す。rename / delete / new item、OneDrive Desktop の切替、monitor identity・DPI・解像度の不一致は個別 skip として表示する。`GetItemPosition` に加え、テスト用 icon の UI Automation bounding rectangle で画面上の効果を確認する。

6. **実装の難しさと、いちばん危ないところ**  
   **中。** 最大の危険は reboot や OneDrive 移動をまたぐ item identity の取り違え。PIDL を保存すれば永続 identity になるとは仮定せず、filesystem identity と Shell identity の寿命を実機で検証する。Auto Arrange が有効、desktop view が取得不能、複数 item が曖昧なら変更しない。

7. **この製品の芯と噛み合う理由**  
   「アイコン座標を編集」ではなく「机を元の状態へ戻す」という結果を、窓配置と一緒に扱える。競合より機能数は少なくても、変更前状態・部分失敗・第三者変更を1件ずつ説明して戻す点が製品の芯になる。

## 7. モニター別のローカル壁紙プレイリスト

**判定: 条件付き研究枠（出荷保留）。** 元状態が monitor 別の静止画であるだけでは足りず、背景が実際に有効だったことまで一意に観測できる環境だけを対象候補にする。ローカル画像だけを扱い、壁紙検索・ダウンロード・生成は行わない。

1. **何ができるようになるか（利用者の言葉）**  
   横画面には横写真のフォルダー、縦画面には縦写真のフォルダーを割り当て、それぞれ別の順番・間隔で巡回する。1画面へ戻したときや機能を止めたときは、開始前の壁紙へ戻す。

2. **なぜ必要か**  
   Windows 11 で「主画面は固定、2画面目だけ slideshow」にしたいが、1つの folder が両画面へ適用されるという投稿がある（[2022-01-07](https://www.reddit.com/r/Windows11/comments/ryj1s2/different_wallpaper_slideshow_for_each_monitor/)）。横用と縦用を分けても逆の画面へ出るという投稿（[2023-02-09](https://www.reddit.com/r/Windows11/comments/10y9rxl/different_wallpaper_on_different_screen/)）、横2枚・縦1枚で別々の画像群を巡回したいという投稿（[2023-04-20](https://www.reddit.com/r/Windows11/comments/12sfhro/possible_to_have_different_slideshow_wallpapers/)）もあり、複数年にわたり同じ需要が続く。

3. **既存ツールはどうしているか、なぜ足りないか**  
   Windows の公開 `IDesktopWallpaper` は monitor 別の静止画を扱う一方、slideshow source / options は system-wide の1組として公開する（[Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-idesktopwallpaper)）。[DisplayFusion の公式 guide](https://www.displayfusion.com/HelpGuide/WorkingWithDisplayFusionMonitorProfiles/?PDF=1) は monitor profile に Wallpaper Profile を関連付けて自動 load でき、競合の機能幅は既に広い。PCカスタムには壁紙 Action 自体がなく、既存の theme schedule は app / system の light・dark を扱う別機能である。作るなら動画、web content、plugin、download を扱わず、「手元の写真を画面の向きごとに巡回し、元へ戻す」に絞る。

4. **Windows のどの公開手段で実現できるか**  
   `IDesktopWallpaper` は monitor の固有 path・矩形、現在の wallpaper、表示方法、slideshow source・shuffle・間隔の GET と、`SetWallpaper` / `SetSlideshow` を公開している（[Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-idesktopwallpaper)）。独立 folder は OS の単一 slideshow 契約ではないため、アプリが起動中に monitor ごと `SetWallpaper` する scheduler として実装する。

5. **元に戻せるか**  
   元が各 monitor の静止画で、その path・file hash と monitor identity を lossless に読め、かつ read-only の実画面観測から背景が有効だったことを一意に証明できる場合だけ戻せる候補とする。`IDesktopWallpaper::Enable` には対応する純粋な getter がなく、`GetStatus` が返すのは slideshow 状態なので、静止画 path の取得だけでは開始状態を復元可能と判定しない（[GetStatus](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-idesktopwallpaper-getstatus)）。この Action は表示 position と background color を変更せず、復元の所有対象にも含めない。現在 path / hash が自分の最後の適用画像と一致し、背景 enabled を再証明でき、さらに元画像の file が存在して開始時 hash と一致する monitor だけ元へ戻す。元 file が削除・編集されていれば第三値として `unknown` と手動案内にする。OS slideshow は現在位置・random sequence まで正確に再現できないため、Windows Spotlight、theme 管理、背景有効状態を証明できない環境、path を再構成できない source、切断された monitor とともに初版では適用しない。共有 profile へローカル path を含めない。

6. **実装の難しさと、いちばん危ないところ**  
   **中。** 最大の危険は、Spotlight や theme が管理する状態を静止画で上書きして元へ戻せなくすること。公開 getter と実画面の別経路観測を合わせても元状態を一意に再構成できなければ、guided / display-only に止める。scheduler の各 tick 前に monitor ごとの現在 path / hash だけでなく、背景 enabled の実画面観測も再証明する。どちらかが自分の直前適用状態と異なるか不明なら、利用者・theme による第三値として即座に ownership を放棄し、それ以上 `SetWallpaper` しない。テスト所有画像は desktop capture でも実描画を確認する。hotplug・sleep・DPI 変更時は stable monitor fingerprint が戻るまで更新しない。

7. **この製品の芯と噛み合う理由**  
   「壁紙エンジン」ではなく「縦横それぞれに合う手元の写真」という初心者向けの結果に限定できる。開始状態の識別を解決できた場合だけ、外部 download なし、monitor 単位の履歴、停止時の exact rollback という安全な小機能になり得る。

## 8. 最大化中だけタスクバーを隠す

**判定: 条件付き。** まず global taskbar で実機互換性を測り、monitor ごとの制御や shell injection は行わない。

1. **何ができるようになるか（利用者の言葉）**  
   普段は時計や通知を見るためタスクバーを表示し、作業アプリを最大化したときだけ隠す。デスクトップへ戻ると自動で表示する。常時 auto-hide より、必要なときだけ画面を広く使える。

2. **なぜ必要か**  
   r/Windows11 の「Intelligent Auto Hide」は、最大化・taskbar と重なる窓のときだけ隠したいという内容で186票を集め、windowed game で困るという賛同もある（[2021-11-01](https://www.reddit.com/r/Windows11/comments/qkhhup/windows_11_taskbar_needs_intelligent_auto_hide/)）。同じ要望は2024年（[最大化時だけ](https://www.reddit.com/r/Windows11/comments/1balxgg/auto_hide_taskbar_for_maximized_windows_only/)）と2025年（[full-screen/maximized 時だけ](https://www.reddit.com/r/Windows11/comments/1p6xdrq/windows_11_taskbar_hiding/)）にも続く。

3. **既存ツールはどうしているか、なぜ足りないか**  
   Windhawk には「Taskbar auto-hide when maximized」mod があり、需要を実際に満たしている（[利用者・作者による紹介](https://www.reddit.com/r/Windows11/comments/1grc0x7/auto_hiding_only_when_a_window_is_maximized_or/)）。ただし Windhawk の公式 wiki は engine が多数 process へ injection し、対象を変えると mod の不動や system instability があり得ると説明する（[Injection targets](https://github.com/ramensoftware/windhawk/wiki/Injection-targets-and-critical-system-processes)）。現行の taskbar 5項目は alignment、search、Show Desktop、Task View、Widgets で、auto-hide は含まない。本製品で足す意味は、機能を狭くし、公開 appbar state と foreground window の観測だけで行い、注入しない場合に限る。

4. **Windows のどの公開手段で実現できるか**  
   `SHAppBarMessage(ABM_GETSTATE / ABM_SETSTATE)` は taskbar の auto-hide state の取得・設定を文書化している（[SHAppBarMessage](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shappbarmessage)、[ABM_SETSTATE](https://learn.microsoft.com/en-us/windows/win32/shell/abm-setstate)）。常駐 hook は使わず、低頻度・debounce 付きで `GetForegroundWindow` と `GetWindowPlacement` を読み、foreground 窓の最大化／復元を再判定する。管理者権限、Explorer injection、symbol は不要。

5. **元に戻せるか**  
   Windows 11 で所有・復元するのは `ABS_AUTOHIDE` bit だけとし、元の on/off を耐久 journal に保存して、現在が自分の適用 state と一致するときだけ戻す。app が異常終了した場合も、次回起動時に同じ一致条件を満たすときだけ recovery する。`ABM_GETSTATE` は Windows 7 以降 `ABS_ALWAYSONTOP` を返さないため、その bit を保存できるとは扱わない。重要なのは `ABM_SETSTATE` が常に `TRUE` を返すため、その return を成功証拠にできないこと。`ABM_GETSTATE`、taskbar 矩形、monitor work area、テスト用最大化窓からの UI 観測を合わせて反映を確認する。

6. **実装の難しさと、いちばん危ないところ**  
   **中。** 最大の危険は taskbar の既知の auto-hide / focus 問題を悪化させ、foreground の短い変化に追従してちらつくこと。debounce、全画面 game・shell UI・通知での除外、手動変更を即座に ownership 放棄する規則が必要。公開 API は system taskbar 全体の state であり、monitor ごとの auto-hide ができるとは書かない。

7. **この製品の芯と噛み合う理由**  
   「taskbar registry」ではなく「最大化したときだけ広く使う」という結果で選べる。競合の injection 方式を採らず、1つの公開状態を期限付きで所有し、いつでも元へ戻すという狭さなら製品方針に合う。

## 9. 更新頻度を含む安全な表示プロファイル

**判定: 実験のみ。** HDR、GPU vendor 設定、既定音声、program 起動を含む総合 profile にはしない。初期実験は、同じ logon session・接続構成で利用者が Windows Settings から一度ずつ選び、full CCD 構成として取得した2状態を15秒だけ切り替えるところまでに限定する。

1. **何ができるようになるか（利用者の言葉）**  
   最初に Windows Settings で60Hzと高い更新頻度を一度ずつ選んで2状態を記録する。その同じ session 中だけ、電池使用時と電源接続時などに記録済み状態を切り替え、終了後は元へ戻す。画面が見えなくなったら、何も押さなくても短時間で元へ戻る。

2. **なぜ必要か**  
   120Hz は良いが電池消費が大きく、電源を抜いたとき60Hzへすぐ切り替えたいという投稿がある（[r/Windows11, 2024-11-19](https://www.reddit.com/r/Windows11/comments/1gurqll/changing_quickly_from_120hz_to_60hz/)）。作業60Hz・ゲーム144Hzの2 profile を求める投稿（[2025-02-08](https://www.reddit.com/r/Windows11/comments/1ikoaau/how_do_i_enable_different_display_settings_for/)）、再起動ごと165Hzから60Hzへ戻る投稿（[r/pcmasterrace, 2023-03-25](https://www.reddit.com/r/pcmasterrace/comments/121n51c/windows_resetting_refesh_rate_to_60hz_after_restart/)）、game 終了後も60Hzのままという投稿（[2023-05-13](https://www.reddit.com/r/pcmasterrace/comments/13g29xm/help_fullscreen_programs_keep_resetting_my_refresh/)）がある。独立投稿は反復しているが、票数はいずれも小さく、需要強度は限定的と扱う。

3. **既存ツールはどうしているか、なぜ足りないか**  
   [DisplayMagician](https://github.com/terrymacdonald/DisplayMagician) は refresh、HDR、NVIDIA Surround / AMD Eyefinity、audio、program 起動、game 終了後の profile 復元まで既に提供するため、総合 profile で競う価値は低い。公式 release は Windows 10 から11への移行時に profile の再作成が必要だったことを記録する（[releases](https://github.com/terrymacdonald/DisplayMagician/releases)）。全 display が接続済みでも profile を使えない bug も報告されている（[#348](https://github.com/terrymacdonald/DisplayMagician/issues/348)）。現行 PCカスタムは Hz を read-only readiness として確認するだけであり、安全な trial が成立するかを調べる余地だけがある。

4. **Windows のどの公開手段で実現できるか**  
   `QueryDisplayConfig` は現在の active path、source / target mode、orientation、scaling、connector 等を取得し（[Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-querydisplayconfig)）、`SetDisplayConfig` は取得済み topology / mode を validate・apply できる（[Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setdisplayconfig)）。`QDC_ONLY_ACTIVE_PATHS` は代替 refresh mode の列挙 API ではないため、初期実験では mode を生成せず、利用者が Settings で選んだ各状態を `QDC_ONLY_ACTIVE_PATHS | QDC_VIRTUAL_MODE_AWARE | QDC_VIRTUAL_REFRESH_RATE_AWARE` で full path / mode 配列として同一 session 中に取得する。2状態を identity で正規化して semantic diff し、target 集合・topology・path priority、source position / size / pixel format、rotation、scaling、output technology が同一で、差分が選んだ refresh に伴う target signal timing / vSync と対応 field だけの場合に限る。full target mode がある場合は path の `refreshRate` ではなく `targetVideoSignalInfo.vSyncFreq` が使われるため（[DISPLAYCONFIG_PATH_TARGET_INFO](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-displayconfig_path_target_info)）、個別 field を編集せず、allowlist 済み配列全体を扱う。検証・適用は `SDC_VALIDATE` または `SDC_APPLY` に `SDC_USE_SUPPLIED_DISPLAY_CONFIG | SDC_VIRTUAL_MODE_AWARE | SDC_VIRTUAL_REFRESH_RATE_AWARE` を組み合わせる。`DISPLAYCONFIG_PATH_BOOST_REFRESH_RATE` は保存・比較し、1 path でも設定されていれば DRR として拒否する（[DISPLAYCONFIG_PATH_INFO](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ns-wingdi-displayconfig_path_info)）。`SDC_SAVE_TO_DATABASE`、`SDC_ALLOW_CHANGES`、別系統の `ChangeDisplaySettingsEx`、生成 mode、driver private API、vendor library は使わない。

5. **元に戻せるか**  
   API 上は戻せるが、**製品として確実に戻せることはまだ未証明**。trial 直前の full path / mode、adapter・target identity、topology、refresh と boost flag を耐久保存し、切替先も同じ session・target 集合・topology で取得済みの場合だけ、まず validate、次に15秒の一時 trial を行う。適用後は `QueryDisplayConfig` で active path / mode を新しく取得し、要求配列と実効配列を分けて記録する。timeout 時も、現在が自分の適用構成と一致し、同じ target 集合・topology・session identity である場合だけ、直前取得した full 配列を `SDC_APPLY | SDC_USE_SUPPLIED_DISPLAY_CONFIG` で一時復元する。rollback 直前には `QDC_DATABASE_CURRENT` も再取得し、trial 前から永続構成だけが第三値へ変わっていた場合でも、その database を上書きせず、直前 active 配列の一時復元だけにする。active 側が第三構成、hotplug、GPU reset、driver fallback なら古い配列を強制しない。app crash、shell restart、画面が真っ黒でも timer が動く標準ユーザー権限の watchdog を実機で証明できなければ出荷しない。再起動後は session-bound の LUID / target ID を含む保存配列を replay せず、再列挙して OS database current への復帰を確認し、証明不能なら `unknown` と手動案内にする。dock / remote session / HDR transition / monitor identity 不一致では自動適用しない。

6. **実装の難しさと、いちばん危ないところ**  
   **大。** 最大の危険は黒画面、誤った monitor path、複数画面の topology 崩壊で、利用者が確認ボタンを押せなくなること。DRR、異なる GPU、USB display、KVM、sleep / resume、remote desktop を含む hardware matrix が必要。現行の通常 unit test や private store round-trip では何も証明できない。

7. **この製品の芯と噛み合う理由**  
   需要と「試して戻す」思想は強く合うが、rollback の実効保証が製品の芯そのものなので、そこを証明できるまでは候補表示にも昇格させない。完成すれば「高Hzで速くなる」ではなく、「自分で選んだ表示と電池の使い分けを忘れず戻す」と表現する。

## 今回は提案しないもの

需要があることと、この製品が安全に提供できることは別である。次は需要を確認できたが、現在の公開契約・復元条件・競合状況から採用案にしない。

| 項目 | 確認した需要・既存手段 | 提案しない理由 |
|---|---|---|
| アプリ別音量の自動固定・音量シーン | アプリ再起動で100%へ戻る報告（[r/Windows11](https://www.reddit.com/r/Windows11/comments/tq4c03/volume_of_apps_not_saving/)）、EarTrumpet への default / maximum volume 要望（[Discussion #680](https://github.com/File-New-Project/EarTrumpet/discussions/680)）。[EarTrumpet](https://github.com/File-New-Project/EarTrumpet) は per-app mixer・出力先・hotkey を提供 | [IAudioSessionManager2](https://learn.microsoft.com/en-us/windows/win32/api/audiopolicy/nn-audiopolicy-iaudiosessionmanager2) は device 上の session 列挙を公開するが、それだけでは第三者 session の setter 契約にならず、Microsoft の [Volume Controls](https://learn.microsoft.com/en-us/windows/win32/coreaudio/volume-controls) は通常アプリが unrelated application の session volume を変更できないと明記する。動作例の存在から documented public contract を推定せず、process / session identity と session 消滅・再生成後の exact rollback も未証明なので保留とする |
| アプリごとの既定音声出力切替 | [EarTrumpet](https://github.com/File-New-Project/EarTrumpet)、DisplayMagician などが提供 | 少なくとも EarTrumpet の source は SDK 文書への参照ではなく、独自の `IPolicyConfigWin7` / `SetDefaultEndpoint` COM 宣言を含む（[公式 repository](https://github.com/File-New-Project/EarTrumpet/blob/master/EarTrumpet/Interop/MMDeviceAPI/IPolicyConfig.cs)）。Microsoft の公開 Core Audio 文書に汎用 default endpoint setter を確認できないため、現在どおり読み取り＋Windows 設定案内にする |
| ゲームごとの HDR 自動 ON/OFF | 「game の時だけ HDR」という反復要望（[r/Windows11](https://www.reddit.com/r/Windows11/comments/zitmzf/turn_hdr_on_only_for_games/)、[AutoActions 利用例](https://www.reddit.com/r/Windows11/comments/1jr673k/)） | Advanced Color の enum には set 用 ID がある一方、`DisplayConfigSetDeviceInfo` の公式 remarks と対応 packet / build 契約が一貫して読めない（[device info type](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ne-wingdi-displayconfig_device_info_type)、[setter](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-displayconfigsetdeviceinfo)）。不明なので「できる」としない |
| 旧 Start / 旧 taskbar / 旧 context menu の復活 | ExplorerPatcher、StartAllBack、Windhawk が広く提供 | Explorer injection、hook、pattern、symbol、非公開 shell behavior と更新追随が中心。[ExplorerPatcher #4987](https://github.com/valinet/ExplorerPatcher/issues/4987) のように Explorer が継続 crash する事故半径が大きく、BRIEF の安全境界に合わない |
| 総合 display / game launcher profile | DisplayMagician が display・HDR・audio・program 起動・終了後復元まで提供 | 競合が既に強く、任意 program / CLI、vendor 固有処理、非公開領域まで広げないと差別化しにくい。採るなら上記9の公開 display API に限定した trial だけ |
| touchpad 全設定の profile 化 | Windows 11 24H2 以降には `SPI_GET/SETTOUCHPADPARAMETERS` が公開された（[TOUCHPAD_PARAMETERS_V1](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-touchpad_parameters_v1)） | 公開化は興味深いが、今回確認できた需要は native Settings の場所・driver 不具合が中心で、用途別 profile の反復需要は十分に示せなかった。まず read-only 対応状況を観測し、需要を追加調査する |

## まず作るならこの3つ

1. **ゲーム中の Shift 割り込みガード**  
   需要が反復し、Windows の手動設定が深いという問題が明確である。公開 GET/SET、setter が制御できる対象設定の backup、ゲームプロファイル終了時の復元が成立し、変更範囲も小さい。アクセシビリティを守る fail-closed 条件を先に試験できる。

2. **見失ったウィンドウを今の画面へ戻す**  
   PowerToys でも長期未充足で、Windows Settings まで画面外へ行く具体的な困りごとがある。現行 `setup.window_layout` の厳密な window identity と外部観測試験を再利用でき、保存済み layout がない人にもすぐ価値が出る。

3. **Windows 更新後のカスタマイズ健全性レポート**  
   最初は read-only で安全に出せ、PCカスタム独自の「元値・適用値・第三値・不明」をそのまま利用者価値へ変えられる。更新に追随して shell を patch する競合とは逆に、測れないものを測れないと示し、必要な項目だけ既存の検証済み Action で再適用できる。

この順なら、1と2で小さく実効状態の外部観測を積み上げ、3で全 Action の detector 品質を利用者向けに可視化できる。電源モードは新しい公開 API を使える有力な4番手、表示更新頻度は困りごとの反復があっても rollback の実機保証ができるまで実験枠に留める。
