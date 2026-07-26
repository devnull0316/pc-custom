# A/B 常駐メモリ計測手順

## 目的

候補A（Tauri 2）と比較fixture Bを、同じWindows・同じ監視負荷・同じ待機時間で測る。採用前に「Tauriなら軽い」「Electronは何MB」と断定せず、プロセスツリー全体の **Private Working Set** の中央値とp95を判断材料にする。

本スクリプトは対象プロセスを起動・終了しない。CCがrelease artifactを起動して明示的にroot PIDを渡すため、任意コマンド実行や実行ファイルpathの組み立てはない。

## 前提

- Windows 11の同一実機、同一OS build/revision、同一電源モードを使う。
- Windows PowerShell 5.1以上とCIM/Performance Counter providerが利用可能であること。
- A/Bともrelease build、DevTools無効、同一React bundle、同一process watcher周期・補正周期にする。
- debugger、profiler、IDE、installer、updaterを接続しない。
- 計測中はWindows Update、ウイルススキャン、ブラウザ操作、ゲーム起動を避ける。停止させたsecurity機能を計測条件にしてはならない。
- 管理者PowerShellは不要。標準ユーザーの通常PowerShellで行う。
- Bのnative watcher比較では、Electronを完全終了し、同等の監視を行うnative watcherだけをroot PIDとして渡す。

## 比較する状態

1. `ui-open`: UIを表示し、初期描画・Action検出が完了したアイドル状態。rootと全子孫processを合算する。
2. `ui-closed-native-core`: ウィンドウ/WebViewをhideではなく破棄した後、tray・journal recovery・process watcherを担うnative coreだけが残る状態。

`ui-closed-native-core`では子孫processが1件でも観測されたrunを無効とし、スクリプトはexit code 2を返す。候補BのElectron main常駐案は任意の参考測定に留め、native-core-only比較には`B-Native-watcher`を使う。

## 1 runの正確な手順

1. 対象artifactを通常権限で起動する。
2. 対象状態へ移し、Action適用、タイムライン操作、画面操作を止める。
3. scriptのwarmupとは別に、状態遷移完了を目視確認する。
4. root processを名前から一意に選び、PIDを固定する。複数候補がある場合は自動選択せず、Task Manager等で本人性を確認する。
5. 次のコマンドを実行する。

候補A、UI表示中:

```powershell
$rootProcess = Get-Process -Name 'pc-custom' | Sort-Object StartTime -Descending | Select-Object -First 1
powershell.exe -NoProfile -File .\scripts\measure-private-working-set.ps1 -Variant A-Tauri -Scenario ui-open -RootProcessId $rootProcess.Id -WarmupSeconds 60 -SampleSeconds 120 -IntervalMilliseconds 1000
```

候補A、UIを破棄したnative coreのみ:

```powershell
$rootProcess = Get-Process -Name 'pc-custom' | Sort-Object StartTime -Descending | Select-Object -First 1
powershell.exe -NoProfile -File .\scripts\measure-private-working-set.ps1 -Variant A-Tauri -Scenario ui-closed-native-core -RootProcessId $rootProcess.Id -WarmupSeconds 60 -SampleSeconds 120 -IntervalMilliseconds 1000
```

候補B、Electron UI表示中（fixtureのprocess名はCCが実artifactに合わせて置き換える）:

```powershell
$rootProcess = Get-Process -Name 'pc-custom-electron-fixture' | Sort-Object StartTime -Descending | Select-Object -First 1
powershell.exe -NoProfile -File .\scripts\measure-private-working-set.ps1 -Variant B-Electron-main -Scenario ui-open -RootProcessId $rootProcess.Id -WarmupSeconds 60 -SampleSeconds 120 -IntervalMilliseconds 1000
```

候補B、Electronを完全終了したnative watcherのみ:

```powershell
$rootProcess = Get-Process -Name 'pc-custom-native-watcher-fixture' | Sort-Object StartTime -Descending | Select-Object -First 1
powershell.exe -NoProfile -File .\scripts\measure-private-working-set.ps1 -Variant B-Native-watcher -Scenario ui-closed-native-core -RootProcessId $rootProcess.Id -WarmupSeconds 60 -SampleSeconds 120 -IntervalMilliseconds 1000
```

process名が一致しない、または`$rootProcess`が空の場合は測定を始めない。PIDを手入力する場合もTask Manager等で対象を確認する。

## 反復と順序

- 各variant × stateを最低10 run測る。
- A→Bだけに偏らせず、A/Bの順を交互または乱数で入れ替える。
- 1 runごとに対象artifactを通常終了し、未復元transactionがないことを確認してから再起動する。
- OS再起動を挟む場合はA/Bの両方へ同じ頻度で挟む。
- watcher負荷確認用に、登録対象processが無い待機状態と、同一fixture processを1件監視中の状態を別seriesとして記録する。

## 出力

既定では`artifacts/memory/`へ次を出力する。

- `*_raw.csv`: 1 intervalごとのprocess tree合算値。
- `*_summary.json`: Private Working Set、Working Set、process/handle/thread数、CPUのminimum/median/p95/maximum/average。

スクリプトはusername、machine name、process command line、実行ファイルpathを収集しない。summaryの`ui_closed_native_core_gate.passed`が`false`、exit codeが0以外、root processが途中終了したrunは比較から除外し、失敗自体は消さず記録する。

## 判定

各runの`private_working_set_bytes.median`を主指標、同p95を安定性指標とする。10 runの中央値とp95をさらに集計し、CPU、handle/thread増加、watcher検知遅延も併記する。平均値だけ、最小値だけ、単一runだけで採否を決めない。

数値予算とA/B再採点はCCが同一実機の結果を確認して確定する。このリポジトリには未実測値を成功値として記入しない。
