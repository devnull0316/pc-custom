import { useEffect, useState } from "react";

import { publicErrorMessage, setTaskbarAutoHide, taskbarAutoHideState } from "../backend";
import type { DataMode, TaskbarAutoHideState } from "../model";
import { Icon } from "./Icon";

interface TaskbarAutoHidePanelProps {
  dataMode: DataMode;
}

/**
 * 最大化しているときだけタスクバーを隠す。
 *
 * 既定は切。入れたときだけ動く。切ると、隠していた場合はタスクバーを戻す。
 * 「オンにしてオフにしても戻らない」を作らないため、戻す責任はここではなく監視側にある。
 *
 * 誰かが Windows の設定から自動的に隠すを触ったら、この機能は手を引く。
 * 取り返さない。取り返すと、利用者と機械が同じ設定を取り合うことになる。
 */
export function TaskbarAutoHidePanel({ dataMode }: TaskbarAutoHidePanelProps) {
  const live = dataMode === "live";
  const [state, setState] = useState<TaskbarAutoHideState | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!live) return;
    let cancelled = false;
    taskbarAutoHideState()
      .then((result) => {
        if (!cancelled) setState(result);
      })
      .catch((cause: unknown) => {
        if (!cancelled) setError(publicErrorMessage(cause));
      });
    return () => {
      cancelled = true;
    };
  }, [live]);

  async function toggle(next: boolean) {
    setBusy(true);
    setError(null);
    try {
      setState(await setTaskbarAutoHide(next));
    } catch (cause) {
      setError(publicErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  }

  if (!live) return null;
  if (state === null) {
    return error === null ? null : (
      <section aria-label="最大化中のタスクバー" className="taskbar-autohide-panel">
        <p className="taskbar-autohide-panel__error" role="alert">
          <Icon name="warning" size={14} />
          {error}
        </p>
      </section>
    );
  }

  return (
    <section aria-label="最大化中のタスクバー" className="taskbar-autohide-panel">
      <header>
        <div>
          <strong>最大化しているときだけタスクバーを隠す</strong>
          <p>
            ウィンドウを最大化している間だけタスクバーを隠し、元に戻すと出します。
            切り替わるまで数秒かかります。
          </p>
        </div>
        <button
          aria-pressed={state.enabled}
          className={state.enabled ? "primary-button" : "secondary-button"}
          disabled={busy}
          onClick={() => toggle(!state.enabled)}
          type="button"
        >
          {busy ? <Icon className="spin" name="spinner" size={14} /> : null}
          {state.enabled ? "使うのをやめる" : "使う"}
        </button>
      </header>
      <ul className="taskbar-autohide-panel__notes">
        <li>全画面のゲームや、画面いっぱいのウィンドウのときは何もしません。</li>
        <li>Windowsの設定から自動的に隠すを手で変えると、この機能は手を引きます。</li>
        <li>使うのをやめると、この機能が隠す前の状態へ戻します。</li>
        <li>もともと常に隠す設定にしている場合は、何もしません。</li>
      </ul>
      {state.notApplicable ? (
        <p className="taskbar-autohide-panel__released">
          <Icon name="info" size={14} />
          もともとタスクバーを常に隠す設定になっているため、この機能の出番がありません。設定は変更していません。
        </p>
      ) : null}
      {state.released ? (
        <p className="taskbar-autohide-panel__released">
          <Icon name="info" size={14} />
          自動的に隠す設定が外から変わったため、この機能は手を引いています。いったん「使うのをやめる」を押してから、もう一度「使う」を押すと再開します。
        </p>
      ) : null}
      {state.lastError === null ? null : (
        <p className="taskbar-autohide-panel__error" role="alert">
          <Icon name="warning" size={14} />
          {state.lastError}
        </p>
      )}
      {error === null ? null : (
        <p className="taskbar-autohide-panel__error" role="alert">
          <Icon name="warning" size={14} />
          {error}
        </p>
      )}
    </section>
  );
}
