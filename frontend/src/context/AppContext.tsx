import {
  createContext,
  useContext,
  useMemo,
  useReducer,
  type Dispatch,
  type PropsWithChildren,
} from "react";
import type {
  AppState,
  ExplorerEntry,
  ExplorerMode,
  JobState,
  PreviewResult,
  ProviderId,
  ProviderState,
  RuntimeConfig,
  ToastItem,
  UploadIndexEntry,
  ViewName,
} from "../types";

type Action =
  | { type: "set-view"; view: ViewName }
  | { type: "set-bridge-ready"; ready: boolean }
  | { type: "set-runtime-config"; runtimeConfig: RuntimeConfig | null }
  | { type: "set-provider"; provider: ProviderId; state: ProviderState }
  | { type: "set-providers-from-status"; providers: Array<{ id: ProviderId; connected: boolean }> }
  | { type: "upsert-jobs"; jobs: JobState[] }
  | { type: "push-toast"; toast: ToastItem }
  | { type: "remove-toast"; id: string }
  | { type: "patch-wizard"; patch: Partial<AppState["wizard"]> }
  | { type: "set-debug-open"; open: boolean }
  | { type: "patch-explorer"; patch: Partial<AppState["explorer"]> }
  | { type: "set-explorer-uploads"; uploads: UploadIndexEntry[]; selectedUploadId: string | null }
  | {
      type: "set-explorer-entries";
      entries: ExplorerEntry[];
      currentPath: string;
      totalCount: number;
      nextOffset: number | null;
      listedAt: string | null;
    }
  | { type: "set-explorer-preview-loading"; requestId: number; path: string }
  | { type: "set-explorer-preview-job"; requestId: number; jobId: string }
  | { type: "set-explorer-preview-ready"; jobId: string; data: PreviewResult }
  | { type: "set-explorer-preview-error"; jobId: string | null; error: string }
  | { type: "toggle-explorer-preview-meta" }
  | { type: "reset-explorer-preview" };

const initialState: AppState = {
  view: "dashboard",
  bridgeReady: false,
  runtimeConfig: null,
  providers: {
    drive: { status: "unknown" },
    r2: { status: "unknown" },
  },
  jobsById: {},
  jobOrder: [],
  toasts: [],
  wizard: {
    step: 1,
    sourcePaths: [],
    baseRemote: "drive",
    remotePath: "backup",
    password: "",
    useKeychain: true,
    submitting: false,
  },
  explorer: {
    provider: "drive",
    mode: "decrypted",
    uploads: [],
    selectedUploadId: null,
    currentPath: "",
    query: "",
    offset: 0,
    limit: 200,
    totalCount: 0,
    nextOffset: null,
    listedAt: null,
    entries: [],
    loading: false,
    error: null,
    entriesLoading: false,
    entriesError: null,
    preview: {
      status: "idle",
      requestId: 0,
      jobId: null,
      path: null,
      data: null,
      error: null,
      metaOpen: false,
    },
  },
  debugOpen: false,
};

function sortJobOrder(jobsById: Record<string, JobState>): string[] {
  return Object.values(jobsById)
    .sort((left, right) => {
      const leftTime = new Date(left.startedAt ?? left.finishedAt ?? 0).getTime();
      const rightTime = new Date(right.startedAt ?? right.finishedAt ?? 0).getTime();
      return rightTime - leftTime;
    })
    .map((job) => job.jobId);
}

function reducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case "set-view":
      return {
        ...state,
        view: action.view,
        explorer: action.view === "explorer"
          ? state.explorer
          : {
              ...state.explorer,
              preview: {
                ...state.explorer.preview,
                status: "idle",
                jobId: null,
                path: null,
                data: null,
                error: null,
                metaOpen: false,
              },
            },
      };
    case "set-bridge-ready":
      return { ...state, bridgeReady: action.ready };
    case "set-runtime-config":
      return { ...state, runtimeConfig: action.runtimeConfig };
    case "set-provider":
      return {
        ...state,
        providers: {
          ...state.providers,
          [action.provider]: action.state,
        },
      };
    case "set-providers-from-status": {
      const providers = { ...state.providers };
      action.providers.forEach((provider) => {
        providers[provider.id] = {
          status: provider.connected ? "connected" : "unknown",
          meta: provider.connected ? "保存済みの設定を読み込みました" : providers[provider.id]?.meta,
        };
      });
      return { ...state, providers };
    }
    case "upsert-jobs": {
      const jobsById = { ...state.jobsById };
      action.jobs.forEach((job) => {
        const normalized = job.kind === "preview"
          ? { ...job, result: null }
          : job;
        jobsById[job.jobId] = {
          ...jobsById[job.jobId],
          ...normalized,
          progress: {
            ...jobsById[job.jobId]?.progress,
            ...normalized.progress,
          },
        };
      });
      return { ...state, jobsById, jobOrder: sortJobOrder(jobsById) };
    }
    case "push-toast":
      return { ...state, toasts: [...state.toasts, action.toast] };
    case "remove-toast":
      return { ...state, toasts: state.toasts.filter((toast) => toast.id !== action.id) };
    case "patch-wizard":
      return { ...state, wizard: { ...state.wizard, ...action.patch } };
    case "set-debug-open":
      return { ...state, debugOpen: action.open };
    case "patch-explorer":
      return { ...state, explorer: { ...state.explorer, ...action.patch } };
    case "set-explorer-uploads":
      return {
        ...state,
        explorer: {
          ...state.explorer,
          uploads: action.uploads,
          selectedUploadId: action.selectedUploadId,
          currentPath: "",
          query: "",
          offset: 0,
          totalCount: 0,
          nextOffset: null,
          entries: [],
          error: null,
          entriesError: null,
          preview: {
            ...state.explorer.preview,
            status: "idle",
            jobId: null,
            path: null,
            data: null,
            error: null,
            metaOpen: false,
          },
        },
      };
    case "set-explorer-entries":
      return {
        ...state,
        explorer: {
          ...state.explorer,
          entries: action.entries,
          currentPath: action.currentPath,
          totalCount: action.totalCount,
          nextOffset: action.nextOffset,
          listedAt: action.listedAt,
          entriesLoading: false,
          entriesError: null,
        },
      };
    case "set-explorer-preview-loading":
      return {
        ...state,
        explorer: {
          ...state.explorer,
          preview: {
            status: "loading",
            requestId: action.requestId,
            jobId: null,
            path: action.path,
            data: null,
            error: null,
            metaOpen: false,
          },
        },
      };
    case "set-explorer-preview-job":
      if (state.explorer.preview.requestId !== action.requestId) return state;
      return {
        ...state,
        explorer: {
          ...state.explorer,
          preview: {
            ...state.explorer.preview,
            jobId: action.jobId,
          },
        },
      };
    case "set-explorer-preview-ready":
      if (state.explorer.preview.jobId !== action.jobId) return state;
      return {
        ...state,
        explorer: {
          ...state.explorer,
          preview: {
            ...state.explorer.preview,
            status: "ready",
            data: action.data,
            error: null,
            metaOpen: false,
          },
        },
      };
    case "set-explorer-preview-error":
      if (action.jobId && state.explorer.preview.jobId !== action.jobId) return state;
      return {
        ...state,
        explorer: {
          ...state.explorer,
          preview: {
            ...state.explorer.preview,
            status: "error",
            error: action.error,
            data: null,
            metaOpen: false,
          },
        },
      };
    case "toggle-explorer-preview-meta":
      return {
        ...state,
        explorer: {
          ...state.explorer,
          preview: {
            ...state.explorer.preview,
            metaOpen: !state.explorer.preview.metaOpen,
          },
        },
      };
    case "reset-explorer-preview":
      return {
        ...state,
        explorer: {
          ...state.explorer,
          preview: {
            ...state.explorer.preview,
            status: "idle",
            jobId: null,
            path: null,
            data: null,
            error: null,
            metaOpen: false,
          },
        },
      };
    default:
      return state;
  }
}

const AppStateContext = createContext<AppState | null>(null);
const AppDispatchContext = createContext<Dispatch<Action> | null>(null);

export function AppProvider({ children }: PropsWithChildren) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const stateValue = useMemo(() => state, [state]);
  return (
    <AppStateContext.Provider value={stateValue}>
      <AppDispatchContext.Provider value={dispatch}>
        {children}
      </AppDispatchContext.Provider>
    </AppStateContext.Provider>
  );
}

export function useAppState() {
  const value = useContext(AppStateContext);
  if (!value) throw new Error("AppStateContext is not available");
  return value;
}

export function useAppDispatch() {
  const value = useContext(AppDispatchContext);
  if (!value) throw new Error("AppDispatchContext is not available");
  return value;
}

export function useToastActions() {
  const dispatch = useAppDispatch();
  return useMemo(
    () => ({
      push(message: string, type: ToastItem["type"] = "ok") {
        dispatch({
          type: "push-toast",
          toast: {
            id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
            type,
            message,
          },
        });
      },
      remove(id: string) {
        dispatch({ type: "remove-toast", id });
      },
    }),
    [dispatch],
  );
}
