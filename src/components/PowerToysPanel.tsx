import { EVERYDAY_FEATURE_GUIDES } from "../catalog";
import type { ActionPresentation, DataMode } from "../model";
import { Icon } from "./Icon";

interface PowerToysPanelProps {
  action: ActionPresentation | undefined;
  dataMode: DataMode;
  detecting: boolean;
  launching: boolean;
  onDetect: () => void;
  onLaunch: () => void;
  onShowInstall: () => void;
}

export function PowerToysPanel({
  action,
  dataMode,
  detecting,
  launching,
  onDetect,
  onLaunch,
  onShowInstall,
}: PowerToysPanelProps) {
  const live = dataMode === "live";
  const integration = action?.currentState?.integration;
  const installed = integration?.installed === true;
  const launchAvailable = integration?.launchAvailable === true;
  const status = action?.currentState?.label ?? "導入状況はまだ確認していません";

  return (
    <section aria-labelledby="everyday-title" className="powertoys-panel">
      <header className="powertoys-panel__header">
        <div>
          <span className="eyebrow">操作・普段使い</span>
          <h2 id="everyday-title">やりたいことからPowerToysの機能を選ぶ</h2>
          <p>日常操作は同等機能を作り直さず、Microsoft PowerToysとWindows標準機能を案内します。</p>
        </div>
        <div aria-live="polite" className="powertoys-status">
          <span className={`powertoys-status__mark powertoys-status__mark--${installed ? "installed" : "idle"}`}>
            <Icon name={installed ? "check" : "info"} size={16} />
          </span>
          <span><small>PowerToys</small><strong>{status}</strong></span>
        </div>
      </header>

      <div className="powertoys-panel__actions">
        {installed && launchAvailable ? (
          <button className="primary-button" disabled={!live || launching} onClick={onLaunch} type="button">
            {launching ? <Icon className="spin" name="spinner" /> : <Icon name="arrow" />}
            PowerToysを起動
          </button>
        ) : (
          <button className="primary-button" disabled={!live} onClick={onShowInstall} type="button">
            <Icon name="plus" />PowerToysの導入へ
          </button>
        )}
        <button className="secondary-button" disabled={!live || detecting} onClick={onDetect} type="button">
          {detecting ? <Icon className="spin" name="spinner" /> : <Icon name="check" />}
          導入状況を再確認
        </button>
      </div>

      {/* 提供元は12回繰り返さず、2つのグループに分けて1回ずつ言う。
          同じ文字が12個並ぶのは情報ではなく雑音で、目が拾う場所を12箇所に増やすだけだった。 */}
      {([
        ["powertoys", "PowerToys の機能"],
        ["builtin", "Windows 標準の機能"],
      ] as const).map(([provider, heading]) => {
        const items = EVERYDAY_FEATURE_GUIDES.filter((guide) => guide.provider === provider);
        if (items.length === 0) return null;
        return (
          <div className="everyday-group" key={provider}>
            <h3 className="everyday-group__heading">{heading}<span>{items.length}</span></h3>
            <ul className="everyday-list">
              {items.map((guide) => (
                <li className="everyday-row" key={guide.id}>
                  <div className="everyday-row__main">
                    <strong>{guide.result}</strong>
                    <p>{guide.description}</p>
                  </div>
                  <span className="everyday-row__feature">{guide.featureName}</span>
                </li>
              ))}
            </ul>
          </div>
        );
      })}

      <div className="inline-note" role="note">
        <Icon name="info" />
        <span>PCカスタムはPowerToysの設定ファイルを読み書きせず、キーフック・常駐フック・他プロセスへのinjectionも行いません。</span>
      </div>
    </section>
  );
}
