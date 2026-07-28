import { useEffect, useState } from "react";

import { buildHealthReport, publicErrorMessage } from "../backend";
import type { DataMode, HealthEntry, HealthReport } from "../model";
import { Icon } from "./Icon";

interface HealthPanelProps {
  dataMode: DataMode;
}

/**
 * 適用した設定が今も残っているかを見る。**読むだけで、何も変更しない。**
 *
 * 値が基準と違っていても、原因が Windows の更新だとは限らない。
 * 利用者自身が変えたのかもしれないし、別のアプリかもしれない。
 * だからここには「違います」までしか書かない。更新の確認日時は参考として別枠に置く。
 *
 * 確認できなかったものは「確認できません」として別のリストに出す。
 * 分からないものを「大丈夫」に混ぜない。
 */
export function HealthPanel({ dataMode }: HealthPanelProps) {
  const live = dataMode === "live";
  const [report, setReport] = useState<HealthReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [openList, setOpenList] = useState<"holding" | "unknown" | null>(null);

  useEffect(() => {
    if (!live) return;
    let cancelled = false;
    setBusy(true);
    buildHealthReport()
      .then((result) => {
        if (!cancelled) setReport(result);
      })
      .catch((cause: unknown) => {
        if (!cancelled) setError(publicErrorMessage(cause));
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [live]);

  async function recheck() {
    setBusy(true);
    setError(null);
    try {
      setReport(await buildHealthReport());
    } catch (cause) {
      setError(publicErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  }

  if (!live) return null;
  if (busy && report === null) {
    return (
      <section className="health-panel health-panel--quiet">
        <Icon className="spin" name="spinner" size={16} />
        <p>いまの状態を確認しています。</p>
      </section>
    );
  }
  if (error !== null) {
    return (
      <section className="health-panel health-panel--quiet">
        <Icon name="warning" size={16} />
        <p>{error}</p>
      </section>
    );
  }
  if (report === null) return null;

  if (!report.hasBaseline) {
    return (
      <section className="health-panel health-panel--quiet">
        <Icon name="check" size={16} />
        <p>{report.noBaselineNote}</p>
      </section>
    );
  }

  const tone = report.changed.length > 0 ? "alert" : "calm";
  return (
    <section aria-label="適用した設定の照合" className={`health-panel health-panel--${tone}`}>
      <header className="health-panel__head">
        <Icon name={report.changed.length > 0 ? "warning" : "check"} size={18} />
        <div>
          <strong>{report.summary}</strong>
          {report.updateReference === null ? null : <p className="health-panel__reference">{report.updateReference}</p>}
        </div>
        <button className="text-button" disabled={busy} onClick={recheck} type="button">
          {busy ? <Icon className="spin" name="spinner" size={14} /> : null}もう一度確認
        </button>
      </header>

      {report.changed.length === 0 ? null : (
        <ul className="health-list">
          {report.changed.map((entry) => (
            <li key={entry.actionId}>
              <strong>{entry.name}</strong>
              <span>{entry.note}</span>
              <small>適用したのは {entry.appliedAt}</small>
            </li>
          ))}
        </ul>
      )}

      <div className="health-panel__folds">
        {report.holding.length === 0 ? null : (
          <Fold
            entries={report.holding}
            label={`適用したときのまま（${report.holding.length}件）`}
            name="holding"
            open={openList === "holding"}
            onToggle={setOpenList}
          />
        )}
        {report.unknown.length === 0 ? null : (
          <Fold
            entries={report.unknown}
            label={`確認できなかったもの（${report.unknown.length}件）`}
            name="unknown"
            open={openList === "unknown"}
            onToggle={setOpenList}
          />
        )}
      </div>
    </section>
  );
}

interface FoldProps {
  entries: readonly HealthEntry[];
  label: string;
  name: "holding" | "unknown";
  open: boolean;
  onToggle: (next: "holding" | "unknown" | null) => void;
}

function Fold({ entries, label, name, open, onToggle }: FoldProps) {
  return (
    <div className="health-fold">
      <button aria-expanded={open} className="text-button" onClick={() => onToggle(open ? null : name)} type="button">
        {label}
      </button>
      {open ? (
        <ul className="health-list health-list--quiet">
          {entries.map((entry) => (
            <li key={entry.actionId}>
              <strong>{entry.name}</strong>
              <span>{entry.note}</span>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
