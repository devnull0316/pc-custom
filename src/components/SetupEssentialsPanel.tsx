import { useEffect, useState } from "react";

import {
  getWindowLayoutStatus,
  openWindowsSettings,
  publicErrorMessage,
  saveWindowLayout,
} from "../backend";
import type {
  ActionPresentation,
  BootstrapStatus,
  DataMode,
  WindowLayoutStatus,
} from "../model";
import { riskLabel } from "../model";
import { methodSummaryForScreen } from "../publicCopy";
import { Icon } from "./Icon";

interface SetupEssentialsPanelProps {
  bootstrap: BootstrapStatus | null;
  dataMode: DataMode;
  defaultAppsAction: ActionPresentation | undefined;
  audioAction: ActionPresentation | undefined;
  windowLayoutAction: ActionPresentation | undefined;
  detectingId: string | null;
  previewingId: string | null;
  onDetect: (actionId: string) => void;
  onPreview: (action: ActionPresentation) => void;
  onError: (error: unknown) => void;
  onNotice: (message: string) => void;
}

type LayoutStatusState =
  | { phase: "idle" }
  | { phase: "loading" }
  | { phase: "loaded"; value: WindowLayoutStatus }
  | { phase: "error"; message: string }
  | { phase: "unavailable" };

function savedAtLabel(value: number | null | undefined): string | null {
  if (value === undefined || value === null || !Number.isFinite(value)) return null;
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return null;
  return date.toLocaleString("ja-JP");
}

function updateImpactLabel(impact: ActionPresentation["updateImpact"] | undefined): string {
  if (impact === "low") return "Update影響: 低い";
  if (impact === "review") return "Update後: 再検証";
  if (impact === "high") return "Update影響: 高い";
  return "Update影響: 確認中";
}

function restartLabel(action: ActionPresentation | undefined): string {
  if (action === undefined) return "再起動: 確認中";
  if (action.requiresRestart) return "再起動: OS";
  if (action.requiresExplorerRestart) return "再起動: Explorer";
  return "再起動: 不要";
}

function ActionMetadata({
  action,
  currentLabel,
}: {
  action: ActionPresentation | undefined;
  currentLabel: string;
}) {
  return (
    <>
      <dl
        aria-label={`${action?.name ?? "この項目"}の対象・状態・方法`}
        className="setup-essential-card__contract"
      >
        <div><dt>どんな人向け</dt><dd>{action?.audience ?? "カタログ接続後に表示します"}</dd></div>
        <div><dt>現在の状態</dt><dd>{currentLabel}</dd></div>
        <div><dt>{action?.kind === "guided" ? "案内後" : "適用後の状態"}</dt><dd>{action?.desiredState ?? "カタログ接続後に表示します"}</dd></div>
        <div><dt>{action?.kind === "guided" ? "案内方法" : "変更方法"}</dt><dd>{action === undefined ? "カタログ接続後に表示します" : methodSummaryForScreen(action)}</dd></div>
      </dl>
      <ul
        aria-label={`${action?.name ?? "この項目"}の属性`}
        className="setup-essential-card__metadata"
      >
        <li>危険度: {action === undefined ? "確認中" : riskLabel(action.riskLevel)}</li>
        <li>管理者: {action === undefined ? "確認中" : action.requiresAdmin ? "必要" : "不要"}</li>
        <li>{restartLabel(action)}</li>
        <li>{updateImpactLabel(action?.updateImpact)}</li>
        <li>復元: {action === undefined ? "確認中" : action.reversible ? "元に戻せる" : "変更なし"}</li>
      </ul>
    </>
  );
}

