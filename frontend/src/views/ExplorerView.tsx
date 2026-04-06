import { iconByName } from "../components/Icons";
import {
  explorerKindLabel,
  explorerTypeLabel,
  fileGlyphLabel,
  formatBytes,
  formatTime,
  snapshotLabel,
} from "../lib/format";
import type {
  ExplorerState,
  ProviderId,
  VaultEntry,
  VaultRepositoryInfo,
  VaultSnapshotInfo,
} from "../types";

interface ExplorerViewProps {
  explorer: ExplorerState;
  providerRepositories: VaultRepositoryInfo[];
  selectedRepository: VaultRepositoryInfo | null;
  selectedSnapshot: VaultSnapshotInfo | null;
  onProviderChange: (provider: ProviderId) => void;
  onRefresh: () => void;
  onSelectRepository: (repoId: string) => void;
  onSelectSnapshot: (snapshotId: string) => void;
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
  const { explorer, providerRepositories, selectedRepository, selectedSnapshot } = props;
  const pathLabel = selectedRepository
    ? `${selectedRepository.repoLocator}:${explorer.currentPath || "/"}` : ".vault";
  const segments = explorer.currentPath.split("/").filter(Boolean);
  const breadcrumbs = [{ label: selectedSnapshot ? snapshotLabel(selectedSnapshot) : "latest", path: "" }].concat(
    segments.map((segment, index) => ({
      label: segment,
      path: segments.slice(0, index + 1).join("/"),
    })),
  );

  return (
    <section className="view active">
      <div className="sec-hd">
        <div className="sec-eye">Vault Explorer</div>
        <h1 className="sec-title">スナップショット閲覧</h1>
        <p className="sec-sub">provider ごとの `.vault` repository を開き、latest または過去 snapshot を browse / preview / restore できます。</p>
      </div>

      <div className="exp-shell">
        <ExplorerHeader
          pathLabel={pathLabel}
          provider={explorer.provider}
          selectedSnapshot={selectedSnapshot}
          onProviderChange={props.onProviderChange}
          onRefresh={props.onRefresh}
        />
        <ExplorerRepositories
          repositories={providerRepositories}
          selectedRepoId={explorer.selectedRepoId}
          onSelectRepository={props.onSelectRepository}
        />
        <ExplorerCurrent
          selectedRepository={selectedRepository}
          snapshots={explorer.snapshots}
          selectedSnapshot={selectedSnapshot}
          onSelectSnapshot={props.onSelectSnapshot}
        />
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
                  <input className="exp-search" value={explorer.query} onChange={(event) => props.onQueryChange(event.target.value)} placeholder="現在のフォルダ内を検索" />
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
                explorer={explorer}
                selectedRepository={selectedRepository}
                selectedSnapshot={selectedSnapshot}
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
              selectedRepository={selectedRepository}
              selectedSnapshot={selectedSnapshot}
              currentPath={explorer.currentPath}
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
  selectedSnapshot,
  onProviderChange,
  onRefresh,
}: {
  pathLabel: string;
  provider: ProviderId;
  selectedSnapshot: VaultSnapshotInfo | null;
  onProviderChange: (provider: ProviderId) => void;
  onRefresh: () => void;
}) {
  return (
    <div className="exp-headbar">
      <div className="exp-pathline">
        <div className="exp-drive-dot">VLT</div>
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
          <div className="exp-current-badge">{selectedSnapshot ? snapshotLabel(selectedSnapshot) : "snapshot なし"}</div>
        </div>
        <button type="button" className="btn btn-ghost btn-sm" onClick={onRefresh}>{iconByName("refresh")}再読み込み</button>
      </div>
    </div>
  );
}

function ExplorerRepositories({
  repositories,
  selectedRepoId,
  onSelectRepository,
}: {
  repositories: VaultRepositoryInfo[];
  selectedRepoId: string | null;
  onSelectRepository: (repoId: string) => void;
}) {
  if (!repositories.length) {
    return <div className="exp-roots"><div className="exp-empty">まだ `.vault` repository がありません。まず snapshot backup を実行してください。</div></div>;
  }

  return (
    <div className="exp-roots">
      {repositories.map((repository) => (
        <button
          key={repository.repoId}
          type="button"
          className={`exp-root ${repository.repoId === selectedRepoId ? "active" : ""}`}
          onClick={() => onSelectRepository(repository.repoId)}
        >
          <span className="exp-root-icon"><span className="file-glyph sm folder">▣</span></span>
          <span className="exp-root-copy">
            <span className="exp-root-title">{repository.displayName}</span>
            <span className="exp-root-meta">{repository.backendKind} · {formatTime(repository.lastSnapshotAt ?? repository.createdAt)}</span>
          </span>
        </button>
      ))}
    </div>
  );
}

