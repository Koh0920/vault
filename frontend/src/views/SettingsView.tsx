import type { AppState } from "../types";

export function SettingsView({
  appState,
  onToggleDebug,
}: {
  appState: AppState;
  onToggleDebug: () => void;
}) {
  return (
    <section className="view active">
      <div className="sec-hd">
        <div className="sec-eye">設定</div>
        <h1 className="sec-title">アプリ設定</h1>
      </div>
      <div className="s-card">
        <div className="s-card-title">rclone 設定ファイルのパス</div>
        <div className="s-card-desc">Drive backend が利用する rclone 設定ファイルの場所です。</div>
        <div className="f">
          <input className="f-inp mono" type="text" readOnly value={appState.runtimeConfig?.configPath ?? ""} placeholder="読み込み中…" />
        </div>
      </div>
      <div className="s-card">
        <div className="s-card-title">デバッグ情報</div>
        <div className="s-card-desc">現在のランタイム設定と vault explorer の状態を表示します。</div>
        <button type="button" className="btn btn-ghost btn-sm" onClick={onToggleDebug}>
          {appState.debugOpen ? "閉じる" : "設定を表示"}
        </button>
        {appState.debugOpen ? (
          <pre style={{ marginTop: 12 }}>
            {JSON.stringify({
              runtimeConfig: appState.runtimeConfig,
              providers: appState.providers,
              explorer: {
                provider: appState.explorer.provider,
                selectedRepoId: appState.explorer.selectedRepoId,
                selectedSnapshotId: appState.explorer.selectedSnapshotId,
                currentPath: appState.explorer.currentPath,
              },
            }, null, 2)}
          </pre>
        ) : null}
      </div>
    </section>
  );
}