export function SetupEssentialsPanel({
  bootstrap,
  dataMode,
  defaultAppsAction,
  audioAction,
  windowLayoutAction,
  detectingId,
  previewingId,
  onDetect,
  onPreview,
  onError,
  onNotice,
}: SetupEssentialsPanelProps) {
  const live = dataMode === "live";
  const [layoutStatusState, setLayoutStatusState] =
    useState<LayoutStatusState>({ phase: "idle" });
  const [layoutBusy, setLayoutBusy] = useState(false);
  const [layoutMessage, setLayoutMessage] = useState<string | null>(null);
  const [unregisteredGamesClosed, setUnregisteredGamesClosed] = useState(false);
  const restoreAllowed =
    live &&
    bootstrap?.mode === "ready" &&
    windowLayoutAction?.availability === "mutable";

  useEffect(() => {
    if (dataMode === "loading") {
      setLayoutStatusState({ phase: "idle" });
      return;
    }
    if (!live) {
      setLayoutStatusState({ phase: "unavailable" });
      return;
    }
    let cancelled = false;
    setLayoutStatusState({ phase: "loading" });
    void getWindowLayoutStatus()
      .then((status) => {
        if (!cancelled) setLayoutStatusState({ phase: "loaded", value: status });
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setLayoutStatusState({ phase: "error", message: publicErrorMessage(error) });
          onError(error);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [live, onError]);

  async function openSettings(action: ActionPresentation | undefined) {
    if (!live || action === undefined) return;
    try {
      await openWindowsSettings(action.id);
    } catch (error: unknown) {
      onError(error);
    }
  }

  async function saveLayout() {
    if (!restoreAllowed || layoutBusy || !unregisteredGamesClosed) return;
    if (
      layoutStatusState.phase === "loaded" &&
      layoutStatusState.value.saved &&
      !window.confirm(
        "保存済みの配置は1件だけです。現在の配置で上書きしますか？以前の保存内容は復元できません。",
      )
    ) {
      return;
    }
    setLayoutBusy(true);
    setLayoutMessage(null);
    try {
      const status = await saveWindowLayout(unregisteredGamesClosed);
      setLayoutStatusState({ phase: "loaded", value: status });
      setLayoutMessage(
        `${status.windowCount}個の対象ウィンドウの配置を保存しました。` +
          (status.excludedGameWindows > 0
            ? ` ゲーム ${status.excludedGameWindows}個は除外しました。`
            : ""),
      );
      onNotice("現在開いている対象ウィンドウの配置を保存しました。");
      if (windowLayoutAction !== undefined) onDetect(windowLayoutAction.id);
    } catch (error: unknown) {
      const message = publicErrorMessage(error);
      setLayoutMessage(message);
      onError(error);
    } finally {
      setUnregisteredGamesClosed(false);
      setLayoutBusy(false);
    }
  }

  const layoutStatus =
    layoutStatusState.phase === "loaded" ? layoutStatusState.value : null;
  const savedAt = savedAtLabel(layoutStatus?.savedAtUnixMs);
  const audioState = audioAction?.currentState;
  const audioItems = audioState?.items ?? [];
  const layoutPreviewBusy =
    windowLayoutAction !== undefined && previewingId === windowLayoutAction.id;
  let layoutStatusTitle = "保存状態はまだ確認していません";
  let layoutStatusDetail = "安全コアへの接続後に保存状態を確認します。";
  let layoutStatusIcon: "check" | "info" | "warning" = "info";
  if (layoutStatusState.phase === "loading") {
    layoutStatusTitle = "保存状態を確認中";
    layoutStatusDetail = "保存済みの配置があるか確認しています。";
  } else if (layoutStatusState.phase === "unavailable") {
    layoutStatusTitle = "この環境では保存状態を確認できません";
    layoutStatusDetail = "安全コアへ接続すると、保存と復元を利用できます。";
  } else if (layoutStatusState.phase === "error") {
    layoutStatusTitle = "保存状態の確認に失敗しました";
    layoutStatusDetail = layoutStatusState.message;
    layoutStatusIcon = "warning";
  } else if (layoutStatusState.phase === "loaded" && layoutStatusState.value.saved) {
    layoutStatusTitle = `${layoutStatusState.value.windowCount}個のウィンドウ配置を保存済み`;
    layoutStatusDetail =
      (savedAt === null ? "保存日時を確認できません" : `${savedAt} に保存`) +
      (layoutStatusState.value.excludedGameWindows > 0
        ? ` ／ ゲーム除外 ${layoutStatusState.value.excludedGameWindows}個`
        : "") +
      (layoutStatusState.value.skippedWindows > 0
        ? ` ／ その他の除外 ${layoutStatusState.value.skippedWindows}個`
        : "");
    layoutStatusIcon = "check";
  } else if (layoutStatusState.phase === "loaded") {
    layoutStatusTitle = "保存された配置はありません";
    layoutStatusDetail = "保存ボタンを押すまで、ウィンドウ配置は記録しません。";
  }

  return (
    <section aria-labelledby="setup-essentials-title" className="setup-essentials">
      <header className="setup-essentials__header">
        <span className="eyebrow">Windowsの仕上げ</span>
        <h2 id="setup-essentials-title">既定アプリ・音声・ウィンドウ配置</h2>
        <p>
          Windows自身の設定が必要な項目は設定画面へ案内し、PCカスタムで扱える配置だけは
          保存内容を確認してから復元します。
        </p>
      </header>

      <div className="setup-essentials__grid">
        <article className="setup-essential-card">
          <div className="setup-essential-card__copy">
            <span className="setup-essential-card__icon"><Icon name="action" /></span>
            <div>
              <h3>{defaultAppsAction?.name ?? "既定のアプリを選ぶ"}</h3>
              <p>
                Windowsの仕様で、ここから直接は変更できません。ファイルやリンクを開くアプリは
                Windowsの設定で選びます。PCカスタムは、Windowsが既定のアプリを覚えている場所を書き換えず、設定画面だけを開きます。
              </p>
            </div>
          </div>
          <ActionMetadata
            action={defaultAppsAction}
            currentLabel={defaultAppsAction?.currentState?.label ?? "PCカスタムによるOS変更なし"}
          />
          <button
            className="secondary-button"
            disabled={!live || defaultAppsAction === undefined}
            onClick={() => void openSettings(defaultAppsAction)}
            type="button"
          >
            <Icon name="arrow" />既定のアプリ設定を開く
          </button>
        </article>

        <article className="setup-essential-card">
          <div className="setup-essential-card__copy">
            <span className="setup-essential-card__icon"><Icon name="info" /></span>
            <div>
              <h3>{audioAction?.name ?? "音声の出力先を確認する"}</h3>
              <p>{audioState?.detail ?? "接続中の音声出力と、現在の既定デバイスを確認します。"}</p>
            </div>
          </div>
          <ActionMetadata
            action={audioAction}
            currentLabel={audioState?.label ?? "まだ確認していません"}
          />
          <div className="setup-essential-card__state" aria-live="polite">
            <strong>{audioState?.label ?? "まだ確認していません"}</strong>
            {audioItems.length === 0 ? null : (
              <ul aria-label="検出した音声出力">
                {audioItems.map((item, index) => <li key={`${item}-${index}`}>{item}</li>)}
              </ul>
            )}
          </div>
          <div className="setup-essential-card__actions">
            <button
              className="secondary-button"
              disabled={!live || audioAction === undefined || detectingId === audioAction.id}
              onClick={() => {
                if (audioAction !== undefined) onDetect(audioAction.id);
              }}
              type="button"
            >
              {audioAction !== undefined && detectingId === audioAction.id
                ? <Icon className="spin" name="spinner" />
                : <Icon name="search" />}
              音声出力を確認
            </button>
            <button
              className="secondary-button"
              disabled={!live || audioAction === undefined}
              onClick={() => void openSettings(audioAction)}
              type="button"
            >
              <Icon name="arrow" />サウンド設定を開く
            </button>
          </div>
        </article>

        <article className="setup-essential-card setup-essential-card--wide">
          <div className="setup-essential-card__copy">
            <span className="setup-essential-card__icon"><Icon name="appearance" /></span>
            <div>
              <h3>{windowLayoutAction?.name ?? "現在のウィンドウ配置を保存する"}</h3>
              <p>
                PCカスタムは<strong>今開いている対象ウィンドウの位置と表示状態だけ</strong>を保存・復元します。
                アプリは起動しません。登録済みゲーム、全画面表示、標準的でないウィンドウは対象外です。
              </p>
            </div>
          </div>
          <ActionMetadata
            action={windowLayoutAction}
            currentLabel={layoutStatusTitle}
          />

          <div className="window-layout-status" aria-live="polite">
            <span
              className={`window-layout-status__mark${
                layoutStatusState.phase === "error" ? " window-layout-status__mark--error" : ""
              }`}
            >
              <Icon name={layoutStatusIcon} />
            </span>
            <div>
              <strong>{layoutStatusTitle}</strong>
              <small>{layoutStatusDetail}</small>
            </div>
          </div>

          <div className="window-layout-boundary" role="note">
            <Icon name="warning" size={16} />
            <p>
              登録済みゲームは実行ファイルIDで除外します。ただし公開APIだけでは、
              未登録のウィンドウゲームを一般のアプリから確実に識別できません。
              保存前に未登録ゲームを閉じてください。
            </p>
          </div>
          <label className="window-layout-acknowledgement">
            <input
              checked={unregisteredGamesClosed}
              disabled={!restoreAllowed || layoutBusy}
              onChange={(event) => setUnregisteredGamesClosed(event.target.checked)}
              type="checkbox"
            />
            <span>未登録のウィンドウゲームを閉じたことを確認しました</span>
          </label>

          <div className="setup-essential-card__actions">
            <button
              className="secondary-button"
              disabled={
                !restoreAllowed ||
                !unregisteredGamesClosed ||
                layoutBusy ||
                layoutPreviewBusy
              }
              onClick={() => void saveLayout()}
              type="button"
            >
              {layoutBusy ? <Icon className="spin" name="spinner" /> : <Icon name="plus" />}
              {layoutStatus?.saved === true ? "現在の配置で上書き保存" : "現在の配置を保存"}
            </button>
            <button
              className="primary-button"
              disabled={
                !restoreAllowed ||
                layoutBusy ||
                layoutStatus?.saved !== true ||
                windowLayoutAction === undefined ||
                layoutPreviewBusy
              }
              onClick={() => {
                if (windowLayoutAction !== undefined) onPreview(windowLayoutAction);
              }}
              type="button"
            >
              {layoutPreviewBusy
                ? <Icon className="spin" name="spinner" />
                : <Icon name="undo" />}
              復元内容を確認
            </button>
          </div>
          {layoutMessage === null ? null : <p className="setup-essential-card__message" role="status">{layoutMessage}</p>}
          <p className="muted small">
            復元時に閉じている、または同じ名前の候補が複数あるウィンドウは勝手に操作せず、対象として報告します。
          </p>
        </article>
      </div>
    </section>
  );
}
