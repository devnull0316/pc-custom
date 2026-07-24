# アーキテクチャ

## 1. 技術構成の決定

**候補 A: Tauri 2 + React + TypeScript + Rust + windows-rs + SQLite を推奨する。** CC の暫定推奨に賛成する。

理由は、最重要の権限分離を Rust コアと短命な Rust 昇格ヘルパーで閉じられ、UI を閉じた常駐状態では WebView を破棄して Rust コアだけを残せるためである。Electron でも専用 helper を使えば安全性と Windows API 深度は実現できるが、Chromium/Node のプロセス群を常駐させるか、別の native watcher を追加して Electron を終了する必要がある。後者では二つのランタイムと起動 IPC を管理する構成になる。

### 1.1 採点方法

0〜5点を付け、`重み × 評点 ÷ 5` を加重点とする。BRIEF §4 は優先順を示しており数値配点そのものは指定していないため、本設計ではその順を `30 / 25 / 20 / 15 / 10` と数値化した。メモリは同一アプリの実測がまだないため、構造上の暫定評価であり、Task 2 のベンチマークで更新する。

| 要件 | 重み | A 評点 / 加重点 | B 評点 / 加重点 | 根拠 |
| --- | ---: | ---: | ---: | --- |
| 安全な権限分離 | 30 | 4.5 / 27.0 | 4.0 / 24.0 | 両者とも別 helper なら成立。A は Rust core と Tauri capability で UI 露出を狭めやすい。B は renderer/main/preload/native helper の監査境界が増える |
| 低い常駐メモリ | 25 | 4.0 / 20.0 | 2.5 / 12.5 | A は Windows 11 の共有 WebView2 を使い、UI破棄後は native core のみ残せる。B の標準構成は Chromium/Node の複数プロセス。数値は未計測 |
| Windows API 統合 | 20 | 5.0 / 20.0 | 5.0 / 20.0 | B も Rust helper を採用すれば windows-rs を利用でき深度は同等。A は native 処理までの配線が短い |
| 署名・配布・更新 | 15 | 4.5 / 13.5 | 4.0 / 12.0 | A は MSI/NSIS と署名必須 updater artifact が公式導線にある。B も Forge/Squirrel/MSIX で成立するが構成選択が増える |
| Update追従・復旧・拡張・開発 | 10 | 4.0 / 8.0 | 4.0 / 8.0 | A は単一native層と型が有利、Rust習熟とWebView2更新試験が負担。B はTS開発と固定Chromiumが有利、Electronとhelperの更新を二重管理 |
| **合計** | **100** | **88.5** | **76.5** | **Aを採用推奨** |

CC 記載の「Electron 100〜200MB級」は本リポジトリ条件では未計測なので、採点の実測根拠には使わない。一次資料で言えるのは、Electron は Chromium 由来の multi-process model と Node main process を持ち、Tauri/Windows 11 側は OS に含まれる共有 WebView2 Runtime を使える、という構造差までである。

感度分析として、B を「native watcher だけ常駐し Electron は完全終了」としてメモリ評点を4.0へ上げると B は84.0点、権限分離も4.5へ上げると87.0点になる。それでも A の88.5点を下回るが差は小さい。したがって推奨は A で固定しつつ、Task 2 で公平な実測を行う。

### 1.2 一次資料による補強

