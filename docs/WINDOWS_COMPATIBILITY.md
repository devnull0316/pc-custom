# Windows 11 互換性設計と一次情報調査

## 1. 調査条件

調査基準日は **2026-07-24**。Microsoft Learn、Windows release health、Windows API documentationを一次情報として優先した。本書の「資料確認済み」はAPIや記載の存在を確認したという意味であり、PCカスタムの実装・実機・driver構成で動作確認済みという意味ではない。実機確認前の項目は明示的に`未検証・要CC確認`とする。

互換性は次の三層を分ける。

1. **Microsoft support**: OS edition/versionがMicrosoftのservicing期間内か。
2. **PCカスタム tested**: そのbuild・architecture・代表hardwareでActionのdetect/apply/rollback試験が合格したか。
3. **runtime available**: その端末でAPI、setting、device、policyが実際に利用可能か。

Microsoftがsupport中でも、2または3を満たさなければ自動変更を有効にしない。

## 2. MVP対象Windows

Microsoftのrelease information/lifecycleを2026-07-24に確認した時点の判断である。latest revisionは毎月変わるため、製品へ固定値として埋め込まない。

| Version | Base build | Home/Pro support end | PCカスタム MVP方針 |
| --- | ---: | --- | --- |
| Windows 11 24H2 | 26100 | 2026-10-13 | 初期test matrix対象。ただし出荷日が終了日に近いため、出荷時に再判定する |
| Windows 11 25H2 | 26200 | 2027-10-12 | 初期test matrixの主対象 |
| Windows 11 26H1 | 28000 | 2028-03-14 | OSは検出するが、対応実機matrixが揃うまで自動変更は無効。24H2/25H2からのin-place updateではなく新hardware向け |
| Windows 11 23H2 | 22631 | Home/Proは終了 | MVP対象外。Enterprise/Educationがsupport中でもPCカスタム未試験としてread-only案内 |

26H1が既存端末への通常のfeature updateではないことはMicrosoft release informationに明記されている。したがって「buildが大きいから25H2と同等」と推測せず、hardware/driverを含む独立行として試験する。

一次情報:

- <https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information>
- <https://learn.microsoft.com/en-us/lifecycle/products/windows-11-home-and-pro>
- <https://learn.microsoft.com/en-us/windows/release-health/status-windows-11-26h1>

## 3. OS identityの取得

### 3.1 取得項目

起動時に`OsIdentity`として次を集約する。

| 項目 | 主取得手段 | 用途 |
| --- | --- | --- |
| version/base build | WMI `Win32_OperatingSystem.Version` / `BuildNumber` | compatibility row選択 |
| revision | 64-bit registry viewの`CurrentBuildNumber`と`UBR`を補助取得 | monthly update単位の記録とtest照合 |
| edition/SKU/product type | WMI `OperatingSystemSKU`, `ProductType`、必要なら`GetProductInfo` | Home/Pro/Enterprise、client判定 |
| architecture | native system情報とprocess architecture | helper/API/registry view選択 |
| install/update identity | last successful self-check時のfingerprint | Update後の再検査開始条件 |
| feature probes | API export、setting、device capability、policy状態 | buildだけでは分からない機能可否 |

WMIの`BuildNumber`はbase buildの一次取得に使える。`UBR`参照はMicrosoft Intuneの公式update package手順で使用例があるが、一般purposeのversion APIとして契約されたものとは扱わない。取得失敗時はrevision不明のまま安全側に倒し、base buildとruntime probeで判定する。`GetVersion`系のmanifest依存に単独で依存しない。

一次情報:

- `Win32_OperatingSystem`: <https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-operatingsystem>
- version helperの注意: <https://learn.microsoft.com/en-us/windows/win32/sysinfo/operating-system-version>
- Microsoft Intuneでの`CurrentBuildNumber`/`UBR`使用例: <https://learn.microsoft.com/en-us/intune/app-management/deployment/deploy-win32-update-package>

### 3.2 集中管理

Action codeへbuild番号を散在させない。署名対象のcompatibility catalogに次を保持する。

- OS version、minimum/maximum tested build、tested revision範囲
- Action ID/version、method ID、必要API、registry view、必要feature probe
- `detect-only` / `guided` / `mutable` / `blocked`の許可mode
- 既知の問題、rollback decoder版、再起動/Explorer反映条件
- hardware条件（AC/DC、display topology、HDR、DRR、audio endpoint等）
- test evidence ID、確認日、承認者、kill switch

