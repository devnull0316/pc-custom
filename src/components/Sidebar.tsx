import type { DataMode, IconName, ViewId } from "../model";
import { Icon } from "./Icon";

interface SidebarProps {
  activeView: ViewId;
  dataMode: DataMode;
  profileCount: number;
  onNavigate: (view: ViewId) => void;
  onOpenDraft: () => void;
}

const NAV_ITEMS: readonly { id: ViewId; label: string; icon: IconName }[] = [
  { id: "home", label: "ホーム", icon: "home" },
  { id: "actions", label: "Action", icon: "action" },
  { id: "profiles", label: "モード", icon: "game" },
  { id: "setup", label: "PCセットアップ", icon: "plus" },
  { id: "timeline", label: "タイムライン", icon: "timeline" },
];

export function Sidebar({ activeView, dataMode, profileCount, onNavigate, onOpenDraft }: SidebarProps) {
  return (
    <aside className="sidebar">
      <div aria-label="Totonoe" className="brand">
        <div aria-hidden="true" className="brand-mark"><span /><span /><span /></div>
        <span className="brand-name">Totonoe</span>
      </div>
      <nav aria-label="メインナビゲーション" className="main-nav">
        {NAV_ITEMS.map((item) => (
          <button
            aria-current={activeView === item.id ? "page" : undefined}
            className="nav-button"
            key={item.id}
            onClick={() => onNavigate(item.id)}
            type="button"
          >
            <Icon name={item.icon} /><span>{item.label}</span>
          </button>
        ))}
      </nav>
      <div className="sidebar-spacer" />
      <button className="draft-button" onClick={onOpenDraft} type="button">
        <span className="draft-button__icon"><Icon name="plus" size={16} /></span>
        <span><strong>プロファイル下書き</strong><small>{profileCount === 0 ? "まだ空です" : `${profileCount}件のAction`}</small></span>
      </button>
      <div className={`connection-state connection-state--${dataMode}`}>
        <span aria-hidden="true" className="connection-dot" />
        <span>{dataMode === "live" ? "安全コア接続中" : dataMode === "loading" ? "安全コアを確認中" : "カタログ閲覧のみ"}</span>
      </div>
    </aside>
  );
}
