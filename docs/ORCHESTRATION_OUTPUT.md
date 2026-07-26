# PCカスタム Task 1 オーケストレーション出力

## 1. リポジトリ調査結果

調査開始時のrepositoryには`BRIEF.md`だけがあり、実装code、project manifest、test、Tauri/Electron scaffold、既存docsは無かった。`BRIEF.md`を唯一の正典として、今回新規に設計文書を作成した。Task 1では`.rs`、`.ts`、`.tsx`等の実装fileを作成していない。

設計を拘束する最重要条件は次のとおりである。

- UI/coreを常時管理者で起動せず、昇格は必要なActionの短命helperに限定する。
- OS操作はcompile-time登録済みAction IDと型付きparameterだけから到達可能にする。
- registryはkey/valueの存在、型、raw元値、適用値を保存し、「既定値」ではなく直前状態へ戻す。
- profile、共有、AI、pluginから任意code、script、binary、shell、registry pathを実行できない。
- Windows build差異を一箇所で管理し、Microsoft support中とPCカスタム実機試験済みを区別する。
- 不明なAPIや効果を断定せず、read-only/guided/`未検証・要CC確認`へ落とす。

## 2. 推奨技術構成

**候補A: Tauri 2 + React + TypeScript + Rust + windows-rs + SQLiteを推奨する。CCの暫定推奨に賛成する。**

0〜5点を付け、`重み × 評点 ÷ 5`で加重した。BRIEFが示すのは優先順で数値そのものではないため、本設計でその順を`30 / 25 / 20 / 15 / 10`へ数値化した。memoryはPCカスタム実測前の構造評価である。

| 要件 | 重み | A 評点 / 加重点 | B: Electron + helper 評点 / 加重点 | 判断 |
| --- | ---: | ---: | ---: | --- |
| 安全な権限分離 | 30 | 4.5 / 27.0 | 4.0 / 24.0 | AはRust core/helperとTauri capabilityで境界が短い。Bも別helperなら成立するがrenderer/main/preloadを追加監査 |
| 低い常駐memory | 25 | 4.0 / 20.0 | 2.5 / 12.5 | AはUIを閉じWebViewを破棄したnative core常駐が可能。B標準形はChromium/Node複数process。数値未計測 |
| Windows API統合 | 20 | 5.0 / 20.0 | 5.0 / 20.0 | BもRust helperで同等APIへ届く。Aはnative層までの配線が短い |
| 署名・installer・更新 | 15 | 4.5 / 13.5 | 4.0 / 12.0 | 両者可能。Aは公式MSI/NSISと署名必須updater導線がまとまる |
| Update・復旧・拡張・開発 | 10 | 4.0 / 8.0 | 4.0 / 8.0 | Aは型と単一native層、BはTS開発性。各々WebView2/Electron更新負担あり |
| **合計** | **100** | **88.5** | **76.5** | **Aを採用推奨** |

CC記載のElectron常駐100〜200MB級は本アプリでは未計測なので得点の実測根拠にしていない。Bをnative watcherのみ常駐としてmemory評点4.0にすると84.0点、権限分離も4.5にすると87.0点でAとの差は小さい。Task 2で同じworkloadのprivate working set、CPU、wakeups、起動時間を測り、Aが予算を満たさない場合は再採点する。

構造根拠は、Windows 11の共有Evergreen WebView2 Runtime、Electronのmulti-process model、Tauri capability/updater、windows-rsのMicrosoft API projectionである。詳細と一次資料は[ARCHITECTURE.md](./ARCHITECTURE.md)に記載した。

## 3. 競合との差別化整理

PCカスタムの差別化は機能数ではなく、次の8点を同時に守ることにある。

