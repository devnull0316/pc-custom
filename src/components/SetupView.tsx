import { useEffect, useMemo, useState } from "react";

import { publicErrorMessage, setupCatalog, setupInstall } from "../backend";
import type { DataMode, InstallOutcome, SetupAppDto } from "../model";
import { Icon } from "./Icon";

interface SetupViewProps {
  dataMode: DataMode;
}

const CATEGORY_LABELS: Readonly<Record<string, string>> = {
  browser: "ブラウザー",
  game: "ゲーム",
  work: "作業・学習",
};

export function SetupView({ dataMode }: SetupViewProps) {
  const live = dataMode === "live";
  const [apps, setApps] = useState<readonly SetupAppDto[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [pendingId, setPendingId] = useState<string | null>(null);
  const [results, setResults] = useState<Readonly<Record<string, InstallOutcome>>>({});
  const [message, setMessage] = useState<string | null>(null);

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

  return (
    <section className="setup-view">
      <header className="view-header">
        <span className="eyebrow">新しいPCをセットアップ</span>
        <h1>よく使うアプリをまとめて入れる</h1>
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
          <h2>{CATEGORY_LABELS[category] ?? category}</h2>
          <ul className="setup-list">
            {list.map((app) => {
              const outcome = results[app.id];
              return (
                <li className="setup-card" key={app.id}>
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

      {message === null ? null : <p className="setup-view__message" role="status">{message}</p>}
      <p className="muted small">
        導入はMicrosoftのWinGetカタログ経由で、ユーザー範囲で行います。一部アプリは管理者確認が出る場合があります。
      </p>
    </section>
  );
}