catalogは機能を狭める更新だけを即時適用できる。未知buildで自動変更を新たに許可する変更は、署名済みapp updateまたはCC承認済みcompatibility releaseとtest evidenceを要求する。remote documentからAction pathやcommandを追加できない。

## 4. 起動時self-checkと判定

起動ごと、およびOS fingerprint変化後に次を行う。

1. 未完了rollbackを最優先で回復する。
2. OS identityとedition/support statusを再取得する。
3. compatibility catalogの該当rowを探す。
4. Actionごとのruntime probeと`detectCurrentState()`を副作用なしで実行する。
5. backup decoderがそのAction versionを復元できるか確認する。
6. 結果をActionカードへ表示し、自動適用の可否を決める。

| 判定 | 振る舞い |
| --- | --- |
| `tested_mutable` | 通常の事前previewとbackupを経て変更可能 |
| `tested_detect_only` | 現在状態のみ表示。変更buttonは出さない |
| `supported_unverified_revision` | detect/guidedのみ。実機test合格まで自動変更しない |
| `unknown_build` | recoveryとread-only診断を許可し、新規自動変更を停止 |
| `feature_missing` | そのActionだけ非対応。代替の公式Settings導線を表示 |
| `policy_managed` | 組織policyを上書きせず、管理対象であることを表示 |
| `state_unknown` | 未適用と誤認せず、apply/rollbackを止める |

未知buildでも、過去のbackupを読むrollback decoderは削除しない。ただし同じraw値でも新buildでresourceの意味や反映方法が変わり得るため、現状態が適用fingerprintと一致するだけでは自動writeしない。Action/versionごとの`rollbackAcrossUnknownBuild`が明示承認され、runtime probeで同じ公開契約とdecoderを確認できたitemだけ自動復旧する。未承認は`RECOVERY_REQUIRED`、第三状態はユーザー確認へ送る。

## 5. BRIEF §8 調査結果