1. Windows知識ゼロでも使える。
2. 設定名でなく、得たい結果から選ぶ。
3. ゲーム・勉強・作業・普段使いのmodeを作れる。
4. game起動等の条件で安全に自動適用する。
5. 変更を1件ずつ記録し、個別または時点まで正確に戻す。
6. 危険設定をstable catalogから分離する。
7. 共有profileに任意code実行能力を持たせない。
8. AIは登録済みActionとparameterだけを提案する。

| 参考競合 | 学ぶ点 | PCカスタムが別に担う点 |
| --- | --- | --- |
| PowerToys | Microsoft公式の安全な便利機能 | mode、条件適用、Action単位journal/rollback。重複機能は再実装より導入・公式設定導線を優先 |
| WinUtil | 多くの設定をまとめる体験 | 初心者向け結果表示、変更前backup、build gate、危険機能の隔離 |
| Winaero Tweaker | 幅広いcatalog | 量よりMVP Actionの検証、用途profile、復元証跡 |
| Sophia Script | 変更と復元の考え方 | PowerShell中心でなく型付きfirst-party Action、任意script禁止 |
| Windhawk | 深いcustomization | injectionをせず、公開API/文書化状態を優先 |
| StartAllBack | 外観体験 | Shell置換でなく限定ActionとUpdate後fail-closed |
| Razer Cortex | game開始時適用・終了時復元 | 効果を誇張せず、lease/transaction/第三状態検出で正確に戻す |
| Playnite | library、theme、extension UX | game library自体よりWindows mode自動化。dynamic code extensionは受け入れない |

## 4. 初期版機能の優先順位

| 優先 | 内容 | 出荷判断 |
| --- | --- | --- |
| P0 | Action registry、compatibility、lossless backup、transaction、timeline、recovery、権限境界 | 1つでも欠ければmutable Actionを出荷しない |
| P1 | 14件の初期Action catalog | Actionごとにmutable/read-only/guidedを分け、実機gate合格分だけ自動化 |
| P2 | game executable登録、process watch、resource lease、終了時復元、crash recovery | P0 transactionが故障注入に合格後 |
| P3 | 署名配布、updater、build/hardware matrix | automation再開までupdate後self-check必須 |
| P4 | data-only共有、AI候補提示 | stable Action schema固定後 |
| P5 | experimental機能 | stable binary/DB/allowlistから隔離し、既定無効 |

`power.user_mode`のユーザーが明示する一回変更は公開APIのstable候補だが、game起動連動の自動電源変更は副作用・競合評価が済むまで後回しにする。MicrosoftがOSをsupport中でもPCカスタムのtest evidenceが無ければ自動適用しない。

## 5. 初期実装 Action 10〜15

初期catalogは14件である。これは14件すべてを初日から自動変更可能にする意味ではない。

