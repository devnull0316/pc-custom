import { useEffect, useMemo, useState } from "react";

import { configSnapshotExport, publicErrorMessage, setupCatalog, setupInstall } from "../backend";
import type {
  ActionPresentation,
  BootstrapStatus,
  DataMode,
  InstallOutcome,
  SetupAppDto,
} from "../model";
import { Icon } from "./Icon";
import { OffscreenWindowRescuePanel } from "./OffscreenWindowRescuePanel";
import { PowerToysPanel } from "./PowerToysPanel";
import { SetupEssentialsPanel } from "./SetupEssentialsPanel";

interface SetupViewProps {
  bootstrap: BootstrapStatus | null;
  dataMode: DataMode;
  powerToysAction: ActionPresentation | undefined;
  powerToysDetecting: boolean;
  powerToysLaunching: boolean;
  onPowerToysDetect: () => void;
  onPowerToysLaunch: () => void;
  actions: readonly ActionPresentation[];
  detectionPendingId: string | null;
  previewPendingId: string | null;
  onDetect: (actionId: string) => void;
  onPreview: (action: ActionPresentation) => void;
  onError: (error: unknown) => void;
  onNotice: (message: string) => void;
}

const CATEGORY_LABELS: Readonly<Record<string, string>> = {
  browser: "ブラウザー",
  game: "ゲーム",
  work: "作業・学習",
};

