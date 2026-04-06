import { useEffect, useMemo, useRef } from "react";
import { AppShell } from "./components/AppShell";
import { JobBar } from "./components/JobBar";
import { ToastLayer } from "./components/ToastLayer";
import { AppProvider, useAppDispatch, useAppState, useToastActions } from "./context/AppContext";
import { useExplorer } from "./hooks/useExplorer";
import { useJobPolling } from "./hooks/useJobPolling";
import { useProviders } from "./hooks/useProviders";
import { createBackupBridge } from "./lib/bridge";
import { historyJobs } from "./lib/format";
import { ConnectionsView } from "./views/ConnectionsView";
import { DashboardView } from "./views/DashboardView";
import { ExplorerView } from "./views/ExplorerView";
import { HistoryView } from "./views/HistoryView";
import { SettingsView } from "./views/SettingsView";
import { WizardView } from "./views/WizardView";

const bridge = createBackupBridge();

function AppInner() {
  const state = useAppState();
  const dispatch = useAppDispatch();
  const toast = useToastActions();
  const bootedRef = useRef(false);

  const explorer = useExplorer(bridge, (jobId) => jobPolling.watchJob(jobId));
  const jobPolling = useJobPolling(bridge, {
    onUploadDone: () => {
      void explorer.loadRepositories();
    },
    onPreviewUpdate: (job) => {
      if (job.kind === "preview" && (job.phase === "done" || job.phase === "failed")) {
        explorer.handlePreviewJob(job.jobId, job.phase, job.result, job.error);
      }
    },
  });
  const providers = useProviders(bridge);

  useEffect(() => {
    if (bootedRef.current) return;
    bootedRef.current = true;
    async function boot() {
      dispatch({ type: "set-bridge-ready", ready: bridge.hasBridge() });
      if (!bridge.hasBridge()) return;

      try {
        const runtimeConfig = await bridge.getRuntimeConfig();
        dispatch({ type: "set-runtime-config", runtimeConfig });
      } catch {
        dispatch({ type: "set-runtime-config", runtimeConfig: null });
      }

      await providers.loadStatuses();
      await jobPolling.loadJobs().catch(() => {});
      await explorer.loadRepositories().catch(() => {});
    }
    void boot();
  }, []);

  const runningJob = useMemo(() => {
    const jobs = historyJobs(state.jobsById, state.jobOrder);
    return jobs.find((job) => job.phase === "running") ?? null;
  }, [state.jobOrder, state.jobsById]);

  const renderedHistory = useMemo(
    () => historyJobs(state.jobsById, state.jobOrder),
    [state.jobOrder, state.jobsById],
  );

  async function startBackup() {
    const sourcePaths = Array.from(
      new Set(state.wizard.sourcePaths.map((path) => path.trim()).filter(Boolean)),
    );

    if (!sourcePaths.length) {
      toast.push("バックアップ対象を1件以上選択してください", "err");
      return;
    }
    if (!state.wizard.password.trim()) {
      toast.push("リポジトリパスワードを入力してください", "err");
      return;
    }

    dispatch({ type: "patch-wizard", patch: { submitting: true } });
    try {
      await bridge.initVaultRepository(
        state.wizard.provider,
        state.wizard.password,
        state.wizard.useKeychain,
      );
      const upload = await bridge.startVaultBackup(state.wizard.provider, sourcePaths);
      dispatch({
        type: "upsert-jobs",
        jobs: [{
          jobId: upload.jobId,
          executeId: upload.executeId,
          kind: "upload",
          phase: "running",
          progress: {
            bytesDone: 0,
            bytesTotal: null,
            speed: null,
            eta: null,
            currentFile: sourcePaths.length === 1 ? sourcePaths[0] : `${sourcePaths.length} items`,
            transfers: null,
          },
          error: null,
          result: null,
          startedAt: new Date().toISOString(),
          finishedAt: null,
        }],
      });
      jobPolling.watchJob(upload.jobId);
      dispatch({ type: "set-view", view: "dashboard" });
      toast.push("スナップショットバックアップを開始しました");
      await explorer.loadRepositories().catch(() => {});
    } catch (error) {
      toast.push(String(error), "err");
    } finally {
      dispatch({ type: "patch-wizard", patch: { submitting: false } });
    }
  }

  function mergeSources(paths: string[]) {
    const merged = Array.from(
      new Set([...state.wizard.sourcePaths, ...paths.map((path) => path.trim()).filter(Boolean)]),
    );
    dispatch({ type: "patch-wizard", patch: { sourcePaths: merged } });
  }

  async function pickFolders() {
    try {
      const result = await bridge.pickFolders();
      if (!result.paths.length) return;
      mergeSources(result.paths);
    } catch (error) {
      toast.push(`フォルダ選択に失敗しました: ${String(error)}`, "err");
    }
  }

  async function pickFiles() {
    try {
      const result = await bridge.pickFiles();
      if (!result.paths.length) return;
      mergeSources(result.paths);
    } catch (error) {
      toast.push(`ファイル選択に失敗しました: ${String(error)}`, "err");
    }
  }

  function removeSource(path: string) {
    dispatch({
      type: "patch-wizard",
      patch: { sourcePaths: state.wizard.sourcePaths.filter((item) => item !== path) },
    });
  }

  function clearSources() {
    dispatch({ type: "patch-wizard", patch: { sourcePaths: [] } });
  }

  const connectedCount = Object.values(state.providers).filter((provider) => provider.status === "connected").length;

  return (
    <>
      <AppShell
        currentView={state.view}
        connectedCount={connectedCount}
        bridgeReady={state.bridgeReady}
        onChangeView={(view) => dispatch({ type: "set-view", view })}
      >
        {state.view === "dashboard" ? (
          <DashboardView
            providers={state.providers}
            onConnect={providers.connect}
            onStartBackup={() => {
              dispatch({ type: "set-view", view: "wizard" });
              dispatch({ type: "patch-wizard", patch: { step: 1 } });
            }}
          />
        ) : null}

        {state.view === "remotes" ? (
          <ConnectionsView providers={state.providers} onConnect={providers.connect} />
        ) : null}

        {state.view === "history" ? (
          <HistoryView jobs={renderedHistory} />
        ) : null}

        {state.view === "explorer" ? (
          <ExplorerView
            explorer={state.explorer}
            providerRepositories={explorer.providerRepositories()}
            selectedRepository={explorer.selectedRepository()}
            selectedSnapshot={explorer.selectedSnapshot()}
            onProviderChange={(provider) => void explorer.setProvider(provider)}
            onRefresh={() => void explorer.refresh()}
            onSelectRepository={(repoId) => void explorer.selectRepository(repoId)}
            onSelectSnapshot={(snapshotId) => void explorer.selectSnapshot(snapshotId)}
            onBreadcrumb={(path) => void explorer.openDirectory(path)}
            onUp={() => void explorer.goUp()}
            onQueryChange={(query) => void explorer.setQuery(query)}
            onPage={(direction) => void explorer.goPage(direction)}
            onOpenDirectory={(path) => void explorer.openDirectory(path)}
            onPreview={(path) => void explorer.startPreview(path)}
            onDownload={(path) => void explorer.startDownload(path)}
            onTogglePreviewMeta={explorer.togglePreviewMeta}
          />
        ) : null}

        {state.view === "wizard" ? (
          <WizardView
            wizard={state.wizard}
            providers={state.providers}
            onBackToDashboard={() => dispatch({ type: "set-view", view: "dashboard" })}
            onPatch={(patch) => dispatch({ type: "patch-wizard", patch })}
            onSubmit={() => void startBackup()}
            onPickFolders={() => void pickFolders()}
            onPickFiles={() => void pickFiles()}
            onRemoveSource={removeSource}
            onClearSources={clearSources}
          />
        ) : null}

        {state.view === "settings" ? (
          <SettingsView
            appState={state}
            onToggleDebug={() => dispatch({ type: "set-debug-open", open: !state.debugOpen })}
          />
        ) : null}
      </AppShell>

      <JobBar job={runningJob} />
      <ToastLayer toasts={state.toasts} />
    </>
  );
}

export default function App() {
  return (
    <AppProvider>
      <AppInner />
    </AppProvider>
  );
}