| 調査項目 | 一次情報で確認できたこと | PCカスタムでの決定 | 状態 |
| --- | --- | --- | --- |
| ビルド番号取得 | WMI `Win32_OperatingSystem`はVersion/BuildNumber/SKU等を公開する。Intune資料にCurrentBuildNumber/UBRの使用例がある | WMIを主、64-bit registryのUBRを補助、feature probeを併用 | 資料確認済み、実装未着手 |
| レジストリ操作 | `RegOpenKeyEx`、`RegQueryValueEx`、`RegSetValueEx`、`RegDeleteValue`等は公開API。WOW64 view指定がある | key/value存在、type、length、raw bytes、view、適用値をlossless保存。最小accessで開く | API確認済み、全対象build実機未検証 |
| Explorer安全再起動 | Explorerを安全に再起動する一般purposeの公開APIは確認できない。`WM_SETTINGCHANGE`や`SHChangeNotify`は変更通知であり再起動保証ではない | 通知で反映を試し、必要ならsign-out/手動案内。process強制終了はMVP禁止、experimental候補 | 公開再起動API未確認・要CC確認 |
| 通知/Focus Assist | Settings URIは公開されるが、通常desktop appがDo Not Disturb全体を確実にtoggle/rollbackする公開setterは確認できない | `ms-settings:notifications` / `ms-settings:quiethours`へのguided Actionのみ。自動toggleしない | setter未確認・要CC確認 |
| 電源モード | Windows 11 desktop app向け`PowerGet/SetUserConfiguredACPowerMode`とDC版、3つのsupported GUIDが公開 | AC/DCを別々にdetect/backup/apply/verify。OS/firmware/policyによるeffective modeとの違いを表示 | API確認済み、権限・端末差は実機未検証 |
| Windows Game Mode | MicrosoftのSettings status資料に`HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled`が掲載されるが、第三者書込契約やeffective state APIとは断定できない | 状態参照と公式Settings案内を優先。自動registry writeはbuild別実機確認まで無効 | write contract未確認・要CC確認 |
| モニターHz | `QueryDisplayConfig`、`SetDisplayConfig`、`ChangeDisplaySettingsEx`等は公開。topology/driver/DRRの考慮が必要 | MVPはread-only readiness。将来自動変更は全topologyを保存し、timeout/revert付きexperimentalでのみ検討 | read API確認済み、write安全性未検証 |
| HDR | 公開enumにadvanced color/HDRのget/set識別子はある一方、set用packet本文、minimum build、API説明には不整合が残る | MVPは対応/現在状態のread-only。自動toggleは契約確認、display identity、color state、driver matrixが揃うまでexperimental | 実用write契約・hardwareとも未検証、要CC確認 |
| オーディオデバイス | Core Audioのendpoint列挙、default endpoint取得、notification callbackは公開 | MVPはdefault render deviceのread-only readinessとSettings導線。OS既定endpointを設定する公開setterは確認できず、非公開`IPolicyConfig`は使わない | setter未確認・要CC確認 |
| ゲームprocess起動/終了 | Toolhelp snapshot、WMI process trace、限定権限process handle、full image path取得、handle waitは公開 | 初期snapshot + WMI start/stop + full-path/creation-time照合 + handle wait +低頻度補正。injection/debug privilegeなし | API確認済み、欠落率は実機未検証 |
| WinGet | MicrosoftのWinGet CLIとApp Installerによる導入経路が公開 | MVPは存在/version/source状態の検出と公式導入案内。無断bootstrap、source変更、自動repairは後回し | detect/guidedのみ |
| startup | Run/RunOnce、Startup folder、Known Folderは公開情報がある。`Win32_StartupCommand`はread-only classだが公式仕様上`SeRestorePrivilege`を要求する | MVP baselineは標準権限で読めるHKCU Run/RunOnceとuser Startup folder。WMI/HKLM/common sourcesはbest-effortで、利用不能をunknown表示。Task Manager完全再現や`StartupApproved`操作はしない | 部分inventory、権限/網羅性未検証 |
| Task Scheduler | Task Scheduler 2.0 COM APIは公開 | 自動profileのMVP要件には使わない。将来のfirst-party recovery task候補も固定definition・明示同意・削除rollback必須 | API確認済み、MVP外 |
| テーマ | Settings status資料に`AppsUseLightTheme`、`SystemUsesLightTheme`、`EnableTransparency`が掲載されるが、第三者書込契約とは限らない | detectとguidedを先行。自動writeは24H2/25H2 VM・実機で反映/rollback/contrast policy確認後のみ | write contract未確認・要CC確認 |
| 権限昇格 | UACの公式elevation verbを使う起動方法が公開 | 標準coreは昇格しない。固定・署名済み・保護install先の短命helperだけを必要時起動。HKCU Actionは標準coreで実行 | API確認済み、UAC account差を試験要 |
| 権限processとの安全IPC | Windows named pipeはsecurity descriptor、local-only flag、client/server process ID取得APIを持つ | helperがone-shot pipe serverを作り、DACL、remote拒否、nonce、期限、PID/path/signature/token相互検証を実施 | API確認済み、攻撃test未実施 |
| クラッシュ後復旧 | Application Recovery and Restart APIはあるがcallback時間等の制約があり、耐久journalの代替ではない | SQLite journalを保証の中心とし、ARRはbest-effort補助。次回起動はrecovery-first | API確認済み、故障注入未実施 |
| Update後互換性 | release health、known issues、lifecycle情報は公開。monthly updateでrevisionと機能状態が変わり得る | OS fingerprint変化時に全Action再probe。未知buildでは新規自動変更停止、signed kill switch、build matrix再試験 | 運用設計済み、実装未着手 |

## 6. 主要項目の詳細

### 6.1 レジストリ

`RegQueryValueEx`でtypeとsizeを先に取得し、raw bytesを欠落なく保存する。string terminatorの扱いを型ごとに検証し、32/64-bit viewを明示する。rollbackは元valueが無かった場合に、現在値がPCカスタムの適用値と一致するときだけvalueを削除する。原子的なempty-key compare-deleteが無いため、新規mutationは元keyが無い場合にfail-closedとし、key全体を自動削除しない。既存keyやsibling valueは消さない。

MicrosoftのSettings status pageに載るregistry pathは、設定状態を読むためのreferenceであって、すべてがthird-party write contractであるとは解釈しない。Actionごとに公開setterの有無とbuild別evidenceをcatalogへ残す。

一次情報:

- Registry functions: <https://learn.microsoft.com/en-us/windows/win32/sysinfo/registry-functions>
- `RegQueryValueEx`: <https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regqueryvalueexw>
- Registry key security/access rights: <https://learn.microsoft.com/en-us/windows/win32/sysinfo/registry-key-security-and-access-rights>

### 6.2 電源

user configured power modeはAC/DCそれぞれのGUIDを保存する。supported valueはBest power efficiency、Balanced、Best performanceの3つだけで、任意GUIDを受けない。API成功だけでなく再取得して検証する。effective power modeはbattery saver、device policy、hardware等で異なり得るため、「現在の希望mode」と「実効状態」を混同しない。

