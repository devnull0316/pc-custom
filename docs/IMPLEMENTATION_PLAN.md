# 実装計画

## 1. 現在地

**Task 1は設計フェーズのみであり、本成果物では実装コード、Tauri/Electron scaffold、manifest、installer、test binaryを作成しない。** Task 1の完了条件は、9設計文書と`ORCHESTRATION_OUTPUT.md`がBRIEFの制約、未検証事項、判断gateを具体化し、CCがTask 2の境界を監査できることである。

実装順序は「画面を増やす」順ではなく、壊さず戻せることを先に証明する順にする。

`Task 2 基盤 → Task 3 ゲームプロファイル → Task 4 stable mutable Action → Task 5 互換性・配布 → Task 6 共有/AI → Task 7 実験的機能評価`

## 2. 全フェーズ共通gate

各Taskは次を満たすまで次へ進めない。

1. BRIEFと設計文書に対する差分理由がADRへ残る。
2. 新しいOS変更にはdetect、validate、backup、apply、verify、rollback、rollback verify、説明、troubleshootingが揃う。
3. failure injectionで途中失敗・process kill・disk error・外部変更を再現し、journalから回復できる。
4. 対応build/architecture/権限のtest evidenceがcompatibility catalogにある。
5. `安全`と表示するActionでも変更方法、制約、未検証範囲をUIで説明できる。
6. 任意code、shell、script、自由形式registry/path/URLをprofile・AI・IPCから渡せない。
7. security review、rollback review、Windows compatibility reviewの未解決high issueが0件である。
8. memory、CPU、wakeups、起動時間、DB durabilityの計測値を前回baselineと比較する。

機能数を完了指標にしない。各縦切りは1つのActionで全ライフサイクルを通し、mockだけで「実装済み」にしない。

## 3. Task 1 — 設計（今回）

### 成果

- product/MVP境界、A/B採点、A推奨
- process、data、権限、IPC、更新、experimental分離のarchitecture
- Action型、transaction、正確なrollback、変更timeline
- ゲームprofile状態機械、resource lease、crash recovery
- Windows 11一次情報調査とcompatibility gate
- Task 2以降の実装順序とCC判断点

### 完了判定

- 必須10文書が日本語で存在する。
- 初期Actionが10〜15件で、要求された8評価項目を持つ。
- 断定できないWindows操作が`未検証・要CC確認`またはguided/read-onlyへdowngradeされる。
- `.rs`、`.ts`、`.tsx`、project scaffold等が作られていない。

## 4. Task 2 — 安全基盤

Task 2は「安全基盤＋縦切り」として実装へ移行した。CC承認と今回指示により、read-only/session sliceに加えてstable HKCU setterと主要UIを前倒しし、architectureを実コードで証明する。

今回の確定scopeは、Tauri 2/React/TypeScript/Rust基盤、Action registry、OsIdentity、SQLite journal/recovery/timeline、registry lossless backup、`session.prevent_sleep`、`power.active_scheme_check`、`explorer.show_extensions`、`explorer.show_hidden`、`theme.color_mode`、`games.process_watch`、ホーム/Action/preview/timeline/Ctrl+K UI、deny-all昇格IPC契約、A/B計測fixtureである。helper実体とadmin Actionは含めない。

### 2.0 CC判断とADR

最初に次を確定する。

- ADR-001: 候補Aを採用し、候補Bへ戻す定量条件
- ADR-002: standard core、UI lifetime、single-instance、background lifecycle
- ADR-003: journal durability、schema migration、backup retention
- ADR-004: compatibility catalogとunknown-build fail-closed
- ADR-005: elevated helperのinstall scope、protected backup store、IPC threat model
- ADR-006: dynamic pluginを禁止し、data-only profileとfirst-party moduleを分ける方針

A/B最終gateでは、同じwatcher workload、同じUI idle/closed条件、同じWindows機でworking set/private bytes/CPU/wakeups/start timeを計測する。`scripts/measure-private-working-set.ps1`と`scripts/MEMORY_AB.md`に従い各状態10 run以上を記録し、Aが要件を満たさない、またはB native watcher構成が明確な総合優位を示す場合だけ再採点する。

### 2.1 repository/build基盤

