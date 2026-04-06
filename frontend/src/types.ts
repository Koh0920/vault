export type ViewName = "dashboard" | "remotes" | "history" | "explorer" | "wizard" | "settings";
export type ProviderId = "drive" | "r2";
export type JobKind = "upload" | "download" | "preview";
export type JobPhase = "queued" | "running" | "done" | "failed" | "canceled";

export interface RuntimeConfig {
  configPath: string;
  stateDir: string;
  rcAddr: string;
  defaultMode: string;
  useKeychain: boolean;
  jobPollIntervalMs: number;
  defaultCryptRemoteSuffix: string;
}

export interface ProviderStatusInfo {
  id: ProviderId;
  connected: boolean;
}

export interface ProviderState {
  status: "unknown" | "pending" | "connected" | "failed";
  meta?: string;
}

export interface ConnectProviderResponse {
  ok: boolean;
  provider: string;
  status: string;
  nextAction: string;
  configPath: string;
}

export interface InitVaultRepositoryResponse {
  repoId: string;
}

export interface StartVaultBackupResponse {
  jobId: string;
  executeId: string;
}

export interface VaultRepositoryInfo {
  repoId: string;
  provider: ProviderId;
  backendKind: string;
  repoLocator: string;
  displayName: string;
  createdAt: string;
  lastSnapshotAt: string | null;
}

export interface VaultSnapshotSummary {
  filesNew: number;
  filesChanged: number;
  filesUnmodified: number;
  dirsNew: number;
  totalFilesProcessed: number;
  totalBytesProcessed: number;
  dataAdded: number;
  dataAddedPacked: number;
}

export interface VaultSnapshotInfo {
  snapshotId: string;
  repoId: string;
  time: string;
  hostname: string;
  label: string;
  tags: string[];
  paths: string[];
  summary: VaultSnapshotSummary | null;
}

export interface VaultEntry {
  name: string;
  displayName: string;
  path: string;
  isDir: boolean;
  size: number;
  modTime: string | null;
  mimeType: string | null;
}

export interface PreviewResult {
  name: string;
  path: string;
  mimeType: string | null;
  kind: "image" | "text" | "unsupported";
  text: string | null;
  imageDataUrl: string | null;
  truncated: boolean;
  size: number;
}

export interface BackupJobResult {
  snapshotId: string;
  filesNew: number;
  filesChanged: number;
  filesUnchanged: number;
  dirsNew: number;
  totalBytesProcessed: number;
  totalBytesAdded: number;
}

export interface DownloadJobResult {
  savedPath: string;
}

export interface JobProgress {
  bytesDone: number;
  bytesTotal: number | null;
  speed: number | null;
  eta: number | null;
  currentFile: string | null;
  transfers: number | null;
}

export interface JobState {
  jobId: string;
  executeId: string;
  kind: JobKind;
  phase: JobPhase;
  progress: JobProgress;
  error: string | null;
  result: PreviewResult | BackupJobResult | DownloadJobResult | Record<string, never> | null;
  startedAt: string | null;
  finishedAt: string | null;
}

export interface ToastItem {
  id: string;
  type: "ok" | "err";
  message: string;
}

export interface WizardState {
  step: 1 | 2 | 3;
  sourcePaths: string[];
  provider: ProviderId;
  password: string;
  useKeychain: boolean;
  submitting: boolean;
}

export interface ExplorerPreviewState {
  status: "idle" | "loading" | "ready" | "error";
  requestId: number;
  jobId: string | null;
  path: string | null;
  data: PreviewResult | null;
  error: string | null;
  metaOpen: boolean;
}

export interface ExplorerState {
  provider: ProviderId;
  repositories: VaultRepositoryInfo[];
  selectedRepoId: string | null;
  snapshots: VaultSnapshotInfo[];
  selectedSnapshotId: string | null;
  currentPath: string;
  query: string;
  offset: number;
  limit: number;
  totalCount: number;
  nextOffset: number | null;
  listedAt: string | null;
  entries: VaultEntry[];
  loading: boolean;
  error: string | null;
  entriesLoading: boolean;
  entriesError: string | null;
  preview: ExplorerPreviewState;
}

export interface AppState {
  view: ViewName;
  bridgeReady: boolean;
  runtimeConfig: RuntimeConfig | null;
  providers: Record<ProviderId, ProviderState>;
  jobsById: Record<string, JobState>;
  jobOrder: string[];
  toasts: ToastItem[];
  wizard: WizardState;
  explorer: ExplorerState;
  debugOpen: boolean;
}
