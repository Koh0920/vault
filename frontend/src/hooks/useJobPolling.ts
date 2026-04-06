import { useEffect, useMemo, useRef } from "react";
import { useAppDispatch, useAppState, useToastActions } from "../context/AppContext";
import type { Bridge } from "../lib/bridge";
import type { JobState } from "../types";

interface PollingCallbacks {
  onUploadDone: () => void;
  onPreviewUpdate: (job: JobState) => void;
}

export function useJobPolling(bridge: Bridge, callbacks: PollingCallbacks) {
  const state = useAppState();
  const dispatch = useAppDispatch();
  const toast = useToastActions();
  const watchedJobIds = useRef(new Set<string>());
  const timerRef = useRef<number | null>(null);
  const notifiedStates = useRef(new Set<string>());

  const ensurePolling = useMemo(
    () => () => {
      if (timerRef.current || watchedJobIds.current.size === 0) return;
      timerRef.current = window.setInterval(async () => {
        const pending = [...watchedJobIds.current];
        await Promise.all(pending.map(async (jobId) => {
          try {
            const status = await bridge.getJobStatus(jobId);
            dispatch({ type: "upsert-jobs", jobs: [status] });
            callbacks.onPreviewUpdate(status);
            if (status.phase === "done" || status.phase === "failed" || status.phase === "canceled") {
              watchedJobIds.current.delete(jobId);
            }

            const noticeKey = `${status.jobId}:${status.phase}`;
            if (!notifiedStates.current.has(noticeKey) && (status.phase === "done" || status.phase === "failed")) {
              notifiedStates.current.add(noticeKey);
              if (status.kind === "upload") {
                if (status.phase === "done") {
                  callbacks.onUploadDone();
                  toast.push("バックアップが完了しました");
                } else {
                  toast.push(`バックアップが失敗しました: ${status.error ?? ""}`, "err");
                }
              } else if (status.kind === "download") {
                if (status.phase === "done") {
                  const savedPath = (status.result as { savedPath?: string } | null)?.savedPath ?? "";
                  toast.push(`Downloads に保存しました: ${savedPath}`);
                } else {
                  toast.push(`ダウンロードに失敗しました: ${status.error ?? ""}`, "err");
                }
              } else if (status.kind === "preview" && status.phase === "failed") {
                toast.push(`プレビューに失敗しました: ${status.error ?? ""}`, "err");
              }
            }
          } catch (error) {
            watchedJobIds.current.delete(jobId);
            const fallback: JobState = {
              jobId,
              executeId: state.jobsById[jobId]?.executeId ?? jobId,
              kind: state.jobsById[jobId]?.kind ?? "upload",
              phase: "failed",
              progress: state.jobsById[jobId]?.progress ?? {
                bytesDone: 0,
                bytesTotal: null,
                speed: null,
                eta: null,
                currentFile: jobId,
                transfers: null,
              },
              error: String(error),
              result: null,
              startedAt: state.jobsById[jobId]?.startedAt ?? new Date().toISOString(),
              finishedAt: new Date().toISOString(),
            };
            dispatch({ type: "upsert-jobs", jobs: [fallback] });
            callbacks.onPreviewUpdate(fallback);
          }
        }));

        if (watchedJobIds.current.size === 0 && timerRef.current) {
          window.clearInterval(timerRef.current);
          timerRef.current = null;
        }
      }, state.runtimeConfig?.jobPollIntervalMs ?? 1000);
    },
    [bridge, callbacks, dispatch, state.jobsById, state.runtimeConfig?.jobPollIntervalMs, toast],
  );

  useEffect(() => () => {
    if (timerRef.current) {
      window.clearInterval(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  return useMemo(
    () => ({
      watchJob(jobId: string) {
        watchedJobIds.current.add(jobId);
        ensurePolling();
      },
      async loadJobs() {
        const result = await bridge.listJobs(null, null, 100);
        dispatch({ type: "upsert-jobs", jobs: result.jobs ?? [] });
        result.jobs?.forEach((job) => {
          if (job.phase === "running") {
            watchedJobIds.current.add(job.jobId);
          }
        });
        ensurePolling();
      },
    }),
    [bridge, dispatch, ensurePolling],
  );
}
