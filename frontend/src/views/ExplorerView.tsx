import { explorerKindLabel, explorerTypeLabel, fileGlyphLabel, formatBytes, formatTime } from "../lib/format";
import type { ExplorerEntry, ExplorerMode, ExplorerState, PreviewResult, ProviderId, UploadIndexEntry } from "../types";
import { iconByName } from "../components/Icons";

interface ExplorerViewProps {
  explorer: ExplorerState;
  providerUploads: UploadIndexEntry[];
  selectedUpload: UploadIndexEntry | null;
  onProviderChange: (provider: ProviderId) => void;
  onModeChange: (mode: ExplorerMode) => void;
  onRefresh: () => void;
  onSelectUpload: (uploadId: string) => void;
  onBreadcrumb: (path: string) => void;
  onUp: () => void;
  onQueryChange: (query: string) => void;
  onPage: (direction: "prev" | "next") => void;
  onOpenDirectory: (path: string) => void;
  onPreview: (path: string) => void;
  onDownload: (path: string) => void;
  onTogglePreviewMeta: () => void;
}

export function ExplorerView(props: ExplorerViewProps) {
  const { explorer, providerUploads, selectedUpload } = props;
  const modeLabel = explorer.mode === "encrypted" ? "暗号化名" : "復号名";
  const uploadBasePath = selectedUpload
    ? [selectedUpload.remoteRootPath, selectedUpload.remoteItemPath].filter(Boolean).join("/")
    : "";
  const pathLabel = selectedUpload
    ? `${selectedUpload.viewCryptRemote}:${uploadBasePath}${explorer.currentPath ? `/${explorer.currentPath}` : ""}`
    : "remote:path";
  const segments = explorer.currentPath.split("/").filter(Boolean);
  const breadcrumbs = [{ label: selectedUpload?.displayName ?? "root", path: "" }].concat(
    segments.map((segment, index) => ({
      label: segment,
      path: segments.slice(0, index + 1).join("/"),
    })),
  );

  return (
    <section className="view active">
      <div className="sec-hd">
        <div className="sec-eye">Explorer</div>
        <h1 className="sec-title">アップロード済みファイル</h1>
        <p className="sec-sub">このアプリが完了記録したアップロード項目を、provider と表示モードで切り替えて確認します。</p>
      </div>

      <div className="exp-shell">
        <ExplorerHeader
          pathLabel={pathLabel}
          provider={explorer.provider}
          mode={explorer.mode}
          onProviderChange={props.onProviderChange}
          onModeChange={props.onModeChange}
          onRefresh={props.onRefresh}
        />
        <ExplorerRoots roots={providerUploads} selectedUploadId={explorer.selectedUploadId} onSelectUpload={props.onSelectUpload} />
        <ExplorerCurrent selectedUpload={selectedUpload} modeLabel={modeLabel} onPreview={props.onPreview} onDownload={props.onDownload} />
        <div className="exp-workspace">
          <div className="exp-pane exp-pane-list">
            <div className="exp-card">
              <div className="exp-browser-bar">
                <div className="exp-breadcrumbs">
                  {breadcrumbs.map((crumb, index) => (
                    <span key={crumb.path}>
                      <button type="button" className={`exp-crumb ${index === breadcrumbs.length - 1 ? "active" : ""}`} onClick={() => props.onBreadcrumb(crumb.path)}>
                        {crumb.label}
                      </button>
                      {index < breadcrumbs.length - 1 ? <span className="exp-crumb-sep">/</span> : null}
                    </span>
                  ))}
                </div>
                <div className="exp-browser-tools">
                  <input className="exp-search" value={explorer.query} onChange={(event) => props.onQueryChange(event.target.value)} placeholder="このフォルダ内を検索" />
                  <button type="button" className="btn btn-ghost btn-sm" onClick={props.onUp} disabled={!explorer.currentPath}>1階層上へ</button>
                </div>
              </div>
              <div className="exp-browser-bar tight">
                <div className="exp-listed-at">{explorer.listedAt ? `最終一覧取得: ${formatTime(explorer.listedAt)}` : ""}</div>
              </div>
              <div className="exp-head">
                <div>項目</div>
                <div>種別</div>
                <div>サイズ</div>
                <div>更新時刻</div>
                <div>操作</div>
              </div>
              <ExplorerList
                selectedUpload={selectedUpload}
                explorer={explorer}
                onOpenDirectory={props.onOpenDirectory}
                onPreview={props.onPreview}
                onDownload={props.onDownload}
              />
              <div className="exp-foot">
                <div className="exp-page">
                  {explorer.totalCount ? `${explorer.offset + 1}-${Math.min(explorer.totalCount, explorer.offset + explorer.entries.length)} / ${explorer.totalCount}` : "0 / 0"}
                </div>
                <div className="exp-actions">
                  <button type="button" className="btn btn-ghost btn-sm" onClick={() => props.onPage("prev")} disabled={explorer.offset === 0}>前へ</button>
                  <button type="button" className="btn btn-ghost btn-sm" onClick={() => props.onPage("next")} disabled={explorer.nextOffset == null}>次へ</button>
                </div>
              </div>
            </div>
          </div>
          <div className="exp-pane exp-pane-preview">
            <ExplorerPreview
              preview={explorer.preview}
              modeLabel={modeLabel}
              pathLabel={pathLabel}
              onToggleMeta={props.onTogglePreviewMeta}
              onDownload={props.onDownload}
            />
          </div>
        </div>
      </div>
    </section>
  );
}

