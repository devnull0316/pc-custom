import { useEffect, useMemo, useState } from "react";

import {
  publicErrorMessage,
  storageHistoryCapture,
  storageHistoryClear,
  storageHistoryList,
} from "../backend";
import type {
  DataMode,
  StorageCategory,
  StorageCategoryPoint,
  StorageHistoryPoint,
} from "../model";
import { Icon } from "./Icon";

interface StorageHistoryPanelProps {
  dataMode: DataMode;
}

const CATEGORY_OPTIONS: readonly {
  id: StorageCategory;
  label: string;
}[] = [
  { id: "downloads", label: "ダウンロード" },
  { id: "documents", label: "ドキュメント" },
  { id: "desktop", label: "デスクトップ" },
  { id: "pictures", label: "ピクチャ" },
  { id: "videos", label: "ビデオ" },
];

function formatBytes(bytes: number): string {
  const absolute = Math.abs(bytes);
  if (absolute < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = absolute / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const sign = bytes < 0 ? "−" : bytes > 0 ? "+" : "";
  return `${sign}${value.toFixed(1)} ${units[unit]}`;
}

function categoryLabel(category: StorageCategory): string {
  return CATEGORY_OPTIONS.find((option) => option.id === category)?.label ?? "既知フォルダー";
}

function categoryNotice(point: StorageCategoryPoint): string | null {
  const notices: string[] = [];
  if (point.truncated) notices.push("走査上限のため途中まで");
  if (point.accessDeniedCount > 0) {
    notices.push(`アクセスできない分岐 ${point.accessDeniedCount}件`);
  }
  if (point.unreadableEntries > 0) {
    notices.push(`読み取れない項目 ${point.unreadableEntries}件`);
  }
  if (point.skippedReparsePoints > 0) {
    notices.push(`追跡しないリンク ${point.skippedReparsePoints}件`);
  }
  return notices.length === 0 ? null : notices.join("・");
}

export function StorageHistoryPanel({ dataMode }: StorageHistoryPanelProps) {
  const live = dataMode === "live";
  const [selected, setSelected] = useState<readonly StorageCategory[]>([
    "downloads",
    "documents",
  ]);
  const [history, setHistory] = useState<readonly StorageHistoryPoint[]>([]);
  const [busy, setBusy] = useState(false);
  const [confirmingClear, setConfirmingClear] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!live) return;
    let active = true;
    void storageHistoryList()
      .then((points) => {
        if (active) setHistory(points);
      })
      .catch((error: unknown) => {
        if (active) setMessage(publicErrorMessage(error));
      });
    return () => {
      active = false;
    };
  }, [live]);

  const selectedSet = useMemo(() => new Set(selected), [selected]);

  function toggle(category: StorageCategory) {
    setSelected((current) =>
      current.includes(category)
        ? current.filter((candidate) => candidate !== category)
        : [...current, category],
    );
  }

  async function capture() {
    setBusy(true);
    setMessage(null);
    setConfirmingClear(false);
    try {
      await storageHistoryCapture(selected);
      setHistory(await storageHistoryList());
      setMessage("容量の集計を記録しました。Windowsやファイルは変更していません。");
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  async function clear() {
    setBusy(true);
    setMessage(null);
    try {
      const count = await storageHistoryClear();
      setHistory([]);
      setConfirmingClear(false);
      setMessage(`${count}回分の集計履歴を消しました。Windowsのファイルは変更していません。`);
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section aria-label="容量がいつ減ったか分かる履歴" className="storage-history">
      <header>
        <div>
          <h2>容量がいつ変わったかを見る</h2>
          <p className="muted small">
            ドライブの空き容量と、選んだWindows既知フォルダーの合計だけを記録します。
            ファイル名・個別パス・中身は保存しません。増減が同じ期間に起きたことだけを示し、
            原因とは断定しません。
          </p>
        </div>
        {history.length === 0 ? null : (
          <button
            className="text-button"
            disabled={busy}
            onClick={() => setConfirmingClear(true)}
            type="button"
          >
            履歴を消す
          </button>
        )}
      </header>

      <fieldset className="storage-history__choices" disabled={!live || busy}>
        <legend>集計する場所</legend>
        {CATEGORY_OPTIONS.map((option) => (
          <label key={option.id}>
            <input
              checked={selectedSet.has(option.id)}
              onChange={() => toggle(option.id)}
              type="checkbox"
            />
            <span>{option.label}</span>
          </label>
        ))}
      </fieldset>

      <div className="config-io__row">
        <button
          className="secondary-button"
          disabled={!live || busy || selected.length === 0}
          onClick={() => void capture()}
          type="button"
        >
          {busy ? <Icon className="spin" name="spinner" /> : <Icon name="timeline" />}
          今の容量を記録
        </button>
        {confirmingClear ? (
          <>
            <button
              className="danger-button"
              disabled={busy}
              onClick={() => void clear()}
              type="button"
            >
              集計履歴だけを消す
            </button>
            <button
              className="secondary-button"
              disabled={busy}
              onClick={() => setConfirmingClear(false)}
              type="button"
            >
              やめる
            </button>
          </>
        ) : null}
      </div>

      {history.length === 0 ? (
        <p className="muted small">まだ記録はありません。2回目から期間ごとの増減を表示します。</p>
      ) : (
        <ol className="storage-history__timeline">
          {history.map((point, index) => (
            <li key={`${point.capturedAtUnixMs}-${index}`}>
              <div className="storage-history__point">
                <time dateTime={new Date(point.capturedAtUnixMs).toISOString()}>
                  {new Date(point.capturedAtUnixMs).toLocaleString("ja-JP")}
                </time>
                <strong>空き {formatBytes(point.driveAvailableBytes)}</strong>
                <span className="muted small">
                  {point.driveFreeDeltaBytes === null
                    ? "最初の記録"
                    : `前回から ${formatBytes(point.driveFreeDeltaBytes)}`}
                </span>
              </div>
              <ul>
                {point.categories.map((category) => {
                  const notice = categoryNotice(category);
                  return (
                    <li key={category.category}>
                      <span>
                        <strong>{categoryLabel(category.category)}</strong>
                        <small>
                          {formatBytes(category.totalBytes)}・{category.fileCount}件
                        </small>
                      </span>
                      <span>
                        <strong>
                          {category.totalBytesDelta === null
                            ? "基準"
                            : `前回から ${formatBytes(category.totalBytesDelta)}`}
                        </strong>
                        {notice === null ? null : <small>{notice}</small>}
                      </span>
                    </li>
                  );
                })}
              </ul>
            </li>
          ))}
        </ol>
      )}

      {message === null ? null : (
        <p className="storage-history__message" role="status">
          {message}
        </p>
      )}
      {live ? null : (
        <p className="muted small">閲覧モードです。安全コアに接続すると集計できます。</p>
      )}
    </section>
  );
}
