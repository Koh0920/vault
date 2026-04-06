import { useDeferredValue, useMemo, useRef } from "react";
import { useAppDispatch, useAppState, useToastActions } from "../context/AppContext";
import type { Bridge } from "../lib/bridge";
import type { ProviderId, VaultRepositoryInfo, VaultSnapshotInfo } from "../types";

function pickRepository(
  repositories: VaultRepositoryInfo[],
  provider: ProviderId,
  selectedRepoId: string | null,
) {
  const providerRepositories = repositories.filter((repository) => repository.provider === provider);
  if (providerRepositories.some((repository) => repository.repoId === selectedRepoId)) {
    return selectedRepoId;
  }
  return providerRepositories[0]?.repoId ?? null;
}

function pickSnapshot(snapshots: VaultSnapshotInfo[], selectedSnapshotId: string | null) {
  if (snapshots.some((snapshot) => snapshot.snapshotId === selectedSnapshotId)) {
    return selectedSnapshotId;
  }
  return snapshots[0]?.snapshotId ?? null;
}

export function useExplorer(bridge: Bridge, watchJob: (jobId: string) => void) {
  const state = useAppState();
  const dispatch = useAppDispatch();
  const toast = useToastActions();
  const requestRef = useRef(0);
  const deferredQuery = useDeferredValue(state.explorer.query);

  async function loadEntries(overrides?: Partial<{
    repoId: string | null;
    snapshotId: string | null;
    currentPath: string;
    query: string;
    offset: number;
    refresh: boolean;
  }>) {
    const repoId = overrides?.repoId ?? state.explorer.selectedRepoId;
    const snapshotId = overrides?.snapshotId ?? state.explorer.selectedSnapshotId;
    const currentPath = overrides?.currentPath ?? state.explorer.currentPath;
    const query = overrides?.query ?? deferredQuery;
    const offset = overrides?.offset ?? state.explorer.offset;
    const refresh = overrides?.refresh ?? false;

    dispatch({
      type: "patch-explorer",
      patch: {
        currentPath,
        query,
        offset,
        entriesLoading: true,
        entriesError: null,
      },
    });

    if (!repoId || !snapshotId) {
      dispatch({
        type: "patch-explorer",
        patch: {
          entries: [],
          entriesLoading: false,
          totalCount: 0,
          nextOffset: null,
          listedAt: null,
        },
      });
      return;
    }

    const requestId = ++requestRef.current;
    try {
      const result = await bridge.listVaultEntries(
        repoId,
        snapshotId,
        currentPath,
        query,
        offset,
        state.explorer.limit,
        refresh,
      );
      if (requestId !== requestRef.current) return;
      dispatch({
        type: "set-explorer-entries",
        entries: result.entries ?? [],
        currentPath: result.currentPath ?? currentPath,
        totalCount: result.totalCount ?? 0,
        nextOffset: result.nextOffset ?? null,
        listedAt: result.listedAt ?? null,
      });
    } catch (error) {
      if (requestId !== requestRef.current) return;
      dispatch({
        type: "patch-explorer",
        patch: {
          entries: [],
          entriesLoading: false,
          entriesError: String(error),
        },
      });
    }
  }

  async function loadSnapshots(overrides?: Partial<{ repoId: string | null; selectedSnapshotId: string | null; refresh: boolean }>) {
    const repoId = overrides?.repoId ?? state.explorer.selectedRepoId;
    if (!repoId) {
      dispatch({
        type: "set-explorer-snapshots",
        snapshots: [],
        selectedSnapshotId: null,
      });
      return;
    }

    dispatch({ type: "patch-explorer", patch: { loading: true, error: null } });
    try {
      const result = await bridge.listVaultSnapshots(repoId, 100);
      const selectedSnapshotId = pickSnapshot(result.snapshots ?? [], overrides?.selectedSnapshotId ?? state.explorer.selectedSnapshotId);
      dispatch({
        type: "set-explorer-snapshots",
        snapshots: result.snapshots ?? [],
        selectedSnapshotId,
      });
      await loadEntries({
        repoId,
        snapshotId: selectedSnapshotId,
        currentPath: "",
        query: "",
        offset: 0,
        refresh: overrides?.refresh ?? true,
      });
    } catch (error) {
      dispatch({
        type: "patch-explorer",
        patch: {
          loading: false,
          error: String(error),
          snapshots: [],
          selectedSnapshotId: null,
          entries: [],
          totalCount: 0,
          nextOffset: null,
          listedAt: null,
        },
      });
      return;
    }
    dispatch({ type: "patch-explorer", patch: { loading: false } });
  }

  async function loadRepositories() {
    dispatch({ type: "patch-explorer", patch: { loading: true, error: null } });
    try {
      const result = await bridge.listVaultRepositories();
      const repositories = result.repositories ?? [];
      const selectedRepoId = pickRepository(repositories, state.explorer.provider, state.explorer.selectedRepoId);
      dispatch({
        type: "set-explorer-repositories",
        repositories,
        selectedRepoId,
      });
      if (selectedRepoId) {
        await loadSnapshots({ repoId: selectedRepoId, selectedSnapshotId: null, refresh: true });
      } else {
        dispatch({ type: "patch-explorer", patch: { loading: false } });
      }
    } catch (error) {
      dispatch({ type: "patch-explorer", patch: { loading: false, error: String(error) } });
    }
  }

  return useMemo(
    () => ({
      explorer: state.explorer,
      providerRepositories() {
        return state.explorer.repositories.filter((repository) => repository.provider === state.explorer.provider);
      },
      selectedRepository() {
        return state.explorer.repositories.find((repository) => repository.repoId === state.explorer.selectedRepoId) ?? null;
      },
      selectedSnapshot() {
        return state.explorer.snapshots.find((snapshot) => snapshot.snapshotId === state.explorer.selectedSnapshotId) ?? null;
      },
      loadRepositories,
      async refresh() {
        await loadRepositories();
      },
      async setProvider(provider: ProviderId) {
        const selectedRepoId = pickRepository(state.explorer.repositories, provider, null);
        dispatch({
          type: "patch-explorer",
          patch: {
            provider,
            selectedRepoId,
            error: null,
          },
        });
        dispatch({ type: "reset-explorer-preview" });
        if (selectedRepoId) {
          await loadSnapshots({ repoId: selectedRepoId, selectedSnapshotId: null, refresh: false });
        } else {
          dispatch({
            type: "set-explorer-snapshots",
            snapshots: [],
            selectedSnapshotId: null,
          });
          dispatch({
            type: "patch-explorer",
            patch: {
              entries: [],
              totalCount: 0,
              nextOffset: null,
              listedAt: null,
            },
          });
        }
      },
      async selectRepository(repoId: string) {
        dispatch({
          type: "patch-explorer",
          patch: {
            selectedRepoId: repoId,
          },
        });
        dispatch({ type: "reset-explorer-preview" });
        await loadSnapshots({ repoId, selectedSnapshotId: null, refresh: true });
      },
      async selectSnapshot(snapshotId: string) {
        dispatch({
          type: "patch-explorer",
          patch: {
            selectedSnapshotId: snapshotId,
            currentPath: "",
            query: "",
            offset: 0,
          },
        });
        dispatch({ type: "reset-explorer-preview" });
        await loadEntries({ snapshotId, currentPath: "", query: "", offset: 0, refresh: true });
      },
      async openDirectory(path: string) {
        dispatch({ type: "patch-explorer", patch: { currentPath: path, offset: 0 } });
        dispatch({ type: "reset-explorer-preview" });
        await loadEntries({ currentPath: path, offset: 0, refresh: true });
      },
      async goUp() {
        if (!state.explorer.currentPath) return;
        const parts = state.explorer.currentPath.split("/").filter(Boolean);
        parts.pop();
        const path = parts.join("/");
        dispatch({ type: "patch-explorer", patch: { currentPath: path, offset: 0 } });
        dispatch({ type: "reset-explorer-preview" });
        await loadEntries({ currentPath: path, offset: 0, refresh: true });
      },
      async goPage(direction: "prev" | "next") {
        const offset = direction === "next"
          ? state.explorer.nextOffset ?? state.explorer.offset
          : Math.max(0, state.explorer.offset - state.explorer.limit);
        dispatch({ type: "patch-explorer", patch: { offset } });
        await loadEntries({ offset, refresh: false });
      },
      async setQuery(query: string) {
        dispatch({ type: "patch-explorer", patch: { query, offset: 0 } });
        await loadEntries({ query, offset: 0, refresh: false });
      },
      async startPreview(path = "") {
        const repoId = state.explorer.selectedRepoId;
        const snapshotId = state.explorer.selectedSnapshotId;
        if (!repoId || !snapshotId) return;
        const requestId = state.explorer.preview.requestId + 1;
        dispatch({ type: "set-explorer-preview-loading", requestId, path });
        try {
          const result = await bridge.startVaultPreview(repoId, snapshotId, path);
          dispatch({ type: "set-explorer-preview-job", requestId, jobId: result.jobId });
          watchJob(result.jobId);
        } catch (error) {
          dispatch({ type: "set-explorer-preview-error", jobId: null, error: String(error) });
          toast.push(`プレビューに失敗しました: ${String(error)}`, "err");
        }
      },
      async startDownload(path = "") {
        const repoId = state.explorer.selectedRepoId;
        const snapshotId = state.explorer.selectedSnapshotId;
        if (!repoId || !snapshotId) return;
        try {
          const result = await bridge.startVaultRestore(repoId, snapshotId, path);
          dispatch({
            type: "upsert-jobs",
            jobs: [{
              jobId: result.jobId,
              executeId: result.jobId,
              kind: "download",
              phase: "running",
              progress: {
                bytesDone: 0,
                bytesTotal: null,
                speed: null,
                eta: null,
                currentFile: path || repoId,
                transfers: null,
              },
              error: null,
              result: null,
              startedAt: new Date().toISOString(),
              finishedAt: null,
            }],
          });
          watchJob(result.jobId);
          toast.push("復元を開始しました");
        } catch (error) {
          toast.push(`ダウンロードに失敗しました: ${String(error)}`, "err");
        }
      },
      handlePreviewJob(jobId: string, phase: "done" | "failed", result: unknown, error: string | null) {
        if (phase === "done" && result) {
          dispatch({ type: "set-explorer-preview-ready", jobId, data: result as never });
          return;
        }
        dispatch({
          type: "set-explorer-preview-error",
          jobId,
          error: error ?? "プレビューに失敗しました",
        });
      },
      togglePreviewMeta() {
        dispatch({ type: "toggle-explorer-preview-meta" });
      },
      resetPreview() {
        dispatch({ type: "reset-explorer-preview" });
      },
    }),
    [bridge, deferredQuery, dispatch, loadRepositories, state.explorer, toast, watchJob],
  );
}