| ID / 結果 | 変更手段 | 保存する復元データ | rollback手順 | 危険度 | admin要否 | 再起動要否 | Windows Update影響 | 状態検出可否 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `explorer.show_extensions` / 拡張子を常に表示 | HKCU `HideFileExt`限定registry。書込契約はbuild実機gate | key/value有無、type、raw元値、適用値、view | applied一致時だけ元値、元々無ければvalue削除。限定通知 | 安全・自動は検証後 | 不要 | OS不要、Explorer反映要試験 | 中 | registry可、見た目要試験 |
| `explorer.show_hidden` / 隠しfileを表示 | HKCU `Hidden`限定。保護OS fileは対象外 | 同上 | 同上 | 注意 | 不要 | OS不要、Explorer反映要試験 | 中 | registry可、見た目要試験 |
| `taskbar.widgets_visibility` / Widgets表示 | Microsoft状態資料のtaskbar registry候補。write fixture合格後のみ | 完全registry snapshot | 第三状態でないことを確認後、元状態へ。強制restartなし | 安全・書込未検証 | 不要 | OS不要、Explorer要試験 | 中〜高 | 候補値可、表示要試験 |
| `taskbar.clock_seconds` / 時計に秒を表示 | 初期は公式Settings手動案内。自動registry変更は無効 | 自動化時のみ完全snapshot | guidedはno-op。将来は元の欠如まで復元 | 注意 | 不要 | 未検証 | 高 | 未検証・要CC確認 |
| `theme.color_mode` / dark/light | Microsoft状態資料の2 HKCU値。contrast/policyをpreflightしwrite gate | 両値のkey/value有無、type、raw元値、適用値 | transaction逆順復元、第三状態は止める | 注意・検証まで自動無効 | 不要 | OS不要、一部app再起動 | 中 | configured可、effectiveは部分的 |
| `theme.transparency` / 透明効果 | Microsoft状態資料のHKCU値。write契約/contrast gate | 完全registry snapshot | 元値または元の欠如へ復元 | 注意・検証まで自動無効 | 不要 | OS不要 | 中 | configured可、effective要試験 |
| `gaming.game_mode` / Game Mode切替 | 状態資料のHKCU値。third-party write契約未確認 | 完全registry snapshot | 元状態へ復元し再検出 | 注意・自動無効 | 不要 | 反映条件未検証 | 中〜高 | 設定値可、実効状態と分離 |
| `session.prevent_sleep` / mode中sleep防止 | 公開`SetThreadExecutionState`、画面点灯は既定OFF | owner、flag、開始、API結果、thread | 最後のleaseで要求解除。process終了時OS解放 | 安全 | 不要 | 不要 | 低 | PCカスタム lease可 |
| `apps.launch_set` / 必要appをまとめて起動 | shellなし。端末上のopaque app ID、known host/launcher拒否。MVPは引数なし、共有/AIからpath指定不可 | 既存process、作成PID/時刻/file identity、local登録ID、終了方針 | 既定は追跡解除。明示時のみ同一性確認後に通常終了要求、強制終了なし | 注意 | 不要 | 不要 | 低 | 可、access deniedは不明 |
| `games.process_watch` / game開始終了を検知 | Toolhelp/WMI、full path、creation time、handle wait。注入なし | file identity、canonical path、instance key、世代 | observer/lease解除。OS変更なし | 安全 | 不要 | 不要 | 低 | 可、access deniedは不明 |
| `startup.inventory` / 自動起動app確認 | baselineはHKCU Run/RunOnceとuser Startup folder。WMI/HKLM等はprivilege差によりbest-effort | source、取得可否、項目、時刻のみ | no-op、MVPは削除/無効化なし | 安全 | baseline不要 | 不要 | 中 | 部分的。source別unknown、完全性を主張しない |
| `power.user_mode` / 希望電源mode切替 | 公開Windows 11 Power API、AC/DC別 | AC/DC元GUID、適用GUID、effective観測 | configuredがapplied時だけ元GUIDへ。外部変更は停止 | 注意 | 原則不要・実機確認 | 不要 | 低〜中 | configured/effective別に可 |
| `power.active_scheme_check` / 電源設定確認 | 公開Power API、必要時read-only公式CLI | 観測値と時刻 | no-op | 安全 | 不要 | 不要 | 低 | 可、OEM modeは不明あり |
| `games.readiness_check` / 準備漏れ確認 | 上記detectとdisplay/audio公開read APIを合成 | probe値、時刻、根拠、unknown理由 | no-op | 安全 | 不要 | 不要 | 中 | probeごとに可/部分/不明 |

## 6. 危険 / ニッチとして後回し

### 永久禁止

- Defender/Firewall無効化、Windows Update完全停止
- pagefile無効化、HPET/BCD変更、根拠の薄いFPS registry、大量service停止
- game/anti-cheatへのinjection、process memory/game file改変
- 任意PowerShell/cmd/bat/DLL/EXE/JS/reg、任意shell、任意URL download
- 出所不明script/binaryの取得・実行

これらはexperimental hostにも入れない。

### 実験的候補