- AC get: <https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-powergetuserconfiguredacpowermode>
- AC set: <https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-powersetuserconfiguredacpowermode>
- DC get: <https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-powergetuserconfigureddcpowermode>
- DC set: <https://learn.microsoft.com/en-us/windows/win32/api/powrprof/nf-powrprof-powersetuserconfigureddcpowermode>
- Effective mode notifications: <https://learn.microsoft.com/en-us/windows/win32/api/powersetting/nf-powersetting-powerregisterforeffectivepowermodenotifications>

通常の明示的一回変更は、実機gate通過後にstable `power.user_mode` Actionになり得る。一方、ゲーム起動条件で自動的に電源modeを変える機能は、thermal/battery影響と複数profile競合が大きいためMVPではexperimentalにも既定追加しない。

### 6.3 表示Hz/HDR

readinessはdisplay device path、adapter/target ID、接続topology、resolution、refresh rateの分子/分母、virtual/physical refresh、DRR、advanced color capability/stateを記録する。将来の変更backupは「Hzの数字一つ」では不十分で、対象display identityと全path構成を含める。

display切断、dock着脱、GPU driver reset、remote desktop、multi-monitor、duplicate/extend、DRR、HDR/SDR切替中は自動適用を止める。15秒等のconfirmation timeoutで旧topologyへ戻す設計も、driverが応答しない場合の保証にはならないためexperimental扱いとする。

- Display configuration: <https://learn.microsoft.com/en-us/windows-hardware/drivers/display/connecting-and-configuring-displays>
- `QueryDisplayConfig`: <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-querydisplayconfig>
- `SetDisplayConfig`: <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setdisplayconfig>
- DisplayConfig device info: <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-displayconfiggetdeviceinfo>
- DisplayConfig device info type: <https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ne-wingdi-displayconfig_device_info_type>
- Device info setter: <https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-displayconfigsetdeviceinfo>

### 6.4 オーディオ

read-only readinessはendpoint ID、friendly name、flow/role、default endpoint、active/unplugged/disabled stateを取得する。endpoint IDはdriver再導入で変わり得るので、profileの永久識別子とはみなさない。default endpoint変更に非公開interfaceや外部utilityを使う設計は採用しない。

- `IMMDeviceEnumerator`: <https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immdeviceenumerator>
- `GetDefaultAudioEndpoint`: <https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-getdefaultaudioendpoint>
- `IMMNotificationClient`: <https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nn-mmdeviceapi-immnotificationclient>

### 6.5 process監視

`OpenProcess`はquery/synchronizeに必要な最小accessだけを要求し、SeDebugPrivilegeを有効にしない。path取得失敗を「対象外」としない。observer断、PID再利用、既存process、multiple instance、sleep/resume、launcher遷移をtest matrixへ入れる。

- Toolhelp snapshot: <https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot>
- `QueryFullProcessImageNameW`: <https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew>
- `RegisterWaitForSingleObject`: <https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-registerwaitforsingleobject>
- `Win32_ProcessStartTrace`: <https://learn.microsoft.com/en-us/previous-versions/windows/desktop/krnlprov/win32-processstarttrace>

### 6.6 昇格とIPC

昇格helperはmachine-scopeの保護されたinstall先に置き、通常userが置換可能なpath、profile指定path、temporary directoryから起動しない。over-the-shoulder UACでは昇格先が別accountになり得るため、HKCU変更をhelperへ渡さない。

named pipeはhelperをserverとし、一回限りのrandom endpointを使う。explicit DACL、remote client拒否、client/server PID取得、image path、publisher/signature、token user/integrity、nonce、transaction ID、deadline、message sizeを双方で検証する。requestはAction ID、Action version、型付きparameter、opaque transaction IDのみで、registry path、command line、backup raw bytesを受け取らない。

over-the-shoulder UACで別admin accountになる場合のexact DACL/SDDLとmandatory integrity labelは未確定である。Task 2の同一admin・別admin・multi-session攻撃testに合格するまでadmin Actionを出荷しない。

privileged ActionのbackupはhelperがOSから取得し、standard userが変更できないmachine-scope storeへdurable commitする。core側DBはopaque backup IDと非機密summaryだけを持つ。rollbackもAction/transaction/backup IDで要求し、coreから「元値」を注入できないようにする。

