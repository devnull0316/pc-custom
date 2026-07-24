import { RESULT_TILES } from "../catalog";
import type { ActionPresentation, BootstrapStatus, CategoryId, DataMode, TimelineItem } from "../model";
import { timelineStatusLabel } from "../model";
import { Icon } from "./Icon";

interface HomeViewProps {
  dataMode: DataMode;
  bootstrap: BootstrapStatus | null;
  actions: readonly ActionPresentation[];
  timeline: readonly TimelineItem[];
  onOpenCategory: (category: CategoryId) => void;
  onOpenTimeline: () => void;
  onReconcile: () => void;
  recoveryBusy: boolean;
}

export function HomeView({ dataMode, bootstrap, actions, timeline, onOpenCategory, onOpenTimeline, onReconcile, recoveryBusy }: HomeViewProps) {
  const recent = timeline[0];
  return (
    <div className="view home-view">
      <header className="view-heading home-heading">
        <div><p className="eyebrow">結果から選ぶ</p><h1>今日は何を整えますか</h1><p>専門用語ではなく、得たい結果から選べます。適用前には必ず差分を確認します。</p></div>
        <div aria-label="システム互換性" className="system-summary">
          <span className={`system-summary__mark system-summary__mark--${bootstrap?.mode ?? "loading"}`}><Icon name={bootstrap?.mode === "ready" ? "check" : "info"} size={15} /></span>
          <span><small>{bootstrap?.osLabel ?? "Windowsを確認中"}</small><strong>{bootstrap?.build == null ? "build 未確認" : `build ${bootstrap.build}`}</strong></span>
        </div>
      </header>
      {bootstrap?.mode === "recovery_required" ? (
        <section aria-labelledby="recovery-title" className="recovery-callout">
          <span className="recovery-callout__icon"><Icon name="recovery" size={22} /></span>
          <div><p className="eyebrow">新しい変更より先に</p><h2 id="recovery-title">{bootstrap.recoveryCount > 0 ? `${bootstrap.recoveryCount}件の復旧を確認してください` : "このWindows buildでは変更を停止しています"}</h2><p>{bootstrap.message}</p></div>
          {bootstrap.recoveryCount > 0 ? <button className="primary-button" disabled={recoveryBusy} onClick={onReconcile} type="button">{recoveryBusy ? <Icon className="spin" name="spinner" /> : <Icon name="recovery" />}復旧を確認</button> : <span className="read-only-badge">読み取りのみ</span>}
        </section>
      ) : null}
      <section aria-labelledby="results-title">
        <div className="section-heading"><div><h2 id="results-title">整えたいこと</h2><p>{actions.length}件の登録済みActionから安全な候補を表示します。</p></div></div>
        <div className="result-grid">
          {RESULT_TILES.map((tile, index) => (
            <button className={`result-tile result-tile--${index + 1}`} key={tile.id} onClick={() => tile.category === "recovery" ? onOpenTimeline() : onOpenCategory(tile.category)} type="button">
              <span className="result-tile__icon"><Icon name={tile.icon} size={22} /></span>
              <span className="result-tile__copy"><strong>{tile.title}</strong><small>{tile.description}</small></span>
              <Icon className="result-tile__arrow" name="chevron" />
            </button>
          ))}
        </div>
      </section>
      <section aria-labelledby="recent-title" className="recent-section">
        <div className="section-heading"><div><h2 id="recent-title">最近の変更</h2><p>履歴は項目ごとに確認し、必要なものだけ戻せます。</p></div><button className="text-button" onClick={onOpenTimeline} type="button">すべて見る</button></div>
        {dataMode === "loading" ? (
          <div aria-label="履歴を読み込み中" className="skeleton-row"><span /><span /><span /></div>
        ) : recent === undefined ? (
          <div className="compact-empty"><span><Icon name="timeline" /></span><p><strong>まだ変更はありません</strong>最初の変更も、適用前に差分を確認できます。</p></div>
        ) : (
          <button className="recent-item" onClick={onOpenTimeline} type="button"><span className={`status-symbol status-symbol--${recent.status}`}><Icon name="check" size={15} /></span><span className="recent-item__copy"><strong>{recent.title}</strong><small>{recent.summary}</small></span><span className={`status-pill status-pill--${recent.status}`}>{timelineStatusLabel(recent.status)}</span><Icon name="chevron" /></button>
        )}
      </section>
    </div>
  );
}
