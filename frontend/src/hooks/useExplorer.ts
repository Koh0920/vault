import { useDeferredValue, useMemo, useRef } from "react";
import { useAppDispatch, useAppState, useToastActions } from "../context/AppContext";
import type { Bridge } from "../lib/bridge";
import type { ExplorerMode, ProviderId, UploadIndexEntry } from "../types";

function pickSelection(uploads: UploadIndexEntry[], provider: ProviderId, selectedUploadId: string | null) {
  const providerUploads = uploads.filter((entry) => entry.provider === provider);
  if (providerUploads.some((entry) => entry.uploadId === selectedUploadId)) {
    return selectedUploadId;
  }
  return providerUploads[0]?.uploadId ?? null;
}

export function useExplorer(bridge: Bridge, watchJob: (jobId: string) => void) {
  const state = useAppState();
  const dispatch = useAppDispatch();
  const toast = useToastActions();
  const requestRef = useRef(0);
  const deferredQuery = useDeferredValue(state.explorer.query);

  async function loadEntries(overrides?: Partial<{
    provider: ProviderId;
    mode: ExplorerMode;
    selectedUploadId: string | null;
    currentPath: string;
    query: string;
    offset: number;
    refresh: boolean;
  }>) {
    const provider = overrides?.provider ?? state.explorer.provider;
    const mode = overrides?.mode ?? state.explorer.mode;
    const uploads = state.explorer.uploads;
    const selectedUploadId = overrides?.selectedUploadId ?? state.explorer.selectedUploadId;
    const currentPath = overrides?.currentPath ?? state.explorer.currentPath;
    const query = overrides?.query ?? deferredQuery;
    const offset = overrides?.offset ?? state.explorer.offset;
    const refresh = overrides?.refresh ?? false;
    const selectedUpload = uploads.find((entry) => entry.uploadId === selectedUploadId && entry.provider === provider) ?? null;

    dispatch({
      type: "patch-explorer",
      patch: {
        provider,
        mode,
        selectedUploadId,
        currentPath,
        query,
        offset,
        entriesLoading: true,
        entriesError: null,
      },
    });

    if (!selectedUpload) {
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

    if (selectedUpload.itemType === "file" && !currentPath) {
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
      const result = await bridge.listExplorerEntries(
        selectedUpload.uploadId,
        currentPath,
        mode,
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

  return useMemo(
    () => ({
      explorer: state.explorer,
      providerUploads() {
        return state.explorer.uploads.filter((entry) => entry.provider === state.explorer.provider);
      },
      selectedUpload() {
        return state.explorer.uploads.find((entry) => entry.uploadId === state.explorer.selectedUploadId) ?? null;
      },
      async loadIndex() {
        dispatch({ type: "patch-explorer", patch: { loading: true, error: null } });
        try {
          const result = await bridge.listUploadIndex();
          const selectedUploadId = pickSelection(result.uploads ?? [], state.explorer.provider, state.explorer.selectedUploadId);
          dispatch({
            type: "set-explorer-uploads",
            uploads: result.uploads ?? [],
            selectedUploadId,
          });
          await loadEntries({ selectedUploadId, currentPath: "", query: "", offset: 0, refresh: true });
        } catch (error) {
          dispatch({ type: "patch-explorer", patch: { error: String(error) } });
        } finally {
          dispatch({ type: "patch-explorer", patch: { loading: false } });
        }
      },
      async refresh() {
        await loadEntries({ refresh: true });
      },
      async setProvider(provider: ProviderId) {
        const selectedUploadId = pickSelection(state.explorer.uploads, provider, null);
        dispatch({
          type: "patch-explorer",
          patch: {
            provider,
            selectedUploadId,
            currentPath: "",
            query: "",
            offset: 0,
          },
        });
        dispatch({ type: "reset-explorer-preview" });
        await loadEntries({ provider, selectedUploadId, currentPath: "", query: "", offset: 0, refresh: true });
      },
      async setMode(mode: ExplorerMode) {
        dispatch({
          type: "patch-explorer",
          patch: {
            mode,
            offset: 0,
          },
        });
        dispatch({ type: "reset-explorer-preview" });
        await loadEntries({ mode, offset: 0, refresh: true });
      },
      async selectUpload(uploadId: string) {
        dispatch({
          type: "patch-explorer",
          patch: {
            selectedUploadId: uploadId,
            currentPath: "",
            query: "",
            offset: 0,
          },
        });
        dispatch({ type: "reset-explorer-preview" });
        await loadEntries({ selectedUploadId: uploadId, currentPath: "", query: "", offset: 0, refresh: true });
      },
      async openDirectory(path: string) {
        dispatch({
          type: "patch-explorer",
          patch: {
            currentPath: path,
            offset: 0,
          },
        });
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
        await loadEntries({ offset });
      },
      async setQuery(query: string) {
        dispatch({ type: "patch-explorer", patch: { query, offset: 0 } });
        await loadEntries({ query, offset: 0 });
      },
      async startPreview(path = "") {
        const selectedUpload = state.explorer.uploads.find((entry) => entry.uploadId === state.explorer.selectedUploadId) ?? null;
        if (!selectedUpload) return;
        const requestId = state.explorer.preview.requestId + 1;
        dispatch({ type: "set-explorer-preview-loading", requestId, path });
        try {
          const result = await bridge.startPreviewExplorerItem(selectedUpload.uploadId, path);
          dispatch({ type: "set-explorer-preview-job", requestId, jobId: result.jobId });
          watchJob(result.jobId);
        } catch (error) {
          dispatch({ type: "set-explorer-preview-error", jobId: null, error: String(error) });
          toast.push(`プレビューに失敗しました: ${String(error)}`, "err");
        }
      },
      async startDownload(path = "") {
        const selectedUpload = state.explorer.uploads.find((entry) => entry.uploadId === state.explorer.selectedUploadId) ?? null;
        if (!selectedUpload) return;
        try {
          const result = await bridge.startDownloadExplorerItem(selectedUpload.uploadId, path);
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
                currentFile: path || selectedUpload.displayName,
                transfers: null,
              },
              error: null,
              result: null,
              startedAt: new Date().toISOString(),
              finishedAt: null,
            }],
          });
          watchJob(result.jobId);
          toast.push("ダウンロードを開始しました");
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
    [bridge, deferredQuery, dispatch, state.explorer, state.explorer.preview.requestId, state.explorer.selectedUploadId, state.explorer.uploads, toast, watchJob],
  );
}
