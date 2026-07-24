# セキュリティ設計

## 1. 安全目標

1. UI/WebViewが侵害されても、任意のOS操作や昇格処理へ到達させない。
2. 標準権限core以外のlocal processが、昇格helperになりすましてActionを実行できないようにする。
3. profile、AI、ログ、更新経路を通じたcode/command注入を構造的に不可能にする。
4. path traversal、reparse、PID再利用、registry view違い等で意図しない対象を変更しない。
5. 変更前状態を失わず、失敗・競合・rollback失敗を隠さない。
6. 秘密情報・個人情報を収集またはログへ残さない。

Rust、Tauri、署名の採用だけで安全とはみなさない。coreはOS APIへアクセスでき、署名済みバイナリにも脆弱性はあり得るため、境界ごとの最小権限と入力検証で守る。

## 2. 保護対象と信頼境界

### 保護対象

- Windows設定と変更前backup
- elevated helperの権限
- Action allowlistとcompatibility判定
- profile/game binding、変更timeline
- updater署名鍵、Authenticode秘密鍵
- ユーザーのlocal path等の個人情報

### 境界

1. React WebView → Rust core IPC
2. standard core → elevated helper IPC/UAC
3. Action handler → Windows API/CLI/registry
4. untrusted profile/AI response → Action request
5. updater/network → installed binaries/manifest
6. SQLite/log/export → local filesystem

## 3. 脅威モデル

| 脅威 | 例 | 主対策 |
| --- | --- | --- |
| command/code injection | profile名、path、引数がshell/script hostへ到達 | shellを使わない、local app ID、app別固定schema、known host/launcher拒否、追加field拒否 |
| path traversal | `..`, relative/UNC/reparseで別fileへ到達 | file handleから最終path/file ID確認、固定root、local fixed drive限定 |
| IPC spoofing | 悪意あるlocal processがhelperへ接続 | random endpoint、明示DACL、remote拒否、相互PID/path/signature/token確認 |
| replay/二重適用 | 盗んだrequestを再送 | nonce、request UUID、transaction state、message counter、期限 |
| confused deputy | coreが任意registry pathをhelperへ依頼 | helper自身のAction ID→固定handler解決。自由形式targetなし |
| renderer compromise | XSSからnative API呼出し | local assetのみ、厳格CSP、Tauri capability縮小、coreで再検証 |
| profile supply chain | 共有profileに実行file/URLを埋込 | data-only schema、code fieldなし、preview、size/depth制限 |
| update tampering | 偽update/helper差替え | HTTPS、Tauri artifact署名、Authenticode、protected install ACL、version整合 |
| rollback破壊 | user/GPO変更を元値で上書き | current==applied fingerprintの比較、third stateで停止 |
| resource exhaustion | 巨大JSON、ログ、IPC payload | byte/count/depth/time上限、bounded capture、rate limit |
| privacy leakage | full path、username、notification content | 最小保存、redaction、raw output禁止、export preview |

対象外の攻撃者は、すでにkernel/SYSTEMを完全支配している攻撃者である。ただしその前提を理由に、standard userからのlocal spoofingを無視しない。

## 4. UI / Tauri 境界

- remote content、remote script、CDN、任意navigation、新規windowを許可しない。
- CSPは `default-src 'self'`、必要なconnect/image/styleだけを個別許可する。unsafe evalは使わない。
- frontendへshell、process、filesystem、SQL、registryの汎用pluginを公開しない。
- Tauri commandは用途別に分け、app manifestへcustom commandを明示登録したうえでwindow labelごとのcapabilityを最小化する。`invoke_handler`へ登録しただけのcustom commandは既定で全window/webviewから利用可能であるため、その既定値を安全境界と誤認しない。
- command引数はRust側で再deserialize・schema検証し、UIで検証済みという主張を信用しない。
- preview tokenは短命で、Action IDs、parameters、before fingerprint、build、expiryに結び付ける。commit時に再検出する。
- DevTools、debug protocol、source mapのproduction公開方針をrelease gateで確認する。

