import { useEffect, useState } from "react";

import { publicErrorMessage, themeScheduleGet, themeScheduleSet } from "../backend";
import type { DataMode, ThemeSchedule } from "../model";
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
  const [schedule, setSchedule] = useState<ThemeSchedule | null>(null);
  const [lastError, setLastError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const state = await themeScheduleGet();
        if (!cancelled) {
          setSchedule(state.schedule);
          setLastError(state.lastError);
        }
      } catch {
        // 安全コア未接続時は閲覧のみ。既定値を表示して操作は無効化する。
        if (!cancelled) {
          setSchedule({ enabled: false, lightAtMinutes: 7 * 60, darkAtMinutes: 19 * 60 });
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  async function save(next: ThemeSchedule) {
    setBusy(true);
    setMessage(null);
    try {
      const state = await themeScheduleSet(next);
      setSchedule(state.schedule);
      setLastError(state.lastError);
      setMessage(
        state.schedule.enabled
          ? "時間帯に合わせて自動で切り替えます。"
          : "自動切り替えを止めました。",
      );
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  if (schedule === null) return null;

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
            disabled={!live || busy}
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
            disabled={!live || busy}
            onChange={(event) => {
              const minutes = fromTimeInput(event.target.value);
              if (minutes !== null) setSchedule({ ...schedule, lightAtMinutes: minutes });
            }}
            type="time"
            value={toTimeInput(schedule.lightAtMinutes)}
          />
        </label>
        <label>
          <span>暗くする</span>
          <input
            disabled={!live || busy}
            onChange={(event) => {
              const minutes = fromTimeInput(event.target.value);
              if (minutes !== null) setSchedule({ ...schedule, darkAtMinutes: minutes });
            }}
            type="time"
            value={toTimeInput(schedule.darkAtMinutes)}
          />
        </label>
        <button
          className="secondary-button"
          disabled={!live || busy}
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
      {message === null ? null : <p className="theme-schedule__message" role="status">{message}</p>}
      {live ? null : <p className="muted small">閲覧モードです。安全コアに接続すると設定できます。</p>}
    </section>
  );
}
