import { useState } from "react";

import { publicErrorMessage, restartExplorerShell } from "../backend";
import type { DataMode, ShellRestartOutcome } from "../model";
import { Icon } from "./Icon";

interface ExplorerRestartPanelProps {
  dataMode: DataMode;
}

/**
 * 一部の設定は、レジストリへ書いただけでは画面が変わらない。
 * エクスプローラー(シェル)を再起動して初めて反映される。
 *
 * 再起動は必ずここから、利用者が押したときだけ行う。適用処理が勝手に走らせることはない。
 * 何が起きるかを先に全部書いておく。押してから驚かせない。
 */
export function ExplorerRestartPanel({ dataMode }: ExplorerRestartPanelProps) {
  const live = dataMode === "live";
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<ShellRestartOutcome | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  async function restart() {
    if (!live) return;
    setBusy(true);
    setMessage(null);
    try {
      const result = await restartExplorerShell();
      setOutcome(result);
      setMessage(
        result.shellReturned
          ? "エクスプローラーを再起動しました。設定が画面に反映されているか確認してください。"
          : "再起動を試みましたが、タスクバーの復帰を確認できませんでした。サインインし直すと戻ります。",
      );
    } catch (error: unknown) {
      // 失敗をそのまま出して終わらない。**次に何をすればいいかを書く。**
      // ここで止まると、タスクバーが戻らないまま利用者が取り残される。
      setMessage(
        `${publicErrorMessage(error)} サインインし直すとタスクバーは戻ります。`
        + "それでも戻らない場合は、PCを再起動してください。",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="restart-panel">
      <header className="restart-panel__head">
        <Icon name="warning" size={18} />
        <div>
          <strong>この変更は、エクスプローラーを再起動すると画面に出ます</strong>
          <p>
            タスクバーやエクスプローラーの見た目に関わる設定は、値を書き換えただけでは
            画面が変わりません。Windows が起動時に読み込んだ内容を使い続けるためです。
          </p>
        </div>
      </header>

      <div className="restart-panel__facts">
        <p><strong>再起動すると、こうなります。</strong></p>
        <ul>
          <li>開いているフォルダーの窓が<strong>すべて閉じます</strong>。保存していない作業はありません（フォルダーの窓は文書を持たないため）が、開き直しは必要です。</li>
          <li>タスクバーとデスクトップのアイコンが<strong>数秒だけ消えて</strong>、戻ります。</li>
          <li>起動中のアプリは終了しません。エクスプローラー以外のプログラムには触れません。</li>
        </ul>
        <p className="restart-panel__caution">
          <Icon name="warning" size={14} />
          <span>
            <strong>全画面のゲーム中は押さないでください。</strong>
            タスクバーが作り直される際にフォーカスが動くため、全画面のゲームが
            ウィンドウ表示に落ちたり最小化されたりすることがあります。
            ゲームを終えてからにしてください。
          </span>
        </p>
      </div>

      <div className="restart-panel__actions">
        <button
          className="secondary-button"
          disabled={!live || busy}
          onClick={() => void restart()}
          type="button"
        >
          {busy ? <Icon className="spin" name="spinner" /> : <Icon name="recovery" />}
          エクスプローラーを再起動する
        </button>
        {live ? null : <span className="muted small">安全コアに接続すると実行できます。</span>}
      </div>

      {message === null ? null : (
        <p className="restart-panel__message" role="status">{message}</p>
      )}
      {outcome === null ? null : (
        <p className="muted small">
          終了したエクスプローラー: {outcome.terminated} 件 ／ タスクバー復帰:{" "}
          {outcome.shellReturned ? "確認" : "未確認"}
          {outcome.relaunched ? " ／ Windows の自動復帰では戻らなかったため起動し直しました" : ""}
        </p>
      )}
    </section>
  );
}
