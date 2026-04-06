import type { ProviderId, ProviderState } from "../types";

export function ConnectionsView({
  providers,
  onConnect,
}: {
  providers: Record<ProviderId, ProviderState>;
  onConnect: (provider: ProviderId) => void;
}) {
  const items: Array<{ id: ProviderId; label: string; icon: string }> = [
    { id: "drive", label: "Google Drive", icon: "🔵" },
    { id: "r2", label: "Cloudflare R2", icon: "🟠" },
  ];
  return (
    <section className="view active">
      <div className="sec-hd">
        <div className="sec-eye">接続先管理</div>
        <h1 className="sec-title">クラウド接続</h1>
        <p className="sec-sub">各クラウドサービスへの接続状態を確認・管理します。</p>
      </div>
      <div className="rml">
        {items.map((item) => {
          const provider = providers[item.id];
          const connected = provider.status === "connected";
          return (
            <div key={item.id} className="rml-row">
              <div className="rml-icon">{item.icon}</div>
              <div style={{ flex: 1 }}>
                <div className="rml-name">{item.label}</div>
                <div className="rml-meta">{provider.meta ?? (connected ? "接続済み" : "未接続")}</div>
              </div>
              <span className={`sbadge ${connected ? "ok" : "off"}`}>{connected ? "接続済み" : "未接続"}</span>
              <button type="button" className="btn btn-sec btn-sm" onClick={() => onConnect(item.id)} disabled={connected || provider.status === "pending"}>
                {connected ? "✓" : "接続"}
              </button>
            </div>
          );
        })}
      </div>
    </section>
  );
}