- Tauri 2 + React + TypeScript UI、Rust core、windows-rs、SQLiteの最小構成
- Rust/TypeScript format、lint、unit/integration test、dependency audit、SBOMのCI
- x64/Arm64 build方針、reproducibleなversion付与、debug/release設定分離
- secretをrepositoryへ置かないsigning/update環境境界

この段階のUIは状態表示と診断に必要な最小限に留める。

### 2.2 compatibility service

- `OsIdentity`取得: base build、UBR、SKU、architecture、feature probes
- build差異を一箇所で管理するcompile-time catalog
- `tested_mutable` / `detect_only` / `guided` / `unknown` / `blocked`判定
- OS fingerprint変化時の全Action self-checkとkill switch
- 24H2/25H2 VM、26H1実機調達/検証計画、Arm64行列

最初のActionは`power.active_scheme_check`または`startup.inventory`のread-only sliceとし、状態不明をfalseへ潰さないことを証明する。

### 2.3 Action registryとjournal

- compile-time Action registry、stable Action ID、schema/version、method ID
- fieldと全必須処理の型境界
- parameter schema、conflict/dependency、resourceKey解決
- SQLite transaction journal、WAL/durability方針、backup envelope、migration
- original/applied/third/unknown reconciliation
- timeline query、1件rollback、指定時点rollback、失敗item再試行

最初のmutable sliceは`session.prevent_sleep`とする。これが合格した後、CC監査P3でstableと確定した`explorer.show_extensions`、`explorer.show_hidden`、`theme.color_mode`を同じTask 2内で追加し、lossless registry backup、非破壊broadcast、正確な欠如状態復元まで通す。

### 2.4 standard/elevated境界spike

既定editionはper-user・helperなし・admin Actionなし・`asInvoker`で製品縦切りを完成させる。Task 2では将来のadmin Action用にprotocol型、deny-all allowlist、strict decoder、peer evidence validator、攻撃spike試験だけを実装し、次Taskのhelper実体が満たす条件を次のとおり固定する。

- machine-scope protected installとhelper署名/identity
- one-shot named pipeのDACL/MIL、local-only、endpoint squatting対策
- 同一adminとover-the-shoulder別adminのUAC経路
- PID/path/signature/token/creation time、nonce、期限、payload上限の相互検証
- helper-owned protected journalとcoreのopaque backup ID同期
- core/helper crash、helper commit後core ID保存前の孤児record、user DB消失、request replay、PID reuse、path replacement、DB tamper
- named pipeが境界を満たさない場合だけ`ncalrpc`比較

今回のcontract試験が合格してもhelperが存在することにはならない。helper実体は次Taskでmachine-scope opt-inとして実装・実機攻撃試験し、合格前はadmin Actionを出荷catalogへ登録しない。helperへraw registry path、command、local backup valueを送らない。

### 2.5 Task 2 test

- in-memory fakeではなく、一時的な専用test resourceでbackup/apply/rollbackを検証
- DB commit直前/直後、OS apply直前/直後、verify中、rollback中のprocess kill
- disk full、DB locked/corrupt、access denied、API timeout、external third state
- unknown build、unsupported edition、policy-managed、feature missing
- property-based parameter検証、64KiB上限・unknown/duplicate field・nonce/replay/counter/deadline・peer identity不一致を含むIPC attack spike、malformed profile拒否
- resident coreを24時間動作させ、handle/memory/wakeup leakを計測

### Task 2完了条件

1. UI→standard core→Action→journal→timeline→rollbackの縦切りが実OSで通る。
2. 再起動後に未完了transactionを検出して安全にreconcileできる。
3. unknown buildでmutationがfail closedする。
4. UIを閉じてWebViewを破棄した常駐値が予算内で、A/B判断が計測で確定する。
5. deny-all IPC契約と攻撃spike結果が文書化される。helper実体は次Taskへ引き継ぎ、admin ActionなしでTask 3へ進められる。

## 5. Task 3 — ゲームプロファイル

### 3.1 process watcher

- startup snapshot、WMI start/stop、full path/creation time検証
- process handle wait、低頻度correction snapshot、observer reconnect
- exact executable registrationとrebind
- multiple instance、access denied、PID reuse、sleep/resume

