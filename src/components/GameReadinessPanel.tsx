import { useState } from "react";

import { detectAction, publicErrorMessage } from "../backend";
import type { ActionState, DataMode } from "../model";
import { Icon } from "./Icon";

interface GameReadinessPanelProps {
  dataMode: DataMode;
}

/**
 * ゲーム前の準備確認（BRIEF §4）。Steam や Playnite の「起動前に環境を確かめる」体験を、
 * Windows 側の状態確認として持ち込んだもの。何も変更せず、読み取った値を並べるだけ。
 */
export function GameReadinessPanel({ dataMode }: GameReadinessPanelProps) {
  const live = dataMode === "live";
  const [state, setState] = useState<ActionState | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  async function check() {
    setBusy(true);
    setMessage(null);
    try {
      const response = await detectAction("games.readiness_check");
      setState(response.state);
    } catch (error: unknown) {
      setState(null);
      setMessage(publicErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  const items = state?.items ?? [];

  return (
    <section className="readiness-panel" aria-label="ゲーム前の準備確認">
      <div className="readiness-panel__head">
        <div>
          <h2>ゲーム前の準備を確かめる</h2>
          <p className="muted small">
            画面のリフレッシュレート、HDR、電源、空き容量、音声の出力先などを、
            <strong>何も変更せずに</strong>読み取って並べます。速くする機能ではなく、
            設定の取りこぼしに気づくためのものです。
          </p>
        </div>
        <button className="secondary-button" disabled={!live || busy} onClick={() => void check()} type="button">
          {busy ? <Icon className="spin" name="spinner" /> : <Icon name="search" />}
          いまの状態を確認
        </button>
      </div>

      {state === null ? null : (
        <>
          <p className="readiness-panel__summary">{state.label}</p>
          <ul className="readiness-panel__list">
            {items.map((item) => {
              const unknown = item.includes("不明") || item.includes("未設定");
              return (
                <li className={unknown ? "readiness-panel__row--unknown" : undefined} key={item}>
                  <Icon name={unknown ? "info" : "check"} size={15} />
                  <span>{item}</span>
                </li>
              );
            })}
          </ul>
          {items.length === 0 ? <p className="muted small">{state.detail}</p> : null}
        </>
      )}

      {message === null ? null : <p className="readiness-panel__message" role="status">{message}</p>}
      {live ? null : <p className="muted small">閲覧モードです。安全コアに接続すると確認できます。</p>}
    </section>
  );
}
