import { iconByName } from "../components/Icons";
import type { ProviderId, ProviderState, WizardState } from "../types";

interface WizardViewProps {
  wizard: WizardState;
  providers: Record<ProviderId, ProviderState>;
  onBackToDashboard: () => void;
  onPatch: (patch: Partial<WizardState>) => void;
  onSubmit: () => void;
  onPickFolders: () => void;
  onPickFiles: () => void;
  onRemoveSource: (path: string) => void;
  onClearSources: () => void;
}

export function WizardView({
  wizard,
  providers,
  onBackToDashboard,
  onPatch,
  onSubmit,
  onPickFolders,
  onPickFiles,
  onRemoveSource,
  onClearSources,
}: WizardViewProps) {
  return (
    <section className="view active">
      <div className="sec-hd">
        <button type="button" className="btn btn-ghost btn-sm" style={{ marginBottom: 14 }} onClick={onBackToDashboard}>
          {iconByName("back")}
          戻る
        </button>
        <div className="sec-eye">New Snapshot</div>
        <h1 className="sec-title">Vault バックアップ設定</h1>
      </div>

      <div className="wiz-steps">
        {[1, 2, 3].map((step) => (
          <div key={step} className={`wiz-si ${wizard.step === step ? "active" : wizard.step > step ? "done" : ""}`}>
            <div className="wiz-num">{step}</div>
            <div className="wiz-lbl">{step === 1 ? "ソース" : step === 2 ? "保存先" : "パスワード"}</div>
            {step < 3 ? <div className="wiz-conn" /> : null}
          </div>
        ))}
      </div>

      {wizard.step === 1 ? (
        <div className="wiz-panel active">
          <div className="wiz-panel-title">バックアップする項目を選択</div>
          <div className="wiz-panel-desc">複数のフォルダまたはファイルを選択して、1つの snapshot として保存します。</div>
          <div className="f-group">
            <div className="f">
              <label className="f-lbl">選択済み項目</label>
              <div className="f-row">
                <button type="button" className="btn btn-ghost btn-sm" onClick={onPickFolders}>フォルダを追加…</button>
                <button type="button" className="btn btn-ghost btn-sm" onClick={onPickFiles}>ファイルを追加…</button>
                <button type="button" className="btn btn-ghost btn-sm" onClick={onClearSources} disabled={!wizard.sourcePaths.length}>クリア</button>
              </div>
              <div className="wiz-source-summary">{wizard.sourcePaths.length} 件選択中</div>
              <div className="wiz-source-list">
                {wizard.sourcePaths.length ? wizard.sourcePaths.map((path) => (
                  <div key={path} className="wiz-source-item">
                    <div className="wiz-source-path">{path}</div>
                    <button type="button" className="btn btn-ghost btn-sm" onClick={() => onRemoveSource(path)}>削除</button>
                  </div>
                )) : (
                  <div className="wiz-source-empty">まだ項目が選択されていません。</div>
                )}
              </div>
            </div>
          </div>
          <div className="wiz-foot">
            <span />
            <button type="button" className="btn btn-pri" onClick={() => onPatch({ step: 2 })} disabled={!wizard.sourcePaths.length}>次へ</button>
          </div>
        </div>
      ) : null}

      {wizard.step === 2 ? (
        <div className="wiz-panel active">
          <div className="wiz-panel-title">保存先の provider</div>
          <div className="wiz-panel-desc">保存先は各 provider の `.vault` repository に固定です。新規バックアップは snapshot として追加されます。</div>
          <div className="f-group">
            <div className="f">
              <div className="f-lbl">クラウドストレージ</div>
              <div className="remote-opts">
                {(["drive", "r2"] as ProviderId[]).map((provider) => (
                  <button
                    key={provider}
                    type="button"
                    className={`remote-opt ${wizard.provider === provider ? "sel" : ""}`}
                    onClick={() => onPatch({ provider })}
                  >
                    <div className="remote-opt-icon">{provider === "drive" ? "🔵" : "🟠"}</div>
                    <div className="remote-opt-name">{provider === "drive" ? "Google Drive" : "Cloudflare R2"}</div>
                    <div className="remote-opt-st">{providers[provider].status === "connected" ? "接続済み" : "未接続"}</div>
                  </button>
                ))}
              </div>
            </div>
          </div>
          <div className="wiz-foot">
            <button type="button" className="btn btn-ghost" onClick={() => onPatch({ step: 1 })}>戻る</button>
            <button type="button" className="btn btn-pri" onClick={() => onPatch({ step: 3 })}>次へ</button>
          </div>
        </div>
      ) : null}

      {wizard.step === 3 ? (
        <div className="wiz-panel active">
          <div className="wiz-panel-title">リポジトリパスワード</div>
          <div className="wiz-panel-desc">`.vault` repository はこのパスワードで保護されます。忘れると snapshot を復元できません。</div>
          <div className="f-group">
            <div className="f">
              <label className="f-lbl">パスワード</label>
              <input className="f-inp" type="password" value={wizard.password} onChange={(event) => onPatch({ password: event.target.value })} />
            </div>
            <div className="tog-row">
              <div>
                <div className="tog-title">キーチェーンに保存</div>
                <div className="tog-desc">macOS Keychain に repository password を安全に保存します。</div>
              </div>
              <label className="tog">
                <input type="checkbox" checked={wizard.useKeychain} onChange={(event) => onPatch({ useKeychain: event.target.checked })} />
                <span className="tog-t" />
              </label>
            </div>
          </div>
          <div className="wiz-foot">
            <button type="button" className="btn btn-ghost" onClick={() => onPatch({ step: 2 })}>戻る</button>
            <button type="button" className={`btn btn-pri btn-lg ${wizard.submitting ? "btn-spin" : ""}`} onClick={onSubmit} disabled={wizard.submitting || !wizard.password.trim()}>
              {iconByName("upload")}
              Snapshot 作成
            </button>
          </div>
        </div>
      ) : null}
    </section>
  );
}