### 3.2 durable state machine

- `登録 → 待機 → 起動検知 → 適用中 → プレイ中 → 終了検知 → 復元中 → 待機`
- session journal、resource lock/lease、idempotency key
- 同desired state共有、反対desired state競合停止、最後のownerだけrollback
- game crashはprocess終了として即時復元。PCカスタム/OS停止で未復元なら次回coreをrecovery-firstで起動

### 3.3 profile UX

- 実行ファイル本人性、Action一覧、危険度、変更前/後、競合のpreview
- 状態遷移と未復元を隠さない実行中表示
- exact executableと明示process group。launcher自動推測はしない
- 安全に停止、automation一時停止、rebind、復旧画面

### Task 3完了条件

- 2ゲーム×複数instance×共有/競合Actionの全終了順で元状態が一致する。
- WMI event欠落、core kill、game crash、sleep/resumeから多重適用せず復元する。
- protected/access-denied processで誤適用しない。
- gameへinjectionせず、debug privilegeを使わない。

## 6. Task 4 — Stable MVP Action catalog

Task 2/3の基盤へ、`MVP_SCOPE.md`の残りのActionをriskと公開契約の順に追加する。`session.prevent_sleep`、`games.process_watch`、`power.active_scheme_check`、`explorer.show_extensions`、`explorer.show_hidden`、`theme.color_mode`はTask 2へ前倒し済みとして扱う。14件すべてを自動変更可能にすることは完了条件ではない。

### Wave 1: 公開API・read-only中心

- `startup.inventory`
- `games.readiness_check`
- `apps.launch_set`（端末上のopaque app ID、MVPは引数なし、known host/launcher拒否、共有/AIからpath指定不可、既定では終了させない）

### Wave 2: 公開setter

- `power.user_mode`（明示的一回変更のみ。AC/DC別）

### Wave 3: registry状態資料を使う条件付きAction

- `taskbar.widgets_visibility`
- `taskbar.clock_seconds`
- `theme.transparency`
- `gaming.game_mode`

clock secondsとtransparencyはCC監査P3でstable HKCU setterへ格上げ済みだが、対象buildの実機smokeと非破壊broadcastを出荷条件にする。WidgetsとGame Modeは格上げ対象外であり、build別のwrite契約が確定しない限りguided/detect-onlyを維持する。

### Task 4完了条件

- 各Actionがカード必須情報とtroubleshootingを持つ。
- apply successをAPI returnだけでなく再detectで検証する。
- 値の欠如、型違い、policy、外部変更をlosslessに扱う。
- profile/AIがAction内部のpath/methodを変更できない。

## 7. Task 5 — 互換性、署名、installer、updater

### 5.1 Windows matrix

- 25H2を主、24H2を移行、26H1を独立hardware行として実施
- x64/Arm64、Home/Pro、standard/UAC、代表display/audio/power構成
- monthly update smoke、feature update rehearsal、known issue quarantine
- Action version/backup decoderの前方・後方互換性

### 5.2 installerと署名

- per-machine protected installを基本とし、標準userのみの機能セットとのtrade-offをADR確定
- core、UI、helper、experimental host、installer、update artifactのpublisher chain
- downgrade、repair、uninstall時のactive transactionとbackup retention
- uninstall前に未復元変更をpreview/rollbackし、ユーザーが保持を選んだ履歴だけexport

昇格helperを通常userが置換可能なper-user directoryへ置かない。per-user配布を用意する場合はadmin Action/helperを含めず、機能差を明示する。

### 5.3 updater

- signed artifact、TLS、version/rollback protection、staged rollout
- update前にactive profileを安全停止し、journal migrationをpreflight
- app crash loop/rollback、compatibility kill switch、旧backup decoder保持
- update後self-check合格までautomationを再開しない

### Task 5完了条件

- clean install/update/downgrade/repair/uninstallをstandard userとUAC拒否で検証する。
- artifact tamper、manifest replay、署名不一致を拒否する。
- OS updateとapp updateの順序を入れ替えても未復元を失わない。

## 8. Task 6 — 共有プロファイルとAI補助

安全基盤とAction catalogが固定されるまで開始しない。

### 共有