- Named pipe security: <https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights>
- `GetNamedPipeClientProcessId`: <https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid>
- `GetNamedPipeServerProcessId`: <https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeserverprocessid>
- UAC execution levels: <https://learn.microsoft.com/en-us/windows/win32/sbscs/application-manifests#requestedexecutionlevel>
- Shellによるapplication起動と昇格: <https://learn.microsoft.com/en-us/windows/win32/shell/launch>

Task 2ではnamed pipe案を攻撃testし、必要なら`ncalrpc`を比較spikeする。比較で実装を二重常設しない。

## 7. Windows Update後の処理

1. 起動時fingerprintが変化したらactive automationを開始しない。
2. 未復元journalを旧decoderでreconcile/rollbackする。
3. OS support、known issue kill switch、catalog署名とrollback decoderを確認する。
4. 全Actionを副作用なしでprobeし、resource mappingが変わっていないか検証する。
5. detect-only smoke testを行う。
6. mutable Actionはbuild/revisionごとの自動test evidenceと実機承認があるものだけ再有効化する。
7. 失敗Actionだけをquarantineし、他Actionと原因を切り分けて表示する。

Windows release healthの最新buildを自動的に「安全」と解釈しない。release情報は候補buildを知る入力であり、PCカスタム固有のapply/rollback合格を代替しない。

## 8. Test matrix

最低限、次の軸を組み合わせる。全組合せを総当たりできない場合も、Actionのriskと変更対象に基づくpairwise matrixと代表実機を定義し、未試験cellを表示する。

- OS: 24H2 latest supported revision、25H2 latest supported revision、26H1代表機
- edition: Home、Pro。Enterprise policy-managedは少なくともnegative test
- architecture: x64、Arm64
- privilege: standard user、admin user、over-the-shoulder UAC、UAC拒否
- hardware: desktop/laptop、AC/DC、single/multi display、HDR/SDR、DRR、dock、複数audio endpoint
- lifecycle: clean install、monthly update直後、feature update後、app update/rollback後
- failure: API error、access denied、DB write failure、helper crash、core kill、power loss、Explorer restart、driver reset

Actionごとの合格条件はdetect、backup、apply、verify、rollback、verifyRolledBack、外部変更競合、再起動後recoveryの全てである。画面上の見た目だけで合格にしない。

## 9. 未検証・要CC確認

1. Settings status資料に記載されたExplorer、taskbar、theme、Game Modeのregistry値を、第三者desktop appが書くことのsupportability。
2. `WM_SETTINGCHANGE` / `SHChangeNotify`後の反映差と、Explorer再起動なしで済むbuild別境界。
3. power mode set APIのstandard-user可否、effective mode、OEM utility/policyとの競合。
4. 26H1実機とArm64での全Action、WebView2、helper、display/audio API。
5. HDR/DRR/multi-GPU/dock構成での状態取得とlossless復元。
6. default audio endpointを変更する公開・supported setterの有無。現時点では未確認なので自動変更しない。
7. Focus Assist/Do Not Disturb全体の公開・supported setterの有無。現時点ではguidedのみ。
8. startup inventoryの網羅率と、Task Manager表示との差。`StartupApproved`直接変更は採用しない。
9. named pipe peer検証、PID再利用、token切替、reparse/署名TOCTOUへの攻撃test。
10. monthly revision取得をUBRに補助依存する範囲と、取得不能時のsupport運用。

これらは不明点を隠す一覧ではなく、自動適用を開ける前のgateである。未解決でもread-only検出、公式Settings案内、安全基盤の実装は進められる。

## 10. その他の一次情報

- Windows Settings URI: <https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings-app>
- Windows settings status — common: <https://learn.microsoft.com/en-us/windows/apps/develop/settings/settings-common>
- Windows settings status — Windows 11: <https://learn.microsoft.com/en-us/windows/apps/develop/settings/settings-windows-11>
- `WM_SETTINGCHANGE`: <https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange>
- `SHChangeNotify`: <https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shchangenotify>
- `SetThreadExecutionState`: <https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setthreadexecutionstate>
- WinGet: <https://learn.microsoft.com/en-us/windows/package-manager/winget/>
- Run/RunOnce: <https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys>
- `Win32_StartupCommand`: <https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-startupcommand>
- Task Scheduler: <https://learn.microsoft.com/en-us/windows/win32/taskschd/task-scheduler-start-page>
- Application Recovery and Restart: <https://learn.microsoft.com/en-us/windows/win32/recovery/application-recovery-and-restart-portal>
