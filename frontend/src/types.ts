export interface RuntimeConfig {
  listenAddr?: string;
  stateDir?: string;
  rcloneBinary?: string;
  googleRedirectUri?: string;
  defaultCryptRemoteSuffix?: string;
  ok?: boolean;
}

export interface DriveStatus {
  ok: boolean;
  connected: boolean;
}

export interface VaultStatus {
  ok: boolean;
  exists?: boolean;
  vaultId?: string | null;
  keyFingerprint?: string | null;
  initialized?: boolean;
}

export interface InitializeResponse {
  ok: boolean;
  vaultId: string;
  recoveryKey: string;
  keyFingerprint: string;
}

export interface UnlockResponse {
  ok: boolean;
  vaultId: string;
  keyFingerprint: string;
  recoveryKey: string;
}

export interface ObjectEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  modTime?: string | null;
  mimeType?: string | null;
  encrypted?: string | null;
}

export interface ListFilesResponse {
  ok: boolean;
  path: string;
  entries: ObjectEntry[];
}

export interface PreviewResponse {
  ok: boolean;
  path: string;
  mimeType: string;
  text: string | null;
  size: number;
}

export interface JobProgress {
  bytesDone: number;
  bytesTotal?: number | null;
  speed?: number | null;
  eta?: number | null;
  currentFile?: string | null;
}

export interface JobStatus {
  jobId: string;
  kind: string;
  phase: string;
  progress: JobProgress;
  error?: string | null;
  result?: unknown;
  startedAt?: string | null;
  finishedAt?: string | null;
}

export interface UploadResult {
  name: string;
  ok: boolean;
  error?: string;
}