- data-only schema: 名前、説明、用途、Action ID/parameter、app launch reference、theme reference、対応version
- import前に互換性、危険度、実際の変更、欠落Action、競合をpreview
- PowerShell/cmd/bat/DLL/EXE/JS/reg、任意URL download、任意shell commandをschemaで表現不能にする
- signature/reputationは補助であり、未署名でもcodeを持てない構造を維持する

### AI

- 入力はユーザー目的と公開可能なAction metadataに限定
- 出力は登録済みAction IDとschema適合parameterのみ
- unknown ID、method/path/command、自由形式codeを拒否
- final previewとユーザー確認なしにapplyしない
- prompt/responseへ個人path、game account、process listを不要に送らない

### Task 6完了条件

- adversarial import/promptでも任意実行・path traversal・危険Action昇格が不可能。
- schema/version不一致をpartial executionせず明示停止する。
- offlineでもprofileの閲覧・rollbackができる。

## 9. Task 7 — Experimental evaluation

対象候補はDND自動切替、display Hz/HDR、default audio、Explorer強制再起動、WinGet bootstrap、startup変更、Task Scheduler、ETW、外部tool連携である。永続禁止項目はここにも入れない。

実験的機能はstable coreへの追加ではなく、別署名binary、別allowlist、別compatibility manifest/DB namespace、既定無効、非常駐、exact-build gate、単独transaction、AI/共有/自動profile除外で評価する。失敗時は機能単位でquarantineし、stable Actionとrollbackを巻き込まない。

stableへ昇格する条件は、公開/supported契約、代表実機matrix、lossless backup、crash recovery、Update追従、security review、ユーザー向け説明がすべて揃うことである。利用者数や要望だけでは昇格しない。

## 10. 優先度と依存関係

| 優先 | 作業 | 依存 | 先に行う理由 |
| --- | --- | --- | --- |
| P0 | OS identity、compatibility fail-closed | Task 1 | 未知buildで変更しない土台 |
| P0 | Action registry、journal、rollback | compatibility | すべての変更の安全保証 |
| P0 | recovery-first、timeline | journal | crash後に被害を残さない |
| P0 | privilege/IPC spike | registry/journal threat model | admin機能の境界を早期に否定/証明 |
| P1 | process watcher、profile state/lease | Action transaction | ゲーム自動化の中核 |
| P1 | read-only/session Action | compatibility/Action | 低riskで縦切りを完成 |
| P1 | persistent standard Action | rollback実証 | 正確な復元を実OSで証明 |
| P1 | build/hardware matrix | 各Action | 出荷modeを決めるevidence |
| P2 | signed distribution/update | schema安定 | migrationと復旧を含む配布保証 |
| P3 | data-only共有/AI | stable registry | 実行能力を増やさず利便性を追加 |
| P4 | experimental host | stable製品出荷後 | 危険・ニッチを隔離して評価 |

## 11. 計測予算とrelease判断

Task 2開始時に数値を固定し、機器specと一緒に記録する。現時点では本アプリの実測がないため、次は`未検証・要CC確認`のrelease gateである。

- UI closed時のprivate working set、CPU、wakeups、handle/thread数
- UI open/idle時のWebView込み値
- game start検知latencyとcorrection頻度
- transaction journal commit latencyとDB growth
- crash recovery時間、rollback成功率、第三状態誤上書き0件
- monthly update後のAction quarantine率

「Electronは100〜200MB」「Tauriなら軽い」という一般論をrelease判定に使わず、同一workloadのA/B fixtureで採否を確定する。

## 12. Task 2へのhandoff

Task 2の最初の縦切りは次の順とする。

1. CCがADR-001〜006、特にinstall scope、helper protected journal、unknown-build policyを承認する。
2. 候補Aの最小build/test基盤と、A/B常駐計測fixtureを作る。
3. `OsIdentity`、compile-time compatibility catalog、read-only Actionを通す。
4. SQLite journalと`session.prevent_sleep`でapply/lease/release/timelineを通す。
5. kill pointを入れたrecovery/rollback試験を通す。
6. helper IPCはisolated spikeとし、合格前にadmin Actionを追加しない。

以上が合格して初めて、Task 3のゲームプロファイル実装へ進む。Task 1ではこの実装に着手しない。
