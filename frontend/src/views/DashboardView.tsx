import { iconByName } from "../components/Icons";
import type { ProviderId, ProviderState } from "../types";

interface DashboardViewProps {
  providers: Record<ProviderId, ProviderState>;
  onConnect: (provider: ProviderId) => void;
  onStartBackup: () => void;
}

export function DashboardView({ providers, onConnect, onStartBackup }: DashboardViewProps) {
  return (
    <section className="view active">
      <div className="sec-hd">
        <div className="sec-eye">Local-first Backup</div>
        <h1 className="sec-title">クラウドバックアップ</h1>
        <p className="sec-sub">ファイルを暗号化してからクラウドへ送信します。鍵はこのマシンを離れません。</p>
      </div>

      <div className="provider-grid">
        <ProviderCard provider="drive" label="Google Drive" state={providers.drive} onConnect={onConnect} />
        <ProviderCard provider="r2" label="Cloudflare R2" state={providers.r2} onConnect={onConnect} />
      </div>

      <div className="dash-cta">
        <div>
          <div className="dash-cta-title">新しいバックアップを開始</div>
          <div className="dash-cta-sub">フォルダを選択して暗号化バックアップを実行します。</div>
        </div>
        <button type="button" className="btn btn-pri btn-lg" onClick={onStartBackup}>
          {iconByName("upload")}
          バックアップを開始
        </button>
      </div>
    </section>
  );
}

function ProviderCard({
  provider,
  label,
  state,
  onConnect,
}: {
  provider: ProviderId;
  label: string;
  state: ProviderState;
  onConnect: (provider: ProviderId) => void;
}) {
  const connected = state.status === "connected";
  const badgeClass = connected ? "ok" : state.status === "pending" ? "pend" : state.status === "failed" ? "err" : "off";
  const badgeLabel = connected ? "接続済み" : state.status === "pending" ? "接続中…" : state.status === "failed" ? "エラー" : provider === "drive" ? "未接続" : "未設定";
  return (
    <div className={`pcard ${connected ? "connected" : ""}`}>
      <div className="pcard-head">
        <div className={`pcard-icon ${provider}`}>{provider === "drive" ? "🔵" : "🟠"}</div>
        <span className={`sbadge ${badgeClass}`}>{badgeLabel}</span>
      </div>
      <div className="pcard-name">{label}</div>
      <div className="pcard-meta">{state.meta ?? (provider === "drive" ? "クリックしてブラウザ認証を開始" : ".env に認証情報を設定して接続")}</div>
      <div className="pcard-action">
        <button type="button" className="btn btn-sec btn-sm btn-w" onClick={() => onConnect(provider)} disabled={connected || state.status === "pending"}>
          {provider === "drive" ? iconByName("launch") : iconByName("box")}
          {provider === "drive" ? "Drive に接続" : "R2 を接続"}
        </button>
      </div>
    </div>
  );
}
