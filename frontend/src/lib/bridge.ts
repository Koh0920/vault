import { open } from "@tauri-apps/plugin-dialog";
import type {
  ConnectProviderResponse,
  CreateCryptRemoteResponse,
  ExplorerEntry,
  ExplorerMode,
  JobState,
  ProviderStatusInfo,
  RuntimeConfig,
  StartUploadResponse,
  UploadIndexEntry,
} from "../types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke?: (command: string, payload?: Record<string, unknown>) => Promise<unknown>;
    };
  }
}

function coreInvoke<T>(command: string, payload: Record<string, unknown> = {}): Promise<T> {
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  if (typeof invoke !== "function") {
    return Promise.reject(new Error("No Tauri invoke bridge detected."));
  }
  return invoke(command, payload) as Promise<T>;
}

export interface Bridge {
  hasBridge(): boolean;
  getRuntimeConfig(): Promise<RuntimeConfig>;
  pickFolders(): Promise<{ paths: string[] }>;
  pickFiles(): Promise<{ paths: string[] }>;
  getProviderStatuses(): Promise<{ providers: ProviderStatusInfo[] }>;
  listUploadIndex(): Promise<{ uploads: UploadIndexEntry[] }>;
  listExplorerEntries(
    uploadId: string,
    path: string,
    mode: ExplorerMode,
    query: string,
    offset: number,
    limit: number,
    refresh: boolean,
  ): Promise<{
    upload: UploadIndexEntry;
    currentPath: string;
    totalCount: number;
    nextOffset: number | null;
    entries: ExplorerEntry[];
    listedAt: string | null;
  }>;
  startDownloadExplorerItem(uploadId: string, path: string): Promise<{ jobId: string }>;
  startPreviewExplorerItem(uploadId: string, path: string): Promise<{ jobId: string }>;
  listJobs(kind?: string | null, status?: string | null, limit?: number): Promise<{ jobs: JobState[] }>;
  connectProvider(provider: string): Promise<ConnectProviderResponse>;
  createCryptRemote(baseRemote: string, suffix: string, remoteRootPath: string, password: string): Promise<CreateCryptRemoteResponse>;
  startUpload(sourcePath: string, remoteName: string, remotePath: string, mode?: string): Promise<StartUploadResponse>;
  getJobStatus(jobId: string): Promise<JobState>;
}

export function createBackupBridge(): Bridge {
  function normalizeDialogPaths(value: string | string[] | null): string[] {
    if (Array.isArray(value)) return value.filter((item): item is string => typeof item === "string");
    return typeof value === "string" ? [value] : [];
  }

  return {
    hasBridge() {
      return typeof window.__TAURI_INTERNALS__?.invoke === "function";
    },

    getRuntimeConfig() {
      return coreInvoke<RuntimeConfig>("get_runtime_config", {});
    },

    async pickFolders() {
      const result = await open({
        directory: true,
        multiple: true,
        title: "バックアップするフォルダを選択",
      });
      return {
        paths: normalizeDialogPaths(result),
      };
    },

    async pickFiles() {
      const result = await open({
        directory: false,
        multiple: true,
        title: "バックアップするファイルを選択",
      });
      return {
        paths: normalizeDialogPaths(result),
      };
    },

    getProviderStatuses() {
      return coreInvoke<{ providers: ProviderStatusInfo[] }>("get_provider_statuses", {});
    },

    listUploadIndex() {
      return coreInvoke<{ uploads: UploadIndexEntry[] }>("list_upload_index", {});
    },

    listExplorerEntries(uploadId, path, mode, query, offset, limit, refresh) {
      return coreInvoke("list_explorer_entries", {
        payload: {
          uploadId,
          path,
          mode,
          query,
          offset,
          limit,
          refresh,
        },
      });
    },

    startDownloadExplorerItem(uploadId, path) {
      return coreInvoke<{ jobId: string }>("start_download_explorer_item", {
        payload: { uploadId, path },
      });
    },

    startPreviewExplorerItem(uploadId, path) {
      return coreInvoke<{ jobId: string }>("start_preview_explorer_item", {
        payload: { uploadId, path },
      });
    },

    listJobs(kind = null, status = null, limit = 50) {
      return coreInvoke<{ jobs: JobState[] }>("list_jobs", {
        payload: { kind, status, limit },
      });
    },

    connectProvider(provider) {
      return coreInvoke<ConnectProviderResponse>("connect_provider", { payload: { provider } });
    },

    createCryptRemote(baseRemote, suffix, remoteRootPath, password) {
      return coreInvoke<CreateCryptRemoteResponse>("create_crypt_remote", {
        payload: {
          baseRemote,
          cryptSuffix: suffix,
          remoteRootPath,
          password,
        },
      });
    },

    startUpload(sourcePath, remoteName, remotePath, mode = "copy") {
      return coreInvoke<StartUploadResponse>("start_upload", {
        payload: { sourcePath, remoteName, remotePath, mode },
      });
    },

    getJobStatus(jobId) {
      return coreInvoke<JobState>("get_job_status", { payload: { jobId } });
    },
  };
}
