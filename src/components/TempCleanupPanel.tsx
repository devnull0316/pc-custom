import { useState } from "react";

import { publicErrorMessage, tempCleanupApply, tempCleanupPlan } from "../backend";
import { canRunLiveMutation } from "../frontendLogic";
import type { DataMode, TempCleanupOutcome, TempCleanupPlan } from "../model";
import { Icon } from "./Icon";

interface TempCleanupPanelProps {
  dataMode: DataMode;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(1)} ${units[unit]}`;
}

export function TempCleanupPanel({ dataMode }: TempCleanupPanelProps) {
  const live = dataMode === "live";
  const [plan, setPlan] = useState<TempCleanupPlan | null>(null);
  const [outcome, setOutcome] = useState<TempCleanupOutcome | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);

  async function loadPlan() {
    if (!canRunLiveMutation(dataMode)) return;
    setBusy(true);
    setMessage(null);
    setOutcome(null);
    setConfirming(false);
    try {
      setPlan(await tempCleanupPlan());
    } catch (error: unknown) {
      setPlan(null);
      setMessage(publicErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function apply() {
    if (!canRunLiveMutation(dataMode)) return;
    setBusy(true);
    setMessage(null);
    try {
      const result = await tempCleanupApply();
      setOutcome(result);
      setPlan(null);
      setConfirming(false);
      setMessage(
        `${result.deletedCount}件を削除し、${formatBytes(result.freedBytes)}分の空きを作りました。`,
      );
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="temp-cleanup" aria-label="一時ファイルの削除">
      <h2>使われていない一時ファイルを消す</h2>
      <p className="muted small">
        アプリが残した一時ファイルのうち、<strong>7日より古いものだけ</strong>が対象です。
        消す前に必ず一覧を表示します。<strong>この操作だけは元に戻せません。</strong>
        ドキュメントや写真などのユーザーファイルは対象外です。
      </p>

      <div className="config-io__row">
        <button className="secondary-button" disabled={!live || busy} onClick={() => void loadPlan()} type="button">
          {busy && plan === null ? <Icon className="spin" name="spinner" /> : <Icon name="search" />}
          消せるものを調べる
        </button>
        {plan === null || plan.candidates.length === 0 ? null : confirming ? (
          <>
            <button className="primary-button" disabled={!live || busy} onClick={() => void apply()} type="button">
              {busy ? <Icon className="spin" name="spinner" /> : <Icon name="warning" />}
              本当に削除する（戻せません）
            </button>
            <button className="secondary-button" disabled={busy} onClick={() => setConfirming(false)} type="button">
              やめる
            </button>
          </>
        ) : (
          <button className="secondary-button" disabled={!live || busy} onClick={() => setConfirming(true)} type="button">
            <Icon name="close" />この{plan.candidates.length}件を削除する
          </button>
        )}
      </div>

      {plan === null ? null : plan.candidates.length === 0 ? (
        <p className="muted small">消せる一時ファイルは見つかりませんでした。</p>
      ) : (
        <>
          <p className="temp-cleanup__summary">
            {plan.candidates.length}件・合計 {formatBytes(plan.totalBytes)}
            {plan.truncated ? "（多いため一部のみ表示）" : ""}
          </p>
          <ul className="temp-cleanup__list">
            {plan.candidates.slice(0, 50).map((candidate) => (
              <li key={`${candidate.fileName}-${candidate.sizeBytes}-${candidate.ageDays}`}>
                <span className="temp-cleanup__name">{candidate.fileName}</span>
                <span className="muted small">
                  {formatBytes(candidate.sizeBytes)} ・ {candidate.ageDays}日前
                </span>
              </li>
            ))}
          </ul>
          {plan.candidates.length > 50 ? (
            <p className="muted small">ほか {plan.candidates.length - 50} 件</p>
          ) : null}
        </>
      )}

      {outcome === null || outcome.skipped.length === 0 ? null : (
        <ul className="temp-cleanup__list">
          {outcome.skipped.map((skip) => (
            <li key={skip.fileName}>
              <span className="temp-cleanup__name">{skip.fileName}</span>
              <span className="muted small">{skip.reason}</span>
            </li>
          ))}
        </ul>
      )}

      {message === null ? null : <p className="temp-cleanup__message" role="status">{message}</p>}
      {live ? null : <p className="muted small">閲覧モードです。安全コアに接続すると操作できます。</p>}
    </section>
  );
}