- DND/Focus Assist自動toggle、display Hz/HDR、default audio変更
- game連動の電源自動変更、Explorer強制再起動
- WinGet/App Installer自動導入、startup変更、Task Scheduler登録
- ETW watcher、PowerToys等外部toolの自動設定

実験候補は別署名binary、別namespace/DB/allowlist/compatibility manifest、既定無効、非常駐、exact-build gate、単独transaction、専用kill switchを要求する。AI、共有profile、自動game profileから呼べず、失敗時は機能単位でquarantineする。stable helperやstable backup decoderを共有しない。公開契約、実機matrix、lossless rollback、security reviewが揃った場合だけ通常Actionへの昇格を再設計する。

## 7. アーキテクチャ案

```text
標準権限
React UI on WebView2（必要時だけ生成）
        │ 型付きcommand
Rust core / asInvoker
  ├─ Action registry / transaction coordinator
  ├─ compatibility service / self-check
  ├─ SQLite journal / timeline / recovery
  ├─ game watcher / resource locks & leases
  └─ UAC + mutual-auth one-shot IPC ── 短命な署名済みelevated helper

別系統・既定無効・非常駐: experimental host
```

UIを閉じた常駐時はWebViewを破棄し、native coreだけを残す。helperは保護されたmachine-scope pathに置き、許可済みAction ID/version、型付きparameter、transaction IDだけを受ける。helper自身がprivileged backupを保護storeへ保存し、coreはopaque IDだけを保持する。per-user editionを用意する場合はhelper/admin Actionなしとする。IPC主案はlocal-onlyの一回限りnamed pipeで、`ncalrpc`はTask 2比較spikeである。

将来のpluginはdata-only profile/theme/metadataを基本とする。新しいOS操作codeは任意導入pluginではなく、first-party review・署名・compatibility test済みmoduleとしてapp releaseに含める。

## 8. Action / rollback 設計案

Actionは最低限、次のfieldを持つ。

`id, name, description, category, tags, supportedWindowsVersions, minimumBuild, maximumTestedBuild, riskLevel, requiresAdmin, requiresRestart, requiresExplorerRestart, conflicts, dependencies`

必須処理は`detectCurrentState(), validate(), createBackup(), apply(), verifyApplied(), rollback(), verifyRolledBack(), explainChanges(), troubleshooting()`である。実装版/schema/method/resource key/evidenceも追加metadataとして持つ。

一括適用は次の順に固定する。

`全Action事前検証 → resource lock → 全backupのdurable commit → 順次apply → 各verify → 成功commit`

途中失敗は実際に適用した順の逆順でrollback/verifyする。rollback失敗は元のerrorで隠さず、timelineへ別itemとして残す。registry backupはkey有無、value有無、type、length、raw元bytes、適用bytes、viewを保持する。元keyが既存で元valueが無ければ対象valueだけを削除して欠如状態へ戻し、元key自体が無い新規mutationはbackup前にfail-closedとする。

rollback前に現在状態を`original / applied / third / unknown`へ分類する。`applied`だけが自動復元候補で、`third`はユーザー/他appの変更を黙って上書きしない。さらに未知buildでは、Action/versionのcross-build rollback承認とruntime probeが無ければ`applied`でも自動writeせず`RECOVERY_REQUIRED`にする。timelineは「この変更だけ戻す / この時点まで / 内容 / 結果 / 失敗だけ再試行 / log出力」を持つ。旧Action backup decoderは保持期間中削除しない。

## 9. ゲームプロファイル状態遷移

基本状態はBRIEFどおりである。

`登録 → 待機 → 起動検知 → 適用中 → プレイ中 → 終了検知 → 復元中 → 待機`

startup snapshot、WMI event、canonical full path、file identity、PID+creation time、process handle wait、低頻度補正を組み合わせる。同一event/instanceはidempotency keyで重複排除する。launcher/childは自動推測せず、exact executableまたは明示process groupを使う。

