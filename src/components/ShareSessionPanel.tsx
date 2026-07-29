import { useEffect, useState } from "react";

import {
  detectAction,
  finishShareSession,
  getShareSessionState,
  getWindowLayoutStatus,
  openWindowsSettings,
  publicErrorMessage,
  startShareSession,
} from "../backend";
import type {
  ActionState,
  DataMode,
  ShareSessionState,
  WindowLayoutStatus,
} from "../model";
import { Icon } from "./Icon";

interface ShareSessionPanelProps {
  dataMode: DataMode;
}

export function ShareSessionPanel({ dataMode }: ShareSessionPanelProps) {
  const live = dataMode === "live";
  const [session, setSession] = useState<ShareSessionState | null>(null);
  const [layout, setLayout] = useState<WindowLayoutStatus | null>(null);
  const [microphone, setMicrophone] = useState<ActionState | null>(null);
  const [audioOutput, setAudioOutput] = useState<ActionState | null>(null);
  const [checked, setChecked] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!live) {
      setSession(null);
      setLayout(null);
      return;
    }
    let cancelled = false;
    void Promise.all([getShareSessionState(), getWindowLayoutStatus()])
      .then(([nextSession, nextLayout]) => {
        if (!cancelled) {
          setSession(nextSession);
          setLayout(nextLayout);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setSession(null);
          setLayout(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [live]);

  async function checkCurrentValues(clearMessage = true) {
    setBusy(true);
    if (clearMessage) setMessage(null);
    const [micResult, audioResult, layoutResult, sessionResult] = await Promise.allSettled([
      detectAction("audio.comms_mic_mute"),
      detectAction("setup.audio_output"),
      getWindowLayoutStatus(),
      getShareSessionState(),
    ]);
    setMicrophone(micResult.status === "fulfilled" ? micResult.value.state : null);
    setAudioOutput(audioResult.status === "fulfilled" ? audioResult.value.state : null);
    if (layoutResult.status === "fulfilled") setLayout(layoutResult.value);
    if (sessionResult.status === "fulfilled") setSession(sessionResult.value);
    setChecked(true);
    if (clearMessage && [micResult, audioResult, layoutResult, sessionResult].some((result) => result.status === "rejected")) {
      setMessage("読み取れなかった項目は、確認できないものとして表示しています。");
    }
    setBusy(false);
  }

  async function toggleSession() {
    setBusy(true);
    setMessage(null);
    try {
      const result = session?.active === true
        ? await finishShareSession()
        : await startShareSession();
      setSession(result.state);
      await checkCurrentValues(false);
      setMessage([result.message, ...result.details].join(" "));
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
      setBusy(false);
    }
  }

  async function openNotificationSettings() {
    setBusy(true);
    setMessage(null);
    try {
      await openWindowsSettings("notifications.toast_banners");
      setMessage("Windowsの通知設定を開きました。表示する通知と応答不可の設定を確認してください。");
    } catch (error: unknown) {
      setMessage(publicErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }

  const active = session?.active === true;
  const layoutReady = layout?.saved === true;
  const microphoneMeasured = microphone !== null
    && !["unknown", "unsupported", "error"].includes(microphone.kind);
  const audioMeasured = audioOutput !== null
    && !["unknown", "unsupported", "error"].includes(audioOutput.kind);
  const microphoneText = checked
    ? microphone?.label ?? "現在値を確認できませんでした"
    : "「いまの状態を確認」で読み取ります";
  const audioText = checked
    ? audioOutput?.label ?? "現在値を確認できませんでした"
    : "「いまの状態を確認」で読み取ります";

  return (
    <section className="share-session" aria-label="画面共有の前と後">
      <header className="share-session__header">
        <div>
          <span className="eyebrow">会議の前と後</span>
          <h2>画面共有の準備</h2>
          <p>
            忘れ物を減らすための確認です。映り込む内容や会議アプリの動作は判定しません。
            自動で扱う項目、自分で確かめる項目、確認できない項目を分けて表示します。
          </p>
        </div>
        <div className="share-session__controls">
          <button
            className="secondary-button"
            disabled={!live || busy}
            onClick={() => void checkCurrentValues()}
            type="button"
          >
            {busy ? <Icon className="spin" name="spinner" /> : <Icon name="search" />}
            いまの状態を確認
          </button>
          <button
            className="primary-button"
            disabled={!live || busy || (!active && !layoutReady)}
            onClick={() => void toggleSession()}
            type="button"
          >
            <Icon name={active ? "undo" : "check"} />
            {active ? "共有を終えて戻す" : "共有前の準備を始める"}
          </button>
        </div>
      </header>

      {!active && !layoutReady ? (
        <p className="share-session__notice" role="note">
          <Icon name="info" />
          セットアップ画面でウィンドウ配置を保存すると、共有用の配置とスリープ抑止を一緒に開始できます。
        </p>
      ) : null}

      <div className="share-session__groups">
        <section className="share-session__group">
          <h3><span>1</span>このアプリが自動で確認・変更したもの</h3>
          <ul>
            <li>
              <Icon name={active ? "check" : "info"} />
              <span>
                <strong>自動スリープと画面消灯</strong>
                <small>{active ? "このセッションの要求を送信中です。終了時に要求を解除します。" : "開始すると、このセッションの間だけ抑止要求を送ります。"}</small>
              </span>
            </li>
            <li>
              <Icon name={layoutReady ? "check" : "info"} />
              <span>
                <strong>ウィンドウ配置</strong>
                <small>{layoutReady ? `保存済みの${layout.windowCount}個を対象にします。会議中に手で動かした窓は終了時に上書きしません。` : "保存済みの配置がありません。"}</small>
              </span>
            </li>
            <li>
              <Icon name={microphoneMeasured ? "check" : "info"} />
              <span>
                <strong>Windows既定の通話用マイク</strong>
                <small>{microphoneText}。会議アプリが同じ入力を使っているかは分かりません。</small>
              </span>
            </li>
            <li>
              <Icon name={audioMeasured ? "check" : "info"} />
              <span>
                <strong>Windows既定の音声出力</strong>
                <small>{audioText}。会議アプリ内で別の出力先が選ばれている場合があります。</small>
              </span>
            </li>
          </ul>
        </section>

        <section className="share-session__group">
          <h3><span>2</span>利用者自身に確認してもらうもの</h3>
          <ul>
            <li>
              <Icon name="info" />
              <span>
                <strong>Windowsの通知と応答不可</strong>
                <small>表示する通知、優先するアプリ、応答不可の規則をWindows側で確認します。</small>
              </span>
              <button
                className="secondary-button"
                disabled={!live || busy}
                onClick={() => void openNotificationSettings()}
                type="button"
              >
                通知設定を開く
              </button>
            </li>
          </ul>
        </section>

        <section className="share-session__group share-session__group--unknown">
          <h3><span>3</span>確認できないもの</h3>
          <ul>
            <li>
              <Icon name="info" />
              <span>
                <strong>Teams、Zoom、ブラウザーなどの通知</strong>
                <small>各アプリ内の通知設定や、優先通知の表示状態は読み取れません。</small>
              </span>
            </li>
            <li>
              <Icon name="info" />
              <span>
                <strong>会議アプリが共有する範囲</strong>
                <small>どの画面やウィンドウを選んだか、別モニターが含まれるかは読み取れません。</small>
              </span>
            </li>
            <li>
              <Icon name="info" />
              <span>
                <strong>相手へ届く音声</strong>
                <small>会議アプリ独自の入力先、ミュート、通信後の音声は読み取れません。</small>
              </span>
            </li>
          </ul>
        </section>
      </div>

      {message === null ? null : <p className="share-session__message" role="status">{message}</p>}
      {live ? null : <p className="muted small">閲覧モードでは状態確認と変更を実行できません。</p>}
    </section>
  );
}