- Tauri は window/webview ごとに permission/capability を限定できる: [Tauri Permissions](https://v2.tauri.app/security/permissions/)、[Capabilities](https://v2.tauri.app/security/capabilities/)
- Windows 11 には Evergreen WebView2 Runtime が含まれ、共有・自動更新される: [WebView2 Evergreen vs Fixed](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version)
- Electron は main/renderer/utility 等の multi-process model を持つ: [Electron Process Model](https://www.electronjs.org/docs/latest/tutorial/process-model)
- Electron 自身も sender 検証、sandbox、context isolation 等を必須の安全策としている: [Electron Security](https://www.electronjs.org/docs/latest/tutorial/security)
- `windows` crate は Microsoft の Windows API projection: [windows-rs](https://github.com/microsoft/windows-rs)、[API docs](https://microsoft.github.io/windows-docs-rs/)
- Tauri は Windows の MSI/NSIS を生成でき、updater は署名検証を無効化できない: [Windows Installer](https://v2.tauri.app/distribute/windows-installer/)、[Updater](https://v2.tauri.app/plugin/updater/)

## 2. プロセス構成

既定editionは**per-user・`asInvoker`・helperなし・admin Actionなし**である。下図のelevated helper経路は次Task以降のmachine-scope opt-in構成を示し、今回の実行体には存在しない。

```text
標準権限
┌────────────────────────────────────────────────────┐
│ totonoe.exe (Rust core / asInvoker)                │
│  Action調停・SQLite・互換判定・tray・game watcher   │
│       ├─ 必要時だけ生成 ─ React UI on WebView2      │
│       └─ UAC起動・相互認証 IPC ───────────────┐     │
└───────────────────────────────────────────────│─────┘
                                                ▼
                                     ┌──────────────────────┐
                                     │ totonoe-elevated.exe │
                                     │ allowlisted Action   │
                                     │ 1 transactionで終了  │
                                     └──────────────────────┘

既定無効・非常駐: totonoe-experimental-host.exe
```

### 2.1 `totonoe.exe`

- 実行 manifest は `asInvoker`。常時管理者で起動しない。
- Rust core が Action レジストリ、transaction coordinator、compatibility service、SQLite、トレイ、ゲーム検知を所有する。
- Windows 操作は `windows-rs` から公開 Win32/COM/WinRT API を呼ぶ。型が生成できるだけでは公開契約の証明にならないため、各 Action が一次資料URLと検証buildを別途持つ。
- HKCU と読み取り専用の通常 Action はこの標準権限コアで処理する。別管理者資格情報による UAC では HKCU の主体が変わるため、HKCU を elevated helper に送らない。

### 2.2 React UI / WebView2

- ローカルに同梱し、署名された frontend asset だけを読み込む。remote HTML/JavaScript、CDN script、任意 navigation を許可しない。
- UI command は `detect`, `preview`, `commit`, `rollback`, `history`, `profile` 等の用途別 API に限定する。
- shell、任意 process、任意 filesystem、任意 registry、汎用 SQL は公開しない。
- Tauriのcustom commandは`invoke_handler`登録だけでは既定で全window/webviewから利用可能なため、app manifestのcommand宣言とwindow label別capabilityを明示し、wildcardを使わない。
- CSP は `default-src 'self'` を基点に最小化する。
- ウィンドウを閉じたら hide ではなく WebView/window を破棄し、トレイから必要時に再生成する。
- WebView compromise を想定し、core 側で全引数と transaction state を再検証する。Tauri capability は helper 認証の代わりにはならない。

### 2.3 `totonoe-elevated.exe`

Task 2ではこのbinaryを作成しない。IPCの型、deny-all allowlist、validation、attack spikeだけを先に固定し、helper実体は次Taskで保護install・署名・実機攻撃試験と一体で実装する。

- helperを含むeditionでは、標準ユーザーが置換できないmachine-scopeの保護インストール先へUI/coreと同時に配置し、全binaryを同一publisherで署名する。
- per-user editionを提供する場合はhelperとadmin Actionを含めず、後からuser-writable pathへhelperだけをdownload/生成しない。
- 固定絶対パスと Windows の正式な UAC 昇格手段で起動し、PATH 検索をしない。
- 一回の transaction に必要な Action ID と型付きパラメータだけを処理して終了する。常駐 service にしない。
- 任意コマンド、任意 executable、任意 URL、自由形式 registry path を受理しない。
- helper 内でも build、precondition、対象 resource、Action allowlist を再解決する。
- UAC 取消、timeout、helper crash、exit code を transaction result として保存する。

### 2.4 昇格 IPC

次Taskのmachine-scope opt-in helperは、**helperをserverとする一回限りのローカル名前付きパイプ**を第一候補とする。Task 2のper-user版はtransportを起動せず、compile-time deny-allで全elevated Actionを拒否する。

必須条件:

1. UAC起動ごとに128bit以上の乱数を含むpipe名、256bit nonce、transaction ID、request IDを作る。
2. helper は最初の一instanceだけを作り、remote client を拒否する。
3. 既定 security descriptor は使用しない。要求元user/logon SID、Administrators、SYSTEMを候補とする明示DACLとmandatory labelを使う。ただしover-the-shoulderで別admin accountになる場合のexact SDDL/MILは未確定であり、次Taskの実証に合格しない構成は出荷しない。
4. core と helper は公開APIで相手のPIDを取得し、起動時に期待したPIDと一致することを確認する。
5. 双方が process creation time、session、token、正規化 image path、署名 publisher/hash も確認し、PID再利用と同名偽装を防ぐ。
6. envelopeは`protocolVersion, requestId, transactionId, messageCounter, issuedAt, deadline, nonce, actionId, actionVersion, typedParameters`の固定schemaとし、unknown/duplicate field、unknown Action/version、範囲外値を拒否する。
7. request/responseは64KiB以下、単一client、30秒以下の期限、単調増加message counterとする。nonce、SID、image pathはログへ残さない。
8. helperが適用直前にOSから状態を再検出し、standard userが変更できないmachine-scope storeへprivileged backupと`PREPARED`をdurable commitしてから変更する。coreからraw復元値を受け取らない。
9. 一要求処理後に pipe を閉じて helper を終了する。

一次資料: [Named Pipe Security](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)、[CreateNamedPipeW](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew)、[client PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid)、[server PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeserverprocessid)。

`ncalrpc` は local IPC の公式推奨であり、次Taskの比較spike対象に残す。ただし Rust/MIDL build、UAC資格情報差、署名照合を含む検証前は主案にしない。[Protocol Sequence](https://learn.microsoft.com/en-us/windows/win32/rpc/selecting-a-protocol-sequence)。

## 3. コンポーネント境界

| コンポーネント | 責務 | 禁止 |
| --- | --- | --- |
| UI | 結果表示、preview、明示確認、timeline | OS直接変更、shell、SQL、Action実装 |
| Action catalog | metadata、schema、説明、compatibility key | 動的コード、外部URL由来ロジック |
| Transaction coordinator | preflight、順序、journal、逆順rollback | backup未確定のapply |
| Windows adapters | Registry/Power/Process等の限定primitive | 任意path/commandを取る汎用primitive |
| Compatibility service | build/probe/試験行列、quarantine | 未知buildの推測許可 |
| Game profile engine | state machine、lease、競合、復旧 | game注入、game file変更 |
| Elevated helper | machine-scope allowlist処理 | UI表示、network、常駐、自由形式要求 |
| Experimental host | 実験Actionの隔離、強制quarantine | stable Action ID、共有/AI/自動profile |

依存方向は UI → application service → Action interface → Windows adapter とする。Action が UI や SQLite の物理 schema を直接参照しない。rollback decoder は Action version ごとに残し、アプリ更新後も古い履歴を読めるようにする。

CC監査P2に従い、Explorer/theme系registry writeの反映は`SHChangeNotify(SHCNE_ASSOCCHANGED, ...)`または対象別のbounded `WM_SETTINGCHANGE`を既定とする。今回のstable ActionはExplorerを強制終了・再起動せず、registry検証結果と画面反映結果を分けてjournal/UIへ返す。

## 4. データ層

標準権限Action用SQLiteは`%LOCALAPPDATA%\Totonoe\data\totonoe.db`に置き、標準ユーザーだけがアクセスできるACLを確認する。DBはUIから直接開かない。

| テーブル | 主な内容 |
| --- | --- |
| `schema_meta` | DB schema version、migration状態 |
| `os_observations` | base build、architecture、edition、runtime probes、観測時刻 |
| `transactions` | 目的、状態、owner、開始/終了、app/protocol version |
| `transaction_items` | 順序、Action ID/version、precondition、各段階、結果 |
| `backups` | 型付き変更前状態、適用値fingerprint、codec version |
| `verification_events` | detect/apply/rollbackの観測値とエラー |
| `profiles` / `profile_actions` | ローカルprofileと登録済みActionの組合せ |
| `game_bindings` | ローカルexe file identityとprofileの対応。共有対象外 |
| `active_profile_sessions` | 状態、検知process、未復元の有無 |
| `action_leases` | resource key、desired fingerprint、owner集合 |
| `audit_events` | 機密情報を除いた構造化診断情報 |

### 特権backupの二層journal

将来admin Actionを有効にする場合、user-scope SQLiteをhelperの復元根拠にしない。elevated helperは対象状態を自身で読み、ProgramData配下等のstandard userが変更できないmachine-scope storeへ、transaction/Action/version/resource、lossless backup、applied fingerprint、codec、integrity metadataを保存する。exact pathとACLは次Taskのinstaller/IPC spikeで決める。

core側SQLiteが保持するのはopaque privileged backup ID、Action ID、非機密summary、helper resultだけである。rollback requestもAction ID、transaction ID、opaque backup IDだけを送り、helperが保護storeと現状態を照合する。両journalのcommit順とcrash reconciliationをprotocol versionの一部として故障注入試験する。helper側recordが無い、改ざん、version不一致の場合は自動writeせず`RECOVERY_REQUIRED`にする。

protected storeにはraw backupとは別に、要求元SIDがread-onlyで確認できる署名/ACL保護されたpending indexを持たせる候補を次Taskで検証する。coreは起動時にuser DBのopaque IDとpending indexを照合し、未完了があれば新規mutationを止めてhelper recoveryを要求する。UAC取消やindex不一致では復旧を保留し、user DBを正として消し込まない。これによりuser DB破損・消失だけでprivileged変更を「未復元なし」と誤認しない。exact ACL、別admin account、index完全性はIPC spikeの合格項目である。

二層commitは次の順に固定する。coreはUAC起動前に`transaction ID + Action ID/version + local SID`を`PRIVILEGED_REQUEST_PREPARED`としてuser DBへcommitする。helperはSIDをwire値から信用せずIPC client tokenから取得し、coreのlocal SIDと一致したtupleをidempotency keyとしてprotected recordとbackupをcommitする。適用後にopaque backup ID/resultを返し、coreは最後にそのIDを結び付ける。helper commit後・core受領前にcrashした孤児recordは、次回起動時にpending indexまたはUAC後の`list nonterminal for verified requesting SID`で発見し、tupleからcore itemへ再結合する。helperはcoreのack前に非終端recordを削除せず、coreもhelperのterminal確認なしにprepared itemを消さない。

### 耐障害性

- WAL を含む具体的 SQLite durability 設定は Task 2 で停電試験と共に決める。既定値を安全と仮定しない。
- 外部変更前に backup 行と `PREPARED` 状態を durable commit する。
- 各 item の apply/verify/rollback 結果を次の外部変更前に commit する。
- DB migration 前に整合性確認と退避を行い、active transaction があれば更新しない。
- backup は registry raw bytes 等を lossless BLOB として保存し、表示用JSONへ丸めない。
- profile のローカル実行パス等は共有・通常ログへ出さない。必要列は user-scope DPAPI の採否を Task 2 で評価する。

## 5. 常駐設計

- Rust core、tray、transaction recovery、game watcher だけを残し、WebView/window は未生成または破棄済みとする。
- elevated/experimental host は通常存在しない。
- game watcher は初期 snapshot、イベント通知、process handle wait、低頻度の取りこぼし補正を組み合わせる。WMI event と polling の負荷は Task 2 で比較する。
- process name だけでは一致させず、最小権限 handle から得た canonical image path、file identity、PID creation time を照合する。

### 測定ゲート

同一 React bundle、同一検知周期、release build、DevTools無効、同じ Windows build で A/B を測る。

1. tray/watchだけでUI未生成
2. UI表示中
3. UIを一度開いて破棄した後
4. game検知中
5. profile適用/復元中

各状態を10回以上測り、全 process tree の Private Working Set、Commit、CPU、wakeups、handle/thread 数の中央値とp95を記録する。実行手順は`scripts/MEMORY_AB.md`、Private Working Setの取得は`scripts/measure-private-working-set.ps1`を正とする。B は Electron main 常駐案と native watcher 常駐・Electron完全終了案の両方を測る。採用前のMB値を文書や広告で断定しない。

## 6. 配布・署名・更新

- 既定artifactはper-userのstandard-user Action限定editionとし、elevated helper/admin Actionを同梱・後付けしない。
- admin Actionを必要とする利用者向けには、helperを保護できるper-machine NSIS/MSIを次Task以降の明示opt-in候補として比較する。installer時のUACとruntimeの標準権限は分ける。
- core、elevated helper、experimental host、installer を Authenticode 署名し、timestamp を付ける。
- Tauri updater artifact の署名鍵と Authenticode 証明書は目的が異なるため両方使う。private key をCI secretとして分離する。
- HTTPS と署名の両方を必須とし、insecure transport option は本番で無効にする。
- active profile、`APPLYING`、`ROLLING_BACK`、`RECOVERY_REQUIRED` が一つでもあれば更新開始を拒否する。
- 更新前に一時 Action を復元し、DB backup、migration dry-run、helper protocol互換性を確認する。
- WebView2 Evergreen の欠落 edge case は installer で検出し、Microsoft 公式配布方針に従う。
- app 更新後と WebView2/Windows 更新検知後に compatibility self-check を行う。

## 7. 将来のプラグイン像

### stable 拡張

- 初期版は動的プラグインを読み込まない。
- community が共有できるのは schema 検証された data-only profile だけである。
- Action 実装の拡張は、Totonoe に同梱され、同一publisherで署名され、compile-time registry に登録された first-party module に限定する。
- DLL、JS、任意のシェルスクリプト、EXE、任意 URL download を runtime plugin として読み込まない。

### 実験的モジュール

`totonoe-experimental-host.exe` を stable core/helper と別にする。

- Action ID は `experimental.*` namespace。
- 別 binary、別 allowlist、別 compatibility manifest、既定未インストールまたは既定無効、非常駐。
- exact tested build だけで有効。未知buildは detect-only。
- 毎回明示同意、単独適用、詳細previewを必須にする。
- ゲーム自動適用、community profile、AI候補、複数Action一括適用から除外する。
- crash/verify失敗で自動 quarantine し、ユーザー操作なしに再試行しない。
- standard-user実験Actionのrollback記録は中央SQLiteの別namespaceへversion付きで保存し、experimental hostの生存だけに依存しない。将来のprivileged実験Actionはstable helperを流用せず、専用の保護store設計に合格するまで実装しない。
- 分離は保守・障害境界であり、未信頼コードを安全にするsandboxとは説明しない。

候補は通知/DND、Hz/HDR自動切替、既定audio切替、Explorer再起動、ETW監視、taskbar内部設定、startup変更、Task Scheduler、WinGet自動導入、third-party連携である。BRIEFの永久禁止項目は experimental にも入れない。

## 8. 重要な未検証事項

- Tauri でwindow破棄後にRust event loopだけを維持する時の実メモリと再生成安定性
- 名前付きパイプの UAC 別資格情報、multi-session、PID再利用、endpoint squatting、WDAC/AppLocker環境
- per-machine NSIS/MSIにおけるhelper/protected journalの配置ACL、署名、更新の原子性、およびhelperなしper-user editionの機能分離
- windows-rs採用版が必要な最新 Windows API metadata を含むか
- WMI event と snapshot polling の負荷・欠落率
- WebView2 Evergreen 更新直後のUI互換性

これらは設計上の未決ではなく、standard-user項目はTask 2、helper関連項目は次Taskの受入試験であり、合格前に「対応済み」と表示しない。