複数profileが同じresourceへ同じdesired stateを要求すればleaseを共有し、最後のowner終了時だけ最初のbackupへ戻す。反対desired stateなら後発を競合停止し、先行を上書きしない。gameだけがcrashした場合はprocess終了として即時に通常復元する。PCカスタム/OS停止で未復元になった場合は、次回core起動時に新規監視よりjournalを先にreconcile/rollbackする。ゲームがまだ動いていても旧sessionを暗黙再開せず、一度安全に戻した後、許可時だけ新sessionで再適用する。

## 10. セキュリティ上のリスク

| リスク | 主対策 |
| --- | --- |
| UI/XSSからOS操作 | UIに直接OS capabilityを与えず、coreがAction ID/schema/build/riskを再検証 |
| IPCなりすまし/replay | one-shot endpoint、explicit DACL/MIL、remote拒否、PID/path/signature/token、nonce、transaction ID、期限、size上限 |
| confused deputy | helperはregistry path/command/raw backupを受けず、allowlisted admin Actionを再解決。privileged backupはhelper所有 |
| path traversal/reparse/TOCTOU | canonical local path、handle/file identity、protected install、reparse拒否、open後再確認 |
| command/code injection | shell文字列なし。Action別固定schema。script host/LOLBins、file association、自由argvを禁止 |
| profile/AI迂回 | data-only schema、known Actionだけ、AIは候補parameterのみ、apply前preview/確認 |
| supply chain/update | 全artifact署名、publisher固定、manifest replay/downgrade防止、staged rollout、SBOM |
| log/privacy | raw command line、token、個人path等を既定収集せず、export時preview/redaction |
| rollback改ざん | standard/privileged journalを分離、integrity check、opaque ID、第三状態を自動上書きしない |

named pipeのexact SDDL/MIL、over-the-shoulder別admin、endpoint squatting、PID reuseは未検証であり、Task 2の攻撃test gateとする。

## 11. 実装フェーズと順番

| Task | 内容 | 完了gate |
| --- | --- | --- |
| 1 | 今回の設計10文書 | 要求網羅、未検証明示、code/scaffoldなし |
| 2 | 候補A基盤、OsIdentity、Action registry、SQLite journal、recovery、IPC spike、A/B計測 | read-only + session Actionの実OS縦切り、kill-point復旧、unknown build fail-closed |
| 3 | game watcher、durable state machine、lease、profile UX | multiple game/instance/crash/sleepで多重適用せず復元 |
| 4 | 14 ActionをWave別追加 | 各Actionの公開契約、実機detect/apply/rollback evidence。未合格はguided/read-only |
| 5 | 24/25/26 matrix、署名installer/updater | tamper/update/downgrade/OS update後もbackupとautomation gate維持 |
| 6 | data-only共有、AI候補 | adversarial import/promptで任意実行不能、preview必須 |
| 7 | isolated experimental評価 | stable障害分離、exact-build試験、昇格criteria全合格 |

各Taskは安全基盤への依存を満たしてから開始する。Action数やUI完成だけで終了せず、failure injection、互換性、security、memoryのevidenceをrelease gateにする。

## 12. 次の Task 2 で基盤実装へ

Task 2開始前にCCが、A採用の再評価条件、machine/per-user install範囲、privileged journal、unknown-build rollback policy、named pipe threat modelを確認する。承認後の最初の縦切りは次の順である。

1. ADRと同一workloadのA/B常駐計測fixture
2. 候補Aの最小build/test基盤
3. compile-time Action registryとcompatibility service
4. SQLite transaction journal、timeline、recovery
5. read-only `power.active_scheme_check`
6. session-only `session.prevent_sleep`のapply/lease/release
7. rollbackのkill-point/failure injection test
8. standard/elevated IPCのisolated attack spike

完了判定は「実OS上で検出→backup→適用→検証→復元→再検証が通り、途中kill後も回復し、未知buildでは変更しない」ことである。**今回のTask 1ではこれらのscaffoldや実装codeを作成していない。**