function ExplorerHeader({
  pathLabel,
  provider,
  mode,
  onProviderChange,
  onModeChange,
  onRefresh,
}: {
  pathLabel: string;
  provider: ProviderId;
  mode: ExplorerMode;
  onProviderChange: (provider: ProviderId) => void;
  onModeChange: (mode: ExplorerMode) => void;
  onRefresh: () => void;
}) {
  return (
    <div className="exp-headbar">
      <div className="exp-pathline">
        <div className="exp-drive-dot">EXP</div>
        <div className="exp-pathcopy">
          <div className="exp-pathlabel">Current Path</div>
          <div className="exp-pathvalue" title={pathLabel}>{pathLabel}</div>
        </div>
      </div>
      <div className="exp-toolbar">
        <div className="exp-groups">
          <div className="exp-seg">
            <button type="button" className={`exp-seg-btn ${provider === "drive" ? "active" : ""}`} onClick={() => onProviderChange("drive")}>Google Drive</button>
            <button type="button" className={`exp-seg-btn ${provider === "r2" ? "active" : ""}`} onClick={() => onProviderChange("r2")}>Cloudflare R2</button>
          </div>
          <div className="exp-seg">
            <button type="button" className={`exp-seg-btn ${mode === "encrypted" ? "active" : ""}`} onClick={() => onModeChange("encrypted")}>暗号化後</button>
            <button type="button" className={`exp-seg-btn ${mode === "decrypted" ? "active" : ""}`} onClick={() => onModeChange("decrypted")}>復号後</button>
          </div>
        </div>
        <button type="button" className="btn btn-ghost btn-sm" onClick={onRefresh}>{iconByName("refresh")}再読み込み</button>
      </div>
    </div>
  );
}