function ExplorerCurrent({
  selectedRepository,
  snapshots,
  selectedSnapshot,
  onSelectSnapshot,
}: {
  selectedRepository: VaultRepositoryInfo | null;
  snapshots: VaultSnapshotInfo[];
  selectedSnapshot: VaultSnapshotInfo | null;
  onSelectSnapshot: (snapshotId: string) => void;
}) {
  if (!selectedRepository) {
    return <div className="exp-current"><div className="exp-current-empty">表示する repository を選択してください。</div></div>;
  }

  return (
    <div className="exp-current">
      <div>
        <div className="exp-current-kicker">Active Repository</div>
        <div className="exp-current-title">{selectedRepository.displayName}</div>
        <div className="exp-current-meta">{selectedRepository.repoLocator}</div>
        <div className="exp-current-meta mono">latest snapshot: {formatTime(selectedRepository.lastSnapshotAt)}</div>
      </div>
      <div className="exp-current-side" style={{ minWidth: 260 }}>
        <label className="f-lbl" style={{ marginBottom: 8 }}>Snapshot</label>
        <select
          className="f-inp"
          value={selectedSnapshot?.snapshotId ?? ""}
          onChange={(event) => onSelectSnapshot(event.target.value)}
          disabled={!snapshots.length}
        >
          {!snapshots.length ? <option value="">snapshot なし</option> : null}
          {snapshots.map((snapshot) => (
            <option key={snapshot.snapshotId} value={snapshot.snapshotId}>
              {snapshotLabel(snapshot)}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}

function ExplorerList({
  explorer,
  selectedRepository,
  selectedSnapshot,
  onOpenDirectory,
  onPreview,
  onDownload,
}: {
  explorer: ExplorerState;
  selectedRepository: VaultRepositoryInfo | null;
  selectedSnapshot: VaultSnapshotInfo | null;
  onOpenDirectory: (path: string) => void;
  onPreview: (path: string) => void;
  onDownload: (path: string) => void;
}) {
  if (!selectedRepository || !selectedSnapshot) {
    return <div className="exp-empty">表示する repository と snapshot を選択してください。</div>;
  }
  if (explorer.entriesLoading) return <div className="exp-empty">snapshot 内容を読み込み中…</div>;
  if (explorer.entriesError) return <div className="exp-empty">一覧の取得に失敗しました: {explorer.entriesError}</div>;
  if (!explorer.entries.length) return <div className="exp-empty">この条件に一致する項目はありません。</div>;
  return (
    <>
      {explorer.entries.map((entry) => (
        <ExplorerRow
          key={entry.path}
          entry={entry}
          onOpenDirectory={onOpenDirectory}
          onPreview={onPreview}
          onDownload={onDownload}
        />
      ))}
    </>
  );
}

function ExplorerRow({
  entry,
  onOpenDirectory,
  onPreview,
  onDownload,
}: {
  entry: VaultEntry;
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
        <button type="button" className="btn btn-ghost btn-sm" onClick={() => onDownload(entry.path)}>復元</button>
      </div>
    </div>
  );
}

function ExplorerPreview({
  preview,
  selectedRepository,
  selectedSnapshot,
  currentPath,
  onToggleMeta,
  onDownload,
}: {
  preview: ExplorerState["preview"];
  selectedRepository: VaultRepositoryInfo | null;
  selectedSnapshot: VaultSnapshotInfo | null;
  currentPath: string;
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
  const absolutePath = [selectedRepository?.repoLocator ?? ".vault", currentPath, data.path].filter(Boolean).join("/");

  return (
    <div className="exp-preview">
      <div className="exp-preview-card">
        <div className="exp-preview-top">
          <div className="exp-preview-title-wrap">
            <div className="exp-preview-title">{data.name}</div>
            <div className="exp-preview-meta">
              {data.kind === "text" && data.truncated
                ? "先頭のみ表示しています"
                : selectedSnapshot
                  ? snapshotLabel(selectedSnapshot)
                  : "preview"}
            </div>
          </div>
          <div className="exp-actions">
            <button type="button" className="btn btn-ghost btn-sm" onClick={onToggleMeta}>{preview.metaOpen ? "閉じる" : "情報"}</button>
            <button type="button" className="btn btn-sec btn-sm" onClick={() => onDownload(data.path)}>復元</button>
          </div>
        </div>
        {preview.metaOpen ? (
          <div className="exp-preview-grid">
            <PreviewStat label="場所" value={absolutePath} mono />
            <PreviewStat label="種別" value={explorerKindLabel(data)} />
            <PreviewStat label="サイズ" value={formatBytes(data.size)} />
            <PreviewStat label="Snapshot" value={selectedSnapshot ? snapshotLabel(selectedSnapshot) : "—"} />
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
    <div>
      <div className="exp-stat-label">{label}</div>
      <div className={`exp-stat-value ${mono ? "mono" : ""}`}>{value}</div>
    </div>
  );
}
