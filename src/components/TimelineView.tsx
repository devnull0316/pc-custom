import { useState } from "react";

import type { BootstrapStatus, DataMode, TimelineItem } from "../model";
import { timelineStatusLabel } from "../model";
import { Icon } from "./Icon";

interface TimelineViewProps {
  dataMode: DataMode;
  bootstrap: BootstrapStatus | null;
  items: readonly TimelineItem[];
  rollbackPendingId: string | null;
  recoveryBusy: boolean;
  onRequestRollback: (item: TimelineItem) => void;
  onRetryRecovery: () => void;
  onOpenActions: () => void;
}

const FORMATTER = new Intl.DateTimeFormat("ja-JP", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });
function formatDate(value: string): string { const date = new Date(value); return Number.isNaN(date.getTime()) ? "時刻不明" : FORMATTER.format(date); }

export function TimelineView({ dataMode, bootstrap, items, rollbackPendingId, recoveryBusy, onRequestRollback, onRetryRecovery, onOpenActions }: TimelineViewProps) {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const canRollback = dataMode === "live" && bootstrap?.mode === "ready";
  return (
    <div className="view timeline-view">
      <header className="view-heading"><div><p className="eyebrow">変更の記録</p><h1>タイムライン</h1><p>変更前後と検証結果を残し、戻したい項目だけを選べます。</p></div></header>
      {bootstrap?.mode === "recovery_required" && bootstrap.recoveryCount > 0 ? (
        <section aria-label="復旧が必要な項目" className="timeline-recovery"><Icon name="warning" size={22} /><div><strong>{bootstrap.recoveryCount}件の未復元項目があります</strong><p>{bootstrap.message}</p></div><button disabled={recoveryBusy || dataMode !== "live"} onClick={onRetryRecovery} type="button">{recoveryBusy ? <Icon className="spin" name="spinner" /> : <Icon name="recovery" />}失敗項目だけ再試行</button></section>
      ) : null}
      {dataMode === "loading" ? (
        <div aria-label="タイムラインを読み込み中" className="timeline-loading">{[0, 1, 2].map((item) => <div className="timeline-skeleton" key={item}><span /><span /><span /></div>)}</div>
      ) : items.length === 0 ? (
        <div className="empty-state"><span className="empty-state__icon"><Icon name="timeline" size={28} /></span><h2>{dataMode === "catalog" ? "履歴へ接続できません" : "まだ変更はありません"}</h2><p>{dataMode === "catalog" ? "静的カタログは閲覧できますが、実際の変更履歴は安全コアへの接続が必要です。" : "Actionを適用すると、変更前後と検証結果がここに記録されます。"}</p><button className="secondary-button" onClick={onOpenActions} type="button">Actionを見る</button></div>
      ) : (
        <ol className="timeline-list">
          {items.map((item) => {
            const expanded = expandedId === item.itemId;
            const pending = rollbackPendingId === item.itemId;
            const alert = item.status === "recovery_required" || item.status === "rollback_failed";
            return (
              <li className="timeline-entry" key={item.itemId}>
                <div className={`timeline-marker timeline-marker--${item.status}`}><Icon name={alert ? "warning" : "check"} size={15} /></div>
                <article>
                  <header className="timeline-entry__header"><div><time dateTime={item.startedAt}>{formatDate(item.startedAt)}</time><h2>{item.title}</h2><p>{item.summary}</p></div><span className={`status-pill status-pill--${item.status}`}>{timelineStatusLabel(item.status)}</span></header>
                  <div className="timeline-state-change"><span><small>変更前</small><strong>{item.before}</strong></span><Icon name="arrow" /><span><small>変更後</small><strong>{item.after}</strong></span></div>
                  {expanded ? (
                    <div className="timeline-details" id={`timeline-details-${item.itemId}`}><ol aria-label="処理結果">{item.stages.map((stage) => <li className={`stage stage--${stage.status}`} key={stage.name}><span><Icon name={stage.status === "failed" ? "warning" : "check"} size={13} /></span>{stage.name}</li>)}</ol><dl><div><dt>Action ID</dt><dd><code>{item.actionId}</code></dd></div><div><dt>Transaction</dt><dd><code>{item.transactionId}</code></dd></div>{item.diagnosticId == null ? null : <div><dt>診断ID</dt><dd><code>{item.diagnosticId}</code></dd></div>}</dl></div>
                  ) : null}
                  <div className="timeline-entry__actions">
                    <button aria-controls={`timeline-details-${item.itemId}`} aria-expanded={expanded} className="text-button" onClick={() => setExpandedId((current) => current === item.itemId ? null : item.itemId)} type="button">{expanded ? "内容・結果を閉じる" : "内容・結果を見る"}</button>
                    {item.retryAvailable ? <button className="text-button" disabled={recoveryBusy || dataMode !== "live"} onClick={onRetryRecovery} type="button">失敗だけ再試行</button> : null}
                    <button className="rollback-button" disabled={!canRollback || !item.rollbackAvailable || pending} onClick={() => onRequestRollback(item)} type="button">{pending ? <Icon className="spin" name="spinner" /> : <Icon name="undo" />}この変更だけ戻す</button>
                  </div>
                </article>
              </li>
            );
          })}
        </ol>
      )}
    </div>
  );
}