一次資料: [Tauri Permissions](https://v2.tauri.app/security/permissions/)、[Capabilities](https://v2.tauri.app/security/capabilities/)、[CSP](https://v2.tauri.app/security/csp/)。

## 5. 権限分離

既定editionはper-user・`asInvoker`・helperなし・admin Actionなしである。Task 2ではelevated transportを起動せず、production allowlistをdeny-allに固定する。

### standard core

- `asInvoker`で実行する。
- HKCU、読み取り、game watcher、SQLiteを担当する。
- HKCU Actionを別資格情報のelevated helperへ送らない。
- admin不要ActionでUACを出さない。

### elevated helper

以下は次Taskで実装するmachine-scope opt-in helperの拘束条件である。Task 2はwire contract、strict validator、peer evidence validator、attack spike unit testまでを完成させ、helper executable自体は作らない。

- 必要なmachine-scope Actionの時だけ正式なWindows UAC経路で起動する。
- protected install directoryの固定絶対pathだけを使用し、起動前後に署名とfile identityを確認する。
- network、UI、汎用file browser、script engineを持たない。
- Action ID、protocol version、transaction ID、型付きparameter以外を受理しない。
- helper側の独立allowlist、build gate、preconditionで再検証する。
- privileged backupはhelper自身がOSから取得し、standard userが変更できないmachine-scope protected journalへ事前commitする。coreからraw復元値を受け取らない。
- 一transactionを処理したら終了する。常駐admin serviceにしない。
- 自分のtoken elevationを確認し、非昇格・想定外account/sessionならfail closedにする。

## 6. IPC 認証・完全性

次Taskのhelper serverはlocal named pipeを採用し、次をすべて満たす。Task 2のper-user版はpipeを作らず、同じschemaをdeny-all validatorで検査するだけである。

- 128bit以上のrandom endpoint、256bit one-time nonce、request ID、transaction ID、単一client、30秒以下のdeadline
- first-instance、remote client拒否、message mode、最大instance数1、既定ACL禁止
- 要求元logon SIDへread/write、Administrators/SYSTEMへfull controlを与える明示DACLと、Medium mandatory label + no-write-up。exact ACL/MILは同一admin/別adminの次Task実機試験で確定
- coreはserver PID、helperはclient PIDを公開APIで取得
- PID、creation time、session、token user/elevation、canonical image path、file identity、publisher/signature hashの相互照合
- fixed protocol version、envelope 64KiB以下、parameter 32件以下、strict serde unknown/duplicate field拒否
- request UUIDと単調なmessage counter、issued/deadline、nonce定数時間比較による重複/replay拒否
- helperがAction IDからresource/handlerを再解決し、wire上のpathやcommandを実行しない
- stageごとのtyped resultと限定error code。秘密・nonceを返さない

Windowsのnamed pipe既定descriptorはEveryone/anonymousにもreadを許すため利用しない。[Named Pipe Security](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)。

Task 2ではoversize、unknown/duplicate field、不正nonce、replay、counter逆転、期限切れ、Action ID/version/schema不一致、PID creation time再利用、session/token/image/file identity/publisher不一致をunit attack spikeでfail-closed確認する。UACの別管理者資格情報、Fast User Switching、RDP/multi-session、endpoint squatting、client先着DoS、WDAC/AppLockerを含む実transport侵入試験はhelper実体と同じ次Taskで行う。

helperのprotected journalはAction/version/resource、lossless backup、applied fingerprint、codec、integrity metadataを所有する。core側user DBにはopaque backup IDと非機密summaryだけを置き、rollbackもopaque IDで要求する。record欠落・改ざん・version不一致ではuser DBの値を代用せず`RECOVERY_REQUIRED`にする。

## 7. 引数・path・command検証

### 共通

- IDはcompile-time registryに存在する値だけ。
- JSON/IPCは追加field、duplicate key、NaN/Infinity、不正Unicodeを拒否する。
- enumは未知値を拒否し、数値は型・上下限・単位を検証する。
- collection、文字列、ネスト、ファイルsize、処理時間に上限を設ける。
- locale依存case変換でsecurity判断しない。

### path

- UI表示文字列ではなく、file pickerから得たhandleを起点にする。
- absolute path化だけで終えず、handleからfinal path、volume、file ID、file typeを確認する。
- MVPの自動起動/game bindingはlocal fixed drive上の通常fileに限定する。
- relative path、環境変数、wildcard、device path、UNC、ADS、未確認reparse、directoryを拒否する。
- 登録後にfile ID/署名/hashが変われば自動適用せず再承認を求める。
- command line全体を保存・比較せず、executableと型付きargvを分ける。
- app起動は端末上で明示登録したopaque app IDだけをActionから参照する。共有profile/AIからpath、app登録、argvを指定できない。

### command実行

- UI/profile/AI入力から任意のシェル文字列を作らない。
- official CLIのexecutable path、subcommand、固定optionはAction実装に埋め込む。
- app起動のMVPは引数なしとする。将来もapp別固定schemaだけを許可し、known shell/script host、system binary launcher、file associationを汎用appとして登録できない。
- Windows process起動では検証済み絶対EXEをapplication nameへ明示し、将来の型付きargvはadapterだけが試験済みWindows quotingでcommand line化する。handle継承を無効にし、argv配列を直接受けるAPIだと仮定しない。
- exit code、bounded stdout/stderr、timeoutを必ず扱い、失敗を成功へ丸めない。
- raw stdout/stderrは個人情報を含み得るため既定で永続化しない。解析に必要な限定codeだけ抽出する。

一次情報: [CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw)、[Creating Processes](https://learn.microsoft.com/en-us/windows/win32/procthread/creating-processes)。

### registry

- hive/subkey/value/viewはAction実装の固定値または限定enumから解決する。
- UI/profile/helper wireから自由形式registry pathを受け取らない。
- readとwriteで最小access rightsを使い、全権限を要求しない。
- raw type/bytesをlosslessにbackupし、rollback前にapplied fingerprintと比較する。

## 8. 共有プロファイル

共有可能:

- schema version、名前、説明、用途
- 登録済みAction IDの組合せ
- Action schemaで共有可と指定した安全な値
- 抽象的なアプリ起動設定、built-in theme/reference ID
- 対応Windows version/build宣言

共有・実行不可:

- 任意のシェルスクリプト、cmd/bat、DLL、EXE、JS、regファイル
- local absolute/relative path、任意command、任意URL、自動download
- registry path/valueを直接指定するfield
- credential、token、environment、個人情報

importはuntrusted inputとして扱い、UTF-8 JSONの固定schema、例えば256KiB以下・Action 100件以下・nesting 16以下という保守的上限をTask 2で確定する。unknown field、duplicate Action、循環dependency、experimental/forbidden IDを拒否する。署名は作者の信頼表示には使えても、schema検証を省略する権限にはしない。

import後は実際に行う変更、現在→適用後、危険度、権限、restart、Update影響、rollback可否を全件previewし、ユーザー確認なしに適用しない。local game executable bindingは共有profileと別データとして端末上で選び直す。

## 9. AI 機能

- AIは登録済みAction IDとschema内parameterだけを返す。
- code、command、script、registry path、URLを生成・実行しない。
- AI応答はuntrusted JSONとして同じprofile schemaよりさらに狭いschemaで検証する。
- unknown Action/field、experimental、禁止Action、build非対応を拒否する。
- AIは直接commit tokenを持たず、候補をpreviewへ渡すだけ。
- 適用前にユーザーへ全変更を表示する。
- promptや応答へfull local path、ログ、秘密を送らない。外部AIを採用する場合は別途privacy同意とdata flow reviewを必須とする。

## 10. 更新・署名・supply chain

- updaterはHTTPSに加えTauri artifact署名を必須とし、検証無効化optionを本番で使わない。
- Windows installerと全実行体をAuthenticode署名し、timestampを付ける。
- helperを単独差替えできないよう、core/helper/protocol/action catalogのversion集合を起動時に照合する。
- install directoryとupdate stagingのACLを確認し、standard userがhelperを置換できないようにする。
- helperを含むeditionはmachine-scope protected installとする。per-user editionを出す場合はhelper/admin Actionを含めず、user-writable pathへ後付けしない。
- update metadataは既存Actionを停止できても、新しいbinary/path/valueをdataだけで追加できない。
- active transaction/sessionがあればupdateを延期し、rollbackとDB backup後に進む。
- dependency lock、license、脆弱性、reproducible artifact/SBOM、CI secretアクセスをrelease reviewに含める。
- WebView2は独立更新されるため、stableに加え先行channelでUI互換試験を行う。

## 11. storage・privacy・ログ

- standard-user ActionのSQLite/backupはuser-local directoryに置きACLを確認する。privileged backupはhelper所有のmachine-scope protected storeへ分離し、coreはopaque IDだけを保持する。
- local game pathは機能上必要なためDBには保存するが、community export、通常ログ、診断exportから除外する。user-scope DPAPI列暗号化をTask 2で評価する。
- notification content、contact、window title、game memory、keystrokeを収集しない。
- process監視は登録exeとのidentity照合に必要な最小情報だけを使い、全process command lineを保存しない。
- ログはAction ID、stage、result/error code、build、時間、匿名diagnostic IDを中心にする。
- stdout/stderr、username、home path、profile名等をredactし、rotation/size/retention上限を持つ。
- 診断exportは内容previewと明示保存先選択を必須にする。

## 12. 永久禁止と実験隔離

Defender/Firewall無効化、Windows Update完全停止、pagefile無効化、HPET/BCD変更、大量service停止、process injection、game process/file改変、任意code実行は、experimental hostにもAction registryにも入れない。profile/AI/pluginから要求された場合は明示的に拒否する。

実験候補は別binary、別namespace、別allowlist、既定無効、exact tested build、毎回同意、単独適用、auto profile/共有/AI禁止とする。別process化は未信頼codeを安全にするsandboxではないため、first-party署名済み実装しか入れない。

## 13. セキュリティ受入試験

- rendererから未許可command、追加field、巨大payload、stale previewを送る
- 偽core/helper、同名exe、署名違い、PID再利用、別sessionからIPC接続
- IPC replay、順序逆転、先着接続、timeout、途中切断
- 64KiB超過、unknown/duplicate field、nonce/action version/transaction mismatch、期限切れ、deny-all allowlist迂回
- path traversal、UNC、reparse、hardlink、file差替え、argument edge case
- profile parserのduplicate key、deep nesting、zip/size bomb相当、未知Action
- updater/helper差替え、署名不一致、version downgrade、active transaction中update
- registry 32/64 view、policy managed、ACL拒否、外部書戻し
- logs/exportに秘密、username、full pathが含まれないこと
- forbidden Action IDがUI、profile、AI、experimentalの全経路で拒否されること

重大な境界の実装後は、threat model更新、static analysis、dependency audit、fuzzing、Windows標準ユーザー/別管理者UAC環境での第三者レビューをrelease gateにする。