export function SetupView({
  bootstrap,
  dataMode,
  powerToysAction,
  powerToysDetecting,
  powerToysLaunching,
  onPowerToysDetect,
  onPowerToysLaunch,
  actions,
  detectionPendingId,
  previewPendingId,
  onDetect,
  onPreview,
  onError,
  onNotice,
}: SetupViewProps) {
  const live = dataMode === "live";
  const [apps, setApps] = useState<readonly SetupAppDto[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [pendingId, setPendingId] = useState<string | null>(null);
  const [results, setResults] = useState<Readonly<Record<string, InstallOutcome>>>({});
  const [message, setMessage] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<string | null>(null);
  const [snapshotBusy, setSnapshotBusy] = useState(false);

  async function captureSnapshot() {
    if (!live) return;
    setSnapshotBusy(true);
    setMessage(null);
    try {
      setSnapshot(await configSnapshotExport());
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
    } finally {
      setSnapshotBusy(false);
    }
  }

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const catalog = await setupCatalog();
        if (!cancelled) setApps(catalog);
      } catch (error: unknown) {
        if (!cancelled) setLoadError(publicErrorMessage(error));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const grouped = useMemo(() => {
    const map = new Map<string, SetupAppDto[]>();
    for (const app of apps) {
      const list = map.get(app.category) ?? [];
      list.push(app);
      map.set(app.category, list);
    }
    return [...map.entries()];
  }, [apps]);

  async function install(app: SetupAppDto) {
    if (!live) return;
    setPendingId(app.id);
    setMessage(null);
    try {
      const outcome = await setupInstall(app.id);
      setResults((current) => ({ ...current, [app.id]: outcome }));
      if (app.id === "powertoys" && outcome.succeeded) onPowerToysDetect();
      setMessage(
        outcome.succeeded
          ? `${outcome.appName} の導入処理が完了しました。`
          : `${outcome.appName} の導入に失敗しました。`,
      );
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
    } finally {
      setPendingId(null);
    }
  }

  // 1画面に「Windowsの仕上げ」「普段使いの機能」「アプリ導入」を全部並べていたため、
  // 2400文字・79個の箱が同時に見えていた。目的を1つずつに切る。
  const [tab, setTab] = useState<"essentials" | "everyday" | "apps">("essentials");

  function showPowerToysInstall() {
    setTab("apps");
    window.setTimeout(() => {
      document.getElementById("setup-app-powertoys")?.scrollIntoView({ behavior: "smooth", block: "center" });
    }, 60);
  }

  return (
    <section className="setup-view">
      <header className="view-header">
        <span className="eyebrow">新しいPCをセットアップ</span>
        <h1>新しいPCの準備をまとめる</h1>
        <p>
          Windowsの仕上げ、普段使いの機能、よく使うアプリの導入を、
          それぞれ内容を確認しながら進めます。
        </p>
      </header>

      <div aria-label="セットアップの内容" className="segmented" role="tablist">
        {([
          ["essentials", "Windowsの仕上げ"],
          ["everyday", "普段使いの機能"],
          ["apps", "アプリを入れる"],
        ] as const).map(([id, label]) => (
          <button
            aria-selected={tab === id}
            className="segmented__item"
            key={id}
            onClick={() => setTab(id)}
            role="tab"
            type="button"
          >
            {label}
          </button>
        ))}
      </div>

      {tab !== "essentials" ? null : (
      <>
        <SetupEssentialsPanel
          audioAction={actions.find((action) => action.id === "setup.audio_output")}
          bootstrap={bootstrap}
          dataMode={dataMode}
          defaultAppsAction={actions.find((action) => action.id === "setup.default_apps")}
          detectingId={detectionPendingId}
          onDetect={onDetect}
          onError={onError}
          onNotice={onNotice}
          onPreview={onPreview}
          previewingId={previewPendingId}
          windowLayoutAction={actions.find((action) => action.id === "setup.window_layout")}
        />
        <OffscreenWindowRescuePanel
          bootstrap={bootstrap}
          dataMode={dataMode}
          onError={onError}
          onNotice={onNotice}
        />
      </>
      )}

      {tab !== "everyday" ? null : (
      <PowerToysPanel
        action={powerToysAction}
        dataMode={dataMode}
        detecting={powerToysDetecting}
        launching={powerToysLaunching}
        onDetect={onPowerToysDetect}
        onLaunch={onPowerToysLaunch}
        onShowInstall={showPowerToysInstall}
      />
      )}

      {tab !== "apps" ? null : (
      <>
      <section aria-labelledby="setup-apps-title" className="setup-apps">
        <header className="setup-apps__header">
          <h2 id="setup-apps-title">よく使うアプリをまとめて入れる</h2>
          <p>
            Microsoft公式の WinGet を使い、<strong>ここに載っている既知のアプリだけ</strong>を導入します。
            任意のアプリIDやコマンドは受け付けません。導入するアプリを選んでから実行します。
          </p>
        </header>

        {live ? null : (
          <div className="inline-note" role="note">
            <Icon name="info" />
            <span>閲覧モードです。安全コアに接続すると、アプリの導入ができます。</span>
          </div>
        )}
        {loadError === null ? null : (
          <div className="inline-note" role="note">
            <Icon name="warning" />
            <span>{loadError}</span>
          </div>
        )}

        {grouped.map(([category, list]) => (
          <div className="setup-group" key={category}>
            <h3>{CATEGORY_LABELS[category] ?? category}</h3>
            <ul className="setup-list">
              {list.map((app) => {
                const outcome = results[app.id];
                return (
                  <li className="setup-card" id={`setup-app-${app.id}`} key={app.id}>
                    <div className="setup-card__body">
                      <strong>{app.name}</strong>
                      <small>{app.description}</small>
                      {outcome === undefined ? null : (
                        <span className={`setup-card__result setup-card__result--${outcome.succeeded ? "ok" : "fail"}`}>
                          <Icon name={outcome.succeeded ? "check" : "warning"} size={14} />
                          {outcome.summary}
                        </span>
                      )}
                    </div>
                    <button
                      className="secondary-button"
                      disabled={!live || pendingId !== null || outcome?.succeeded === true}
                      onClick={() => void install(app)}
                      type="button"
                    >
                      {pendingId === app.id ? <Icon className="spin" name="spinner" /> : <Icon name="plus" />}
                      {outcome?.succeeded === true ? "導入済み" : "導入する"}
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
      </section>

      <div className="setup-group">
        <h2>今の設定を控えておく</h2>
        <p className="muted small">
          いま検出できている設定の状態を、読み取り専用の控えとして書き出します。Windowsは変更しません。
          新しいPCへ移すときの参照や、変更前後の記録に使えます。
        </p>
        <div className="config-io__row">
          <button
            className="secondary-button"
            disabled={!live || snapshotBusy}
            onClick={() => void captureSnapshot()}
            type="button"
          >
            {snapshotBusy ? <Icon className="spin" name="spinner" /> : <Icon name="arrow" />}
            今の設定を控える
          </button>
          {snapshot === null ? null : (
            <textarea
              aria-label="現在設定の控え（JSON）"
              className="config-io__text"
              readOnly
              rows={5}
              value={snapshot}
            />
          )}
        </div>
      </div>

      <p className="muted small">
        導入はMicrosoftのWinGetカタログ経由で、ユーザー範囲で行います。一部アプリは管理者確認が出る場合があります。
      </p>
      </>
      )}

      {message === null ? null : <p className="setup-view__message" role="status">{message}</p>}
    </section>
  );
}