function ExplorerRoots({
  roots,
  selectedUploadId,
  onSelectUpload,
}: {
  roots: UploadIndexEntry[];
  selectedUploadId: string | null;
  onSelectUpload: (uploadId: string) => void;
}) {
  if (!roots.length) {
    return <div className="exp-roots"><div className="exp-empty">まずバックアップを完了すると、ここに起点フォルダが表示されます。</div></div>;
  }
  return (
    <div className="exp-roots">
      {roots.map((entry) => {
        const type = explorerTypeLabel(entry);
        return (
          <button key={entry.uploadId} type="button" className={`exp-root ${entry.uploadId === selectedUploadId ? "active" : ""}`} onClick={() => onSelectUpload(entry.uploadId)}>
            <span className="exp-root-icon"><span className={`file-glyph sm ${type}`}>{fileGlyphLabel(type)}</span></span>
            <span className="exp-root-copy">
              <span className="exp-root-title">{entry.displayName}</span>
              <span className="exp-root-meta">{explorerKindLabel(entry)} · {formatTime(entry.uploadedAt)}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}

function ExplorerCurrent({
  selectedUpload,
  modeLabel,
  onPreview,
  onDownload,
}: {
  selectedUpload: UploadIndexEntry | null;
  modeLabel: string;
  onPreview: (path: string) => void;
  onDownload: (path: string) => void;
}) {
  if (!selectedUpload) {
    return <div className="exp-current"><div className="exp-current-empty">表示するアップロード起点を選択してください。</div></div>;
  }
  return (
    <div className="exp-current">
      <div>
        <div className="exp-current-kicker">Active Root</div>
        <div className="exp-current-title">{selectedUpload.displayName}</div>
        <div className="exp-current-meta">{selectedUpload.sourcePath}</div>
        <div className="exp-current-meta mono">
          {selectedUpload.viewCryptRemote}:
          {[selectedUpload.remoteRootPath, selectedUpload.remoteItemPath].filter(Boolean).join("/")}
        </div>
      </div>
      <div className="exp-current-side">
        <div className="exp-current-badge">{modeLabel}</div>
        <div className="exp-actions">
          {selectedUpload.itemType === "file" ? <button type="button" className="btn btn-sec btn-sm" onClick={() => onPreview("")}>プレビュー</button> : null}
          <button type="button" className="btn btn-ghost btn-sm" onClick={() => onDownload("")}>ダウンロード</button>
        </div>
      </div>
    </div>
  );
}

function ExplorerList({
  selectedUpload,
  explorer,
  onOpenDirectory,
  onPreview,
  onDownload,
}: {
  selectedUpload: UploadIndexEntry | null;
  explorer: ExplorerState;
  onOpenDirectory: (path: string) => void;
  onPreview: (path: string) => void;
  onDownload: (path: string) => void;
}) {
  if (!selectedUpload) {
    return <div className="exp-empty">表示するアップロード起点を選択してください。</div>;
  }
  if (selectedUpload.itemType === "file" && !explorer.currentPath) {
    return <div className="exp-empty">このアップロードは単体ファイルです。プレビューまたはダウンロードを実行してください。</div>;
  }
  if (explorer.entriesLoading) return <div className="exp-empty">フォルダ内容を読み込み中…</div>;
  if (explorer.entriesError) return <div className="exp-empty">一覧の取得に失敗しました: {explorer.entriesError}</div>;
  if (!explorer.entries.length) return <div className="exp-empty">この条件に一致する項目はありません。</div>;
  return (
    <>
      {explorer.entries.map((entry) => <ExplorerRow key={entry.path} entry={entry} onOpenDirectory={onOpenDirectory} onPreview={onPreview} onDownload={onDownload} />)}
    </>
  );
}

function ExplorerRow({
  entry,
  onOpenDirectory,
  onPreview,
  onDownload,
}: {
  entry: ExplorerEntry;
  onOpenDirectory: (path: string) => void;
  onPreview: (path: string) => void;
  onDownload: (path: string) => void;
}) {
  const type = explorerTypeLabel(entry);
  return (
    <div className="exp-row">
      <div className="exp-name">
        <div className="exp-icon"><span className={`file-glyph sm ${type}`}>{fileGlyphLabel(type)}</span></div>
        <div style={{ minWidth: 0 }}>
          <div className="exp-title">{entry.displayName || entry.name}</div>
          <div className="exp-sub">{entry.path}</div>
        </div>
      </div>
      <div className="exp-cell"><span className={`exp-chip ${entry.isDir ? "dir" : "file"}`}>{explorerKindLabel(entry)}</span></div>
      <div className="exp-cell">{formatBytes(entry.size)}</div>
      <div className="exp-cell">{formatTime(entry.modTime)}</div>
      <div className="exp-actions">
        {entry.isDir ? (
          <button type="button" className="btn btn-sec btn-sm" onClick={() => onOpenDirectory(entry.path)}>開く</button>
        ) : (
          <button type="button" className="btn btn-sec btn-sm" onClick={() => onPreview(entry.path)}>プレビュー</button>
        )}
        <button type="button" className="btn btn-ghost btn-sm" onClick={() => onDownload(entry.path)}>ダウンロード</button>
      </div>
    </div>
  );
}

function ExplorerPreview({
  preview,
  modeLabel,
  pathLabel,
  onToggleMeta,
  onDownload,
}: {
  preview: ExplorerState["preview"];
  modeLabel: string;
  pathLabel: string;
  onToggleMeta: () => void;
  onDownload: (path: string) => void;
}) {
  if (preview.status === "loading") {
    return <div className="exp-preview"><div className="exp-preview-card empty">プレビューを読み込み中…</div></div>;
  }
  if (preview.status === "error") {
    return <div className="exp-preview"><div className="exp-preview-card empty">{preview.error}</div></div>;
  }
  if (preview.status !== "ready" || !preview.data) {
    return <div className="exp-preview"><div className="exp-preview-card empty">ファイルを選択するとプレビューが表示されます</div></div>;
  }

  const data = preview.data;
  const type = explorerTypeLabel(data);

  return (
    <div className="exp-preview">
      <div className="exp-preview-card">
        <div className="exp-preview-top">
          <div className="exp-preview-title-wrap">
            <div className="exp-preview-title">{data.name}</div>
            <div className="exp-preview-meta">{data.kind === "text" && data.truncated ? "先頭のみ表示しています" : modeLabel}</div>
          </div>
          <div className="exp-actions">
            <button type="button" className="btn btn-ghost btn-sm" onClick={onToggleMeta}>{preview.metaOpen ? "閉じる" : "情報"}</button>
            <button type="button" className="btn btn-sec btn-sm" onClick={() => onDownload(data.path)}>ダウンロード</button>
          </div>
        </div>
        {preview.metaOpen ? (
          <div className="exp-preview-grid">
            <PreviewStat label="場所" value={pathLabel} mono />
            <PreviewStat label="種別" value={explorerKindLabel(data)} />
            <PreviewStat label="サイズ" value={formatBytes(data.size)} />
            <PreviewStat label="表示" value={data.kind === "unsupported" ? "未対応" : modeLabel} />
          </div>
        ) : null}
        {data.kind === "image" && data.imageDataUrl ? (
          <div className="exp-preview-canvas image">
            <img className="exp-preview-image" src={data.imageDataUrl} alt={data.name} />
          </div>
        ) : data.kind === "text" ? (
          <div className="exp-preview-canvas">
            <pre className="exp-preview-text">{data.text ?? ""}</pre>
          </div>
        ) : (
          <div className="exp-preview-canvas unsupported">
            <div className="exp-preview-placeholder">
              <span className={`file-glyph lg ${type}`}>{fileGlyphLabel(type)}</span>
              <span>プレビューできません</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function PreviewStat({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="exp-preview-stat">
      <span className="exp-preview-label">{label}</span>
      <span className={`exp-preview-value ${mono ? "mono" : ""}`}>{value}</span>
    </div>
  );
}
