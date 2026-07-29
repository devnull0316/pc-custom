import { useEffect, useState } from "react";

import { hotCornerGet, hotCornerSet, publicErrorMessage } from "../backend";
import type {
  DataMode,
  HotCornerAction,
  HotCornerSetting,
} from "../model";
import { Icon } from "./Icon";

interface HotCornerPanelProps {
  dataMode: DataMode;
}

const CORNERS: readonly {
  key: keyof Pick<
    HotCornerSetting,
    "topLeft" | "topRight" | "bottomLeft" | "bottomRight"
  >;
  label: string;
}[] = [
  { key: "topLeft", label: "左上" },
  { key: "topRight", label: "右上" },
  { key: "bottomLeft", label: "左下" },
  { key: "bottomRight", label: "右下" },
];

export function HotCornerPanel({ dataMode }: HotCornerPanelProps) {
  const live = dataMode === "live";
  const [setting, setSetting] = useState<HotCornerSetting | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!live) {
      setSetting(null);
      return;
    }
    let cancelled = false;
    void hotCornerGet()
      .then((state) => {
        if (!cancelled) {
          setSetting(state.setting);
          setMessage(state.lastError);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) setMessage(publicErrorMessage(error));
      });
    return () => {
      cancelled = true;
    };
  }, [live]);

  function updateCorner(
    key: (typeof CORNERS)[number]["key"],
    action: HotCornerAction,
  ) {
    setSetting((current) =>
      current === null ? current : { ...current, [key]: action },
    );
    setMessage(null);
  }

  async function save() {
    if (setting === null) return;
    setBusy(true);
    setMessage(null);
    try {
      const state = await hotCornerSet(setting);
      setSetting(state.setting);
      setMessage("保存しました。角に着いてもWindowsの設定やモードは変更しません。");
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section aria-labelledby="hot-corner-title" className="hot-corner-panel">
      <div className="hot-corner-panel__heading">
        <span className="hot-corner-panel__icon">
          <Icon name="arrow" size={18} />
        </span>
        <div>
          <h2 id="hot-corner-title">画面の角からモードを開く</h2>
          <p>
            マウスを止めたときだけPCカスタムを前へ出します。
            <strong>モードの適用は行いません。</strong>
          </p>
        </div>
      </div>

      {setting === null ? (
        <p className="muted small">
          {live ? "設定を読み込んでいます。" : "安全コアに接続すると設定できます。"}
        </p>
      ) : (
        <>
          <div className="hot-corner-grid">
            {CORNERS.map((corner) => (
              <label className="field" key={corner.key}>
                <span>{corner.label}</span>
                <select
                  disabled={!live || busy}
                  onChange={(event) =>
                    updateCorner(corner.key, event.target.value as HotCornerAction)
                  }
                  value={setting[corner.key]}
                >
                  <option value="none">何もしない</option>
                  <option value="open_modes">モード画面を開く</option>
                </select>
              </label>
            ))}
          </div>
          <div className="hot-corner-timing">
            <label className="field">
              <span>角で止まる時間</span>
              <select
                disabled={!live || busy}
                onChange={(event) =>
                  setSetting({ ...setting, dwellMs: Number(event.target.value) })
                }
                value={setting.dwellMs}
              >
                <option value={600}>0.6秒</option>
                <option value={1000}>1秒</option>
                <option value={1500}>1.5秒（既定）</option>
                <option value={2000}>2秒</option>
                <option value={3000}>3秒</option>
              </select>
            </label>
            <label className="field">
              <span>もう一度開けるまで</span>
              <select
                disabled={!live || busy}
                onChange={(event) =>
                  setSetting({
                    ...setting,
                    cooldownMs: Number(event.target.value),
                  })
                }
                value={setting.cooldownMs}
              >
                <option value={5000}>5秒</option>
                <option value={10000}>10秒</option>
                <option value={15000}>15秒（既定）</option>
                <option value={30000}>30秒</option>
                <option value={60000}>60秒</option>
              </select>
            </label>
          </div>
          <div className="hot-corner-panel__footer">
            <p className="muted small">
              4つとも「何もしない」が既定です。全画面・最大化中と、モニター間の角では開きません。
            </p>
            <button
              className="secondary-button"
              disabled={!live || busy}
              onClick={() => void save()}
              type="button"
            >
              {busy ? <Icon className="spin" name="spinner" /> : <Icon name="check" />}
              設定を保存
            </button>
          </div>
        </>
      )}
      {message === null ? null : (
        <p className="hot-corner-panel__message" role="status">
          {message}
        </p>
      )}
    </section>
  );
}
