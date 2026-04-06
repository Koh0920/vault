import { iconByName, LockLogo } from "./Icons";
import type { ProviderId, ViewName } from "../types";

interface SidebarProps {
  currentView: ViewName;
  connectedCount: number;
  bridgeReady: boolean;
  onChangeView: (view: ViewName) => void;
}

export function Sidebar({ currentView, connectedCount, bridgeReady, onChangeView }: SidebarProps) {
  const items: Array<{ view: ViewName; label: string; icon: ReturnType<typeof iconByName>; badge?: number }> = [
    { view: "dashboard", label: "ダッシュボード", icon: iconByName("dashboard") },
    { view: "remotes", label: "接続先", icon: iconByName("remotes"), badge: connectedCount || undefined },
    { view: "history", label: "履歴", icon: iconByName("history") },
    { view: "explorer", label: "エクスプローラ", icon: iconByName("explorer") },
  ];

  return (
    <div id="sidebar">
      <div className="logo">
        <div className="logo-icon"><LockLogo /></div>
        <span className="logo-name">Vault</span>
      </div>
      <nav className="nav">
        <div className="nav-sec">メイン</div>
        {items.map((item) => (
          <button key={item.view} type="button" className={`nav-item ${currentView === item.view ? "active" : ""}`} onClick={() => onChangeView(item.view)}>
            {item.icon}
            {item.label}
            {item.badge ? <span className="nav-badge">{item.badge}</span> : null}
          </button>
        ))}
        <div className="nav-div" />
        <div className="nav-sec">その他</div>
        <button type="button" className={`nav-item ${currentView === "settings" ? "active" : ""}`} onClick={() => onChangeView("settings")}>
          {iconByName("settings")}
          設定
        </button>
      </nav>
      <div className="sb-foot">
        <div className="bridge-pill">
          <span className={`bridge-dot ${bridgeReady ? "ok" : ""}`} />
          <span>{bridgeReady ? "Tauri bridge 接続済み" : "bridge なし（開発中）"}</span>
        </div>
      </div>
    </div>
  );
}
