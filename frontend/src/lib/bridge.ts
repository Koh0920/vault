import { open } from "@tauri-apps/plugin-dialog";
import type {
  ConnectProviderResponse,
  InitVaultRepositoryResponse,
  JobState,
  ProviderStatusInfo,
  RuntimeConfig,
  StartVaultBackupResponse,
  VaultEntry,
  VaultRepositoryInfo,
  VaultSnapshotInfo,
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
  connectProvider(provider: string): Promise<ConnectProviderResponse>;
  initVaultRepository(provider: string, password: string, useKeychain: boolean): Promise<InitVaultRepositoryResponse>;
  startVaultBackup(provider: string, sourcePaths: string[]): Promise<StartVaultBackupResponse>;
  listVaultRepositories(): Promise<{ repositories: VaultRepositoryInfo[] }>;
  listVaultSnapshots(repoId: string, limit?: number): Promise<{ snapshots: VaultSnapshotInfo[] }>;
  listVaultEntries(
    repoId: string,
    snapshotId: string,
    path: string,
    query: string,
    offset: number,
    limit: number,
    refresh: boolean,
  ): Promise<{
    repository: VaultRepositoryInfo;
    snapshot: VaultSnapshotInfo;
    currentPath: string;
    totalCount: number;
    nextOffset: number | null;
    entries: VaultEntry[];
    listedAt: string | null;
  }>;
  startVaultRestore(repoId: string, snapshotId: string, path: string): Promise<{ jobId: string }>;
  startVaultPreview(repoId: string, snapshotId: string, path: string): Promise<{ jobId: string }>;
  listJobs(kind?: string | null, status?: string | null, limit?: number): Promise<{ jobs: JobState[] }>;
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
      return { paths: normalizeDialogPaths(result) };
    },

    async pickFiles() {
      const result = await open({
        directory: false,
        multiple: true,
        title: "バックアップするファイルを選択",
      });
      return { paths: normalizeDialogPaths(result) };
    },

    getProviderStatuses() {
      return coreInvoke<{ providers: ProviderStatusInfo[] }>("get_provider_statuses", {});
    },

    connectProvider(provider) {
      return coreInvoke<ConnectProviderResponse>("connect_provider", { payload: { provider } });
    },

    initVaultRepository(provider, password, useKeychain) {
      return coreInvoke<InitVaultRepositoryResponse>("init_vault_repository", {
        payload: { provider, password, useKeychain },
      });
    },

    startVaultBackup(provider, sourcePaths) {
      return coreInvoke<StartVaultBackupResponse>("start_vault_backup", {
        payload: { provider, sourcePaths },
      });
    },

    listVaultRepositories() {
      return coreInvoke<{ repositories: VaultRepositoryInfo[] }>("list_vault_repositories", {});
    },

    listVaultSnapshots(repoId, limit = 100) {
      return coreInvoke<{ snapshots: VaultSnapshotInfo[] }>("list_vault_snapshots", {
        payload: { repoId, limit },
      });
    },

    listVaultEntries(repoId, snapshotId, path, query, offset, limit, refresh) {
      return coreInvoke("list_vault_entries", {
        payload: { repoId, snapshotId, path, query, offset, limit, refresh },
      });
    },

    startVaultRestore(repoId, snapshotId, path) {
      return coreInvoke<{ jobId: string }>("start_vault_restore", {
        payload: { repoId, snapshotId, path },
      });
    },

    startVaultPreview(repoId, snapshotId, path) {
      return coreInvoke<{ jobId: string }>("start_vault_preview", {
        payload: { repoId, snapshotId, path },
      });
    },

    listJobs(kind = null, status = null, limit = 50) {
      return coreInvoke<{ jobs: JobState[] }>("list_jobs", {
        payload: { kind, status, limit },
      });
    },

    getJobStatus(jobId) {
      return coreInvoke<JobState>("get_job_status", { payload: { jobId } });
    },
  };
}
