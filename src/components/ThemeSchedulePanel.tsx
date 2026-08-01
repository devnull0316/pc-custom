import { useEffect, useRef, useState } from "react";

import { listTimeline, publicErrorMessage, rollbackItem, themeScheduleGet, themeScheduleSet } from "../backend";
import { LatestRequestGuard, failedRead, loadingRead, successfulRead } from "../frontendLogic";
import type { DataMode, ThemeSchedule, TimelineItem } from "../model";
import { Icon } from "./Icon";

interface ThemeSchedulePanelProps {
  dataMode: DataMode;
}

function toTimeInput(minutes: number): string {
  const hour = Math.floor(minutes / 60) % 24;
  const minute = minutes % 60;
  return `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
}

function fromTimeInput(value: string): number | null {
  const match = /^(\d{2}):(\d{2})$/.exec(value);
  if (match === null) return null;
  const hour = Number(match[1]);
  const minute = Number(match[2]);
  if (hour > 23 || minute > 59) return null;
  return hour * 60 + minute;
}

export function ThemeSchedulePanel({ dataMode }: ThemeSchedulePanelProps) {
  const live = dataMode === "live";
  const [scheduleRead, setScheduleRead] = useState(() => loadingRead<ThemeSchedule | null>(null));
  const [lastError, setLastError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  // オフにしても、オンの間に適用された明るさはそのまま残る。
  // 以前はそれを伝えず、戻すにはタイムラインまで移動する必要があった。
  const [revertableItemId, setRevertableItemId] = useState<string | null>(null);
  const [reverting, setReverting] = useState(false);
  const loadRequests = useRef(new LatestRequestGuard());
  const schedule = scheduleRead.value;
  const editable = live && scheduleRead.status === "ready";

  useEffect(() => {
    if (dataMode !== "live") {
      loadRequests.current.invalidate();
      return;
    }
    const generation = loadRequests.current.begin();
    void (async () => {
      try {
        const state = await themeScheduleGet();
        if (loadRequests.current.isCurrent(generation)) {
          setScheduleRead(successfulRead(state.schedule));
          setLastError(state.lastError);
        }
      } catch (error: unknown) {
        if (loadRequests.current.isCurrent(generation)) {
          setScheduleRead((current) => failedRead(current, publicErrorMessage(error)));
        }
      }
    })();
    return () => {
      loadRequests.current.invalidate();
    };
  }, [dataMode]);

  async function save(next: ThemeSchedule) {
    if (!editable) return;
    setBusy(true);
    setMessage(null);
    try {
      const state = await themeScheduleSet(next);
      setScheduleRead(successfulRead(state.schedule));
      setLastError(state.lastError);
      if (state.schedule.enabled) {
        setMessage("時間帯に合わせて自動で切り替えます。");
        setRevertableItemId(null);
      } else {
        // 止めただけでは、既に変わった明るさは戻らない。そう書いて、戻す手段を隣に置く。
        const applied = await lastAppliedColorMode();
        setRevertableItemId(applied);
        setMessage(
          applied === null
            ? "自動切り替えを止めました。"
            : "自動切り替えを止めました。すでに変わった明るさはそのままです。",
        );
      }
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  /// 直近に適用された明るさの変更のうち、まだ戻せるものを探す。
  async function lastAppliedColorMode(): Promise<string | null> {
    try {
      const timeline = await listTimeline();
      const hit = timeline.find(
        (item: TimelineItem) => item.actionId === "theme.color_mode" && item.rollbackAvailable,
      );
      return hit?.itemId ?? null;
    } catch {
      return null;
    }
  }

  async function revertApplied() {
    if (revertableItemId === null) return;
    setReverting(true);
    try {
      const result = await rollbackItem({ itemId: revertableItemId });
      setMessage(
        result.status === "recovery_required"
          ? result.message
          : "明るさを元へ戻しました。",
      );
      setRevertableItemId(null);
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
    } finally {
      setReverting(false);
    }
  }

  if (schedule === null) {
    return (
      <section className="theme-schedule" aria-label="時間帯によるテーマ切り替え">
        <h2>時間帯で明るさを切り替える</h2>
        <p className="muted small" role="status">
          {dataMode !== "live"
            ? "閲覧モードです。安全コアに接続すると設定を読み取れます。"
            : scheduleRead.status === "error"
            ? `設定を読み取れませんでした: ${scheduleRead.message}`
            : "設定を読み込んでいます。"}
        </p>
      </section>
    );
  }

  return (
    <section className="theme-schedule" aria-label="時間帯によるテーマ切り替え">
      <div className="theme-schedule__head">
        <div>
          <h2>時間帯で明るさを切り替える</h2>
          <p className="muted small">
            指定した時刻になったら、ライト/ダークを自動で切り替えます。
            切り替えは履歴に1件ずつ残るので、後から個別に元へ戻せます。
            途中で手動変更したときは、次の時刻まで上書きしません。
          </p>
        </div>
        <label className="theme-schedule__toggle">
          <input
            checked={schedule.enabled}
            disabled={!editable || busy}
            onChange={(event) => void save({ ...schedule, enabled: event.target.checked })}
            type="checkbox"
          />
          <span>{schedule.enabled ? "オン" : "オフ"}</span>
        </label>
      </div>

      <div className="theme-schedule__row">
        <label>
          <span>明るくする</span>
          <input
            disabled={!editable || busy}
            onChange={(event) => {
              const minutes = fromTimeInput(event.target.value);
              if (minutes !== null) setScheduleRead(successfulRead({ ...schedule, lightAtMinutes: minutes }));
            }}
            type="time"
            value={toTimeInput(schedule.lightAtMinutes)}
          />
        </label>
        <label>
          <span>暗くする</span>
          <input
            disabled={!editable || busy}
            onChange={(event) => {
              const minutes = fromTimeInput(event.target.value);
              if (minutes !== null) setScheduleRead(successfulRead({ ...schedule, darkAtMinutes: minutes }));
            }}
            type="time"
            value={toTimeInput(schedule.darkAtMinutes)}
          />
        </label>
        <button
          className="secondary-button"
          disabled={!editable || busy}
          onClick={() => void save(schedule)}
          type="button"
        >
          {busy ? <Icon className="spin" name="spinner" /> : <Icon name="check" />}時刻を保存
        </button>
      </div>

      {lastError === null ? null : (
        <p className="theme-schedule__error" role="status">
          <Icon name="warning" size={14} />
          自動切り替えに失敗しました: {lastError}
        </p>
      )}
      {message === null ? null : (
        <p className="theme-schedule__message" role="status">
          {message}
          {revertableItemId === null ? null : (
            <button className="link-button" disabled={!live || reverting} onClick={() => void revertApplied()} type="button">
              {reverting ? "戻しています…" : "明るさも元に戻す"}
            </button>
          )}
        </p>
      )}
      {live ? null : <p className="muted small">閲覧モードです。安全コアに接続すると設定できます。</p>}
    </section>
  );
}
