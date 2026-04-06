import { createBackupBridge } from "./bridge.js";

const bridge = createBackupBridge();
const watchedJobIds = new Set();
const notifiedJobStates = new Set();
let explorerSearchTimer = null;

const state = {
  view: "dashboard",
  bridge: false,
  runtimeConfig: null,
  providers: {
    drive: { status: "unknown" },
    r2: { status: "unknown" },
  },
  wizard: {
    step: 1,
    sourcePath: "",
    baseRemote: "drive",
    remotePath: "backup",
    password: "",
    useKeychain: true,
  },
  jobsById: {},
  jobOrder: [],
  pollTimer: null,
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
    preview: null,
    previewJobId: null,
    previewLoading: false,
    previewError: null,
    previewMetaOpen: false,
  },
};

const $ = (id) => document.getElementById(id);
const on = (id, ev, fn) => $(`${id}`)?.addEventListener(ev, fn);

function escapeHtml(value = "") {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function formatBytes(bytes) {
  if (bytes == null || Number.isNaN(Number(bytes))) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = Number(bytes);
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value >= 10 || unitIndex === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
}

function formatExplorerTime(value) {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString("ja-JP");
}

function toast(msg, type = "ok") {
  const el = document.createElement("div");
  el.className = `toast ${type}`;
  const icon = type === "ok"
    ? `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="#0de8a3" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>`
    : `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="#f87171" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>`;
  el.innerHTML = `${icon}<span>${msg}</span>`;
  $("toasts")?.appendChild(el);
  setTimeout(() => el.remove(), 3200);
}

function showView(name) {
  document.querySelectorAll(".view").forEach((el) => el.classList.remove("active"));
  document.querySelectorAll(".nav-item[data-view]").forEach((el) => el.classList.remove("active"));
  $(`view-${name}`)?.classList.add("active");
  document.querySelector(`.nav-item[data-view="${name}"]`)?.classList.add("active");
  state.view = name;
  if (name === "explorer") renderExplorer();
}

function setProvider(id, status, meta) {
  const badge = $(`${id}-badge`);
  const metaEl = $(`${id}-meta`);
  const card = $(`card-${id}`);
  const btn = $(`btn-connect-${id}`);
  state.providers[id].status = status;

  if (badge) {
    badge.className = "sbadge";
    if (status === "connected") {
      badge.classList.add("ok");
      badge.textContent = "接続済み";
      card?.classList.add("connected");
      if (btn) {
        btn.textContent = "✓ 接続済み";
        btn.disabled = true;
      }
    } else if (status === "pending") {
      badge.classList.add("pend");
      badge.textContent = "接続中…";
    } else if (status === "failed") {
      badge.classList.add("err");
      badge.textContent = "エラー";
      card?.classList.remove("connected");
      if (btn) btn.disabled = false;
    } else {
      badge.classList.add("off");
      badge.textContent = id === "drive" ? "未接続" : "未設定";
      card?.classList.remove("connected");
    }
  }

  if (meta && metaEl) metaEl.textContent = meta;
  const wizardState = $(`wos-${id}`);
  if (wizardState) wizardState.textContent = status === "connected" ? "接続済み" : "未接続";

  const connected = Object.values(state.providers).filter((provider) => provider.status === "connected").length;
  const navBadge = $("nav-badge");
  if (navBadge) {
    navBadge.style.display = connected > 0 ? "" : "none";
    navBadge.textContent = String(connected);
  }

  renderRml();
}

async function connectProvider(id) {
  setProvider(id, "pending", id === "drive" ? "ブラウザで認証中…" : "設定を確認中…");
  try {
    const res = await bridge.connectProvider(id);
    if (res.ok) {
      setProvider(id, "connected", res.nextAction ?? "接続済み");
      toast(`${id === "drive" ? "Google Drive" : "Cloudflare R2"} の接続が完了しました`);
    } else {
      setProvider(id, "failed", res.nextAction ?? res.status);
      toast(res.nextAction ?? "接続に失敗しました", "err");
    }
  } catch (error) {
    setProvider(id, "failed", String(error));
    toast(String(error), "err");
  }
}

async function pickFolder() {
  try {
    const result = await bridge.pickFolder();
    if (!result?.path) return;
    state.wizard.sourcePath = result.path;
    const input = $("wiz-src");
    if (input) input.value = result.path;
  } catch (error) {
    toast(`フォルダ選択に失敗しました: ${String(error)}`, "err");
  }
}

function renderRml() {
  const list = $("rml");
  if (!list) return;
  const items = [
    { id: "drive", label: "Google Drive", icon: "🔵", bg: "rgba(66,133,244,.1)" },
    { id: "r2", label: "Cloudflare R2", icon: "🟠", bg: "rgba(246,130,31,.1)" },
  ];
  list.innerHTML = items.map((item) => {
    const ok = state.providers[item.id].status === "connected";
    return `
      <div class="rml-row">
        <div class="rml-icon" style="background:${item.bg}">${item.icon}</div>
        <div style="flex:1">
          <div class="rml-name">${item.label}</div>
          <div class="rml-meta">${ok ? "接続済み" : "未接続"}</div>
        </div>
        <span class="sbadge ${ok ? "ok" : "off"}">${ok ? "接続済み" : "未接続"}</span>
        <button class="btn btn-sec btn-sm" data-rml="${item.id}" ${ok ? "disabled" : ""}>${ok ? "✓" : "接続"}</button>
      </div>`;
  }).join("");
  list.querySelectorAll("[data-rml]").forEach((el) => {
    el.addEventListener("click", () => connectProvider(el.dataset.rml));
  });
}

function goStep(n) {
  state.wizard.step = n;
  [1, 2, 3].forEach((i) => {
    $(`wp${i}`)?.classList.toggle("active", i === n);
    const indicator = $(`wsi${i}`);
    if (!indicator) return;
    indicator.classList.remove("active", "done");
    if (i === n) indicator.classList.add("active");
    else if (i < n) indicator.classList.add("done");
  });
}

function selectWizRemote(id) {
  state.wizard.baseRemote = id;
  document.querySelectorAll(".remote-opt").forEach((el) => {
    el.classList.toggle("sel", el.dataset.remote === id);
  });
}

function rememberJobs(jobs) {
  jobs.forEach((job) => {
    const normalized = job.kind === "preview"
      ? {
          ...job,
          result: null,
        }
      : job;
    state.jobsById[job.jobId] = {
      progress: {},
      ...state.jobsById[job.jobId],
      ...normalized,
      progress: {
        ...state.jobsById[job.jobId]?.progress,
        ...normalized.progress,
      },
    };
  });

  state.jobOrder = Object.values(state.jobsById)
    .sort((left, right) => {
      const leftTime = new Date(left.startedAt ?? left.finishedAt ?? 0).getTime();
      const rightTime = new Date(right.startedAt ?? right.finishedAt ?? 0).getTime();
      return rightTime - leftTime;
    })
    .map((job) => job.jobId);
}

function historyJobs() {
  return state.jobOrder
    .map((jobId) => state.jobsById[jobId])
    .filter((job) => job && job.kind !== "preview");
}

function currentRunningJob() {
  return historyJobs().find((job) => job.phase === "running") ?? null;
}

function renderJobBar() {
  const job = currentRunningJob();
  const bar = $("job-bar");
  if (!bar) return;

  if (!job) {
    bar.classList.remove("vis");
    return;
  }

  bar.classList.add("vis");
  $("jb-file").textContent = job.progress?.currentFile ?? job.progress?.current_item ?? job.jobId;

  const fill = $("jb-fill");
  if (fill) {
    if (job.progress?.bytesTotal > 0) {
      fill.classList.remove("indeterminate");
      fill.style.width = `${Math.min(100, Math.round((job.progress.bytesDone / job.progress.bytesTotal) * 100))}%`;
      fill.style.transform = "";
    } else {
      fill.classList.add("indeterminate");
      fill.style.width = "";
    }
  }

  $("jb-speed").textContent = job.progress?.speed != null
    ? `${(job.progress.speed / 1024 / 1024).toFixed(1)} MB/s`
    : "—";

  $("jb-eta").textContent = job.progress?.eta != null
    ? (job.progress.eta > 3600
      ? `ETA ${Math.floor(job.progress.eta / 3600)}h${Math.floor((job.progress.eta % 3600) / 60)}m`
      : job.progress.eta > 60
        ? `ETA ${Math.floor(job.progress.eta / 60)}m${job.progress.eta % 60}s`
        : `ETA ${job.progress.eta}s`)
    : "ETA —";

  const phase = $("jb-phase");
  if (phase) {
    phase.className = "phase-badge";
    phase.classList.add("run");
    phase.textContent = job.kind === "download" ? "ダウンロード中" : "実行中";
  }
}

function renderHistory() {
  const list = $("hist-list");
  if (!list) return;
  const jobs = historyJobs();
  if (!jobs.length) {
    list.innerHTML = `<p style="color:var(--t3);font-size:13px;margin-top:12px">まだバックアップを実行していません。</p>`;
    return;
  }

  list.innerHTML = jobs.map((job) => `
    <div class="hist-row">
      <span class="phase-badge ${job.phase === "done" ? "done" : job.phase === "failed" ? "fail" : "run"}">
        ${job.phase === "done" ? "完了" : job.phase === "failed" ? "失敗" : "実行中"}
      </span>
      <div style="flex:1;min-width:0">
        <div style="font-size:12.5px;font-weight:500;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
          ${escapeHtml(job.progress?.currentFile ?? job.jobId)}
        </div>
        <div style="font-size:11px;color:var(--t2);margin-top:2px">
          ${escapeHtml(formatExplorerTime(job.startedAt ?? job.finishedAt))}
        </div>
      </div>
      <div style="font-family:'JetBrains Mono',monospace;font-size:11px;color:var(--t3)">${job.kind}</div>
    </div>
  `).join("");
}

function setPreviewResult(job) {
  if (state.explorer.previewJobId !== job.jobId) return;
  if (job.phase === "done") {
    state.explorer.preview = job.result ?? null;
    state.explorer.previewError = null;
    state.explorer.previewLoading = false;
    state.explorer.previewMetaOpen = false;
  } else if (job.phase === "failed") {
    state.explorer.preview = null;
    state.explorer.previewError = job.error ?? "プレビューに失敗しました";
    state.explorer.previewLoading = false;
  }
}

function notifyJobTransition(job) {
  const noticeKey = `${job.jobId}:${job.phase}`;
  if (notifiedJobStates.has(noticeKey)) return;
  if (!["done", "failed"].includes(job.phase)) return;
  notifiedJobStates.add(noticeKey);

  if (job.kind === "upload") {
    if (job.phase === "done") {
      void loadExplorerIndex();
      toast("バックアップが完了しました");
    } else {
      toast(`バックアップが失敗しました: ${job.error ?? ""}`, "err");
    }
    return;
  }

  if (job.kind === "download") {
    if (job.phase === "done") {
      toast(`Downloads に保存しました: ${job.result?.savedPath ?? ""}`);
    } else {
      toast(`ダウンロードに失敗しました: ${job.error ?? ""}`, "err");
    }
    return;
  }

  if (job.kind === "preview" && job.phase === "failed") {
    toast(`プレビューに失敗しました: ${job.error ?? ""}`, "err");
  }
}

function applyJobUpdate(job) {
  rememberJobs([job]);
  setPreviewResult(job);
  notifyJobTransition(job);
  renderHistory();
  renderJobBar();
  if (state.view === "explorer") renderExplorer();
}

async function pollJobs() {
  if (!watchedJobIds.size) {
    if (state.pollTimer) clearInterval(state.pollTimer);
    state.pollTimer = null;
    return;
  }

  const pending = [...watchedJobIds];
  await Promise.all(pending.map(async (jobId) => {
    try {
      const status = await bridge.getJobStatus(jobId);
      applyJobUpdate(status);
      if (["done", "failed"].includes(status.phase)) {
        watchedJobIds.delete(jobId);
      }
    } catch (error) {
      watchedJobIds.delete(jobId);
      applyJobUpdate({
        jobId,
        executeId: state.jobsById[jobId]?.executeId ?? jobId,
        kind: state.jobsById[jobId]?.kind ?? "upload",
        phase: "failed",
        progress: state.jobsById[jobId]?.progress ?? { currentFile: jobId },
        error: String(error),
        result: null,
        startedAt: state.jobsById[jobId]?.startedAt ?? new Date().toISOString(),
        finishedAt: new Date().toISOString(),
      });
    }
  }));
}

function ensureJobPolling() {
  if (state.pollTimer || !watchedJobIds.size) return;
  state.pollTimer = window.setInterval(() => {
    void pollJobs();
  }, state.runtimeConfig?.jobPollIntervalMs ?? 1000);
}

function watchJob(jobId) {
  watchedJobIds.add(jobId);
  ensureJobPolling();
}

async function loadJobs() {
  try {
    const result = await bridge.listJobs(null, null, 100);
    rememberJobs(result.jobs ?? []);
    result.jobs?.forEach((job) => {
      if (job.phase === "running" && (job.kind === "upload" || job.kind === "download")) {
        watchJob(job.jobId);
      }
    });
  } catch (error) {
    console.error("Failed to load jobs", error);
  } finally {
    renderHistory();
    renderJobBar();
  }
}

function explorerUploadsForProvider() {
  return state.explorer.uploads.filter((entry) => entry.provider === state.explorer.provider);
}

function getSelectedExplorerUpload() {
  return explorerUploadsForProvider().find((entry) => entry.uploadId === state.explorer.selectedUploadId) ?? null;
}

function resetExplorerPreview() {
  state.explorer.preview = null;
  state.explorer.previewJobId = null;
  state.explorer.previewLoading = false;
  state.explorer.previewError = null;
  state.explorer.previewMetaOpen = false;
}

function explorerKindLabel(entry) {
  if (!entry) return "Item";
  if (entry.isDir || entry.itemType === "directory") return "Folder";
  const path = (entry.displayName || entry.name || "").toLowerCase();
  if (path.endsWith(".md") || path.endsWith(".markdown")) return "Markdown";
  if (path.endsWith(".pdf")) return "PDF Document";
  if (path.endsWith(".csv")) return "CSV File";
  if (path.endsWith(".json")) return "JSON File";
  if (path.endsWith(".js") || path.endsWith(".ts") || path.endsWith(".tsx") || path.endsWith(".jsx") || path.endsWith(".py") || path.endsWith(".rs")) return "Code File";
  if (path.endsWith(".png") || path.endsWith(".jpg") || path.endsWith(".jpeg") || path.endsWith(".gif") || path.endsWith(".webp")) return "Image File";
  return "File";
}

function explorerTypeLabel(entry) {
  if (!entry) return "file";
  if (entry.isDir || entry.itemType === "directory") return "folder";
  const path = (entry.displayName || entry.name || "").toLowerCase();
  if (path.endsWith(".pdf")) return "pdf";
  if (path.endsWith(".md") || path.endsWith(".markdown")) return "markdown";
  if (path.endsWith(".csv")) return "csv";
  if (path.endsWith(".json") || path.endsWith(".js") || path.endsWith(".ts") || path.endsWith(".tsx") || path.endsWith(".jsx") || path.endsWith(".py") || path.endsWith(".rs")) return "code";
  if (path.endsWith(".png") || path.endsWith(".jpg") || path.endsWith(".jpeg") || path.endsWith(".gif") || path.endsWith(".webp")) return "image";
  return "file";
}

function explorerIcon(entry, large = false) {
  const type = explorerTypeLabel(entry);
  const size = large ? "lg" : "sm";
  return `<span class="file-glyph ${size} ${type}">${type === "folder" ? "▣" : type === "pdf" ? "PDF" : type === "markdown" ? "MD" : type === "csv" ? "CSV" : type === "code" ? "</>" : type === "image" ? "IMG" : "FILE"}</span>`;
}

function ensureExplorerSelection() {
  const providerUploads = explorerUploadsForProvider();
  if (providerUploads.some((entry) => entry.uploadId === state.explorer.selectedUploadId)) {
    return;
  }
  state.explorer.selectedUploadId = providerUploads[0]?.uploadId ?? null;
  state.explorer.currentPath = "";
  state.explorer.query = "";
  state.explorer.offset = 0;
  state.explorer.entries = [];
  state.explorer.entriesError = null;
  resetExplorerPreview();
}

function explorerRemoteRef(entry) {
  return `${entry.viewCryptRemote}:${entry.remoteItemPath}`;
}

async function loadExplorerIndex() {
  state.explorer.loading = true;
  state.explorer.error = null;
  renderExplorer();
  try {
    const result = await bridge.listUploadIndex();
    state.explorer.uploads = result.uploads ?? [];
    ensureExplorerSelection();
    await loadExplorerEntries({ refresh: true });
  } catch (error) {
    state.explorer.error = String(error);
  } finally {
    state.explorer.loading = false;
    renderExplorer();
  }
}

async function loadExplorerEntries({ refresh = false } = {}) {
  const selectedUpload = getSelectedExplorerUpload();
  state.explorer.entriesLoading = true;
  state.explorer.entriesError = null;
  renderExplorer();

  if (!selectedUpload) {
    state.explorer.entries = [];
    state.explorer.entriesLoading = false;
    renderExplorer();
    return;
  }

  if (selectedUpload.itemType === "file" && !state.explorer.currentPath) {
    state.explorer.entries = [];
    state.explorer.totalCount = 0;
    state.explorer.nextOffset = null;
    state.explorer.entriesLoading = false;
    renderExplorer();
    return;
  }

  try {
    const result = await bridge.listExplorerEntries(
      selectedUpload.uploadId,
      state.explorer.currentPath,
      state.explorer.mode,
      state.explorer.query,
      state.explorer.offset,
      state.explorer.limit,
      refresh,
    );
    state.explorer.entries = result.entries ?? [];
    state.explorer.currentPath = result.currentPath ?? state.explorer.currentPath;
    state.explorer.totalCount = result.totalCount ?? 0;
    state.explorer.nextOffset = result.nextOffset ?? null;
    state.explorer.listedAt = result.listedAt ?? null;
  } catch (error) {
    state.explorer.entries = [];
    state.explorer.entriesError = String(error);
  } finally {
    state.explorer.entriesLoading = false;
    renderExplorer();
  }
}

function setExplorerProvider(provider) {
  state.explorer.provider = provider;
  ensureExplorerSelection();
  state.explorer.offset = 0;
  void loadExplorerEntries({ refresh: true });
}

function setExplorerMode(mode) {
  state.explorer.mode = mode;
  state.explorer.offset = 0;
  resetExplorerPreview();
  void loadExplorerEntries({ refresh: true });
}

function selectExplorerUpload(uploadId) {
  state.explorer.selectedUploadId = uploadId;
  state.explorer.currentPath = "";
  state.explorer.query = "";
  state.explorer.offset = 0;
  state.explorer.entries = [];
  state.explorer.entriesError = null;
  resetExplorerPreview();
  void loadExplorerEntries({ refresh: true });
}

function openExplorerDirectory(path) {
  state.explorer.currentPath = path;
  state.explorer.offset = 0;
  resetExplorerPreview();
  void loadExplorerEntries({ refresh: true });
}

function goExplorerUp() {
  if (!state.explorer.currentPath) return;
  const parts = state.explorer.currentPath.split("/").filter(Boolean);
  parts.pop();
  state.explorer.currentPath = parts.join("/");
  state.explorer.offset = 0;
  resetExplorerPreview();
  void loadExplorerEntries({ refresh: true });
}

function setExplorerQuery(query) {
  state.explorer.query = query;
  state.explorer.offset = 0;
  if (explorerSearchTimer) clearTimeout(explorerSearchTimer);
  explorerSearchTimer = window.setTimeout(() => {
    void loadExplorerEntries();
  }, 180);
}

function goExplorerPage(direction) {
  if (direction === "next" && state.explorer.nextOffset != null) {
    state.explorer.offset = state.explorer.nextOffset;
    void loadExplorerEntries();
  }
  if (direction === "prev") {
    state.explorer.offset = Math.max(0, state.explorer.offset - state.explorer.limit);
    void loadExplorerEntries();
  }
}

async function startExplorerPreview(path = "") {
  const selectedUpload = getSelectedExplorerUpload();
  if (!selectedUpload) return;
  state.explorer.preview = null;
  state.explorer.previewJobId = null;
  state.explorer.previewError = null;
  state.explorer.previewLoading = true;
  state.explorer.previewMetaOpen = false;
  renderExplorer();
  try {
    const result = await bridge.startPreviewExplorerItem(selectedUpload.uploadId, path);
    state.explorer.previewJobId = result.jobId;
    watchJob(result.jobId);
  } catch (error) {
    state.explorer.previewLoading = false;
    state.explorer.previewError = String(error);
    toast(`プレビューに失敗しました: ${String(error)}`, "err");
    renderExplorer();
  }
}

async function startExplorerDownload(path = "") {
  const selectedUpload = getSelectedExplorerUpload();
  if (!selectedUpload) return;
  try {
    const result = await bridge.startDownloadExplorerItem(selectedUpload.uploadId, path);
    rememberJobs([{
      jobId: result.jobId,
      executeId: result.jobId,
      kind: "download",
      phase: "running",
      progress: { currentFile: path || selectedUpload.displayName },
      error: null,
      result: null,
      startedAt: new Date().toISOString(),
      finishedAt: null,
    }]);
    renderHistory();
    renderJobBar();
    watchJob(result.jobId);
    toast("ダウンロードを開始しました");
  } catch (error) {
    toast(`ダウンロードに失敗しました: ${String(error)}`, "err");
  }
}

function renderExplorer() {
  const status = $("exp-status");
  const roots = $("exp-roots");
  const current = $("exp-current");
  const breadcrumbs = $("exp-breadcrumbs");
  const listedAt = $("exp-listed-at");
  const list = $("exp-list");
  const preview = $("exp-preview");
  const search = $("exp-search");
  const page = $("exp-page");
  const prev = $("btn-exp-prev");
  const next = $("btn-exp-next");
  const up = $("btn-exp-up");
  if (!status || !roots || !current || !breadcrumbs || !listedAt || !list || !preview || !search || !page || !prev || !next || !up) return;

  $("exp-provider-drive")?.classList.toggle("active", state.explorer.provider === "drive");
  $("exp-provider-r2")?.classList.toggle("active", state.explorer.provider === "r2");
  $("exp-mode-encrypted")?.classList.toggle("active", state.explorer.mode === "encrypted");
  $("exp-mode-decrypted")?.classList.toggle("active", state.explorer.mode === "decrypted");
  search.value = state.explorer.query;

  const providerUploads = explorerUploadsForProvider();
  const selectedUpload = getSelectedExplorerUpload();
  const modeLabel = state.explorer.mode === "encrypted" ? "暗号化名" : "復号名";
  const pathLabel = selectedUpload
    ? `${selectedUpload.viewCryptRemote}:${selectedUpload.remoteItemPath}${state.explorer.currentPath ? `/${state.explorer.currentPath}` : ""}`
    : "remote:path";

  if (state.explorer.loading) {
    status.textContent = "索引を読み込み中…";
    status.title = "索引を読み込み中…";
    roots.innerHTML = `<div class="exp-empty">読み込み中…</div>`;
    current.innerHTML = `<div class="exp-current-empty">読み込み中…</div>`;
    breadcrumbs.innerHTML = "";
    listedAt.textContent = "";
    list.innerHTML = `<div class="exp-empty">読み込み中…</div>`;
    preview.innerHTML = `<div class="exp-preview-card empty">ファイルを選択するとプレビューが表示されます</div>`;
    page.textContent = "";
    prev.disabled = true;
    next.disabled = true;
    up.disabled = true;
    return;
  }

  if (state.explorer.error) {
    status.textContent = `索引の読み込みに失敗しました: ${state.explorer.error}`;
    status.title = status.textContent;
    roots.innerHTML = `<div class="exp-empty">索引の読み込みに失敗しました。</div>`;
    current.innerHTML = `<div class="exp-current-empty">索引の読み込みに失敗しました。</div>`;
    breadcrumbs.innerHTML = "";
    listedAt.textContent = "";
    list.innerHTML = `<div class="exp-empty">索引の読み込みに失敗しました。</div>`;
    preview.innerHTML = `<div class="exp-preview-card empty">索引の読み込みに失敗しました。</div>`;
    page.textContent = "";
    prev.disabled = true;
    next.disabled = true;
    up.disabled = true;
    return;
  }

  status.textContent = selectedUpload
    ? pathLabel
    : providerUploads.length
      ? `${modeLabel}で表示しています。アップロード済みルート ${providerUploads.length} 件。`
      : "この provider には、まだアップロード済み項目がありません。";
  status.title = status.textContent;

  roots.innerHTML = providerUploads.length
    ? providerUploads.map((entry) => `
        <button class="exp-root ${entry.uploadId === state.explorer.selectedUploadId ? "active" : ""}" data-exp-root="${escapeHtml(entry.uploadId)}">
          <span class="exp-root-icon">${explorerIcon(entry)}</span>
          <span class="exp-root-copy">
            <span class="exp-root-title">${escapeHtml(entry.displayName)}</span>
            <span class="exp-root-meta">${escapeHtml(explorerKindLabel(entry))} · ${escapeHtml(formatExplorerTime(entry.uploadedAt))}</span>
          </span>
        </button>
      `).join("")
    : `<div class="exp-empty">まずバックアップを完了すると、ここに起点フォルダが表示されます。</div>`;

  if (!selectedUpload) {
    current.innerHTML = `<div class="exp-current-empty">表示するアップロード起点を選択してください。</div>`;
    breadcrumbs.innerHTML = "";
    listedAt.textContent = "";
    list.innerHTML = `<div class="exp-empty">表示するアップロード起点を選択してください。</div>`;
    preview.innerHTML = `<div class="exp-preview-card empty">ファイルを選択するとプレビューが表示されます</div>`;
    page.textContent = "";
    prev.disabled = true;
    next.disabled = true;
    up.disabled = true;
    return;
  }

  const segments = state.explorer.currentPath.split("/").filter(Boolean);
  const crumbs = [{ label: selectedUpload.displayName, path: "" }]
    .concat(segments.map((segment, index) => ({
      label: segment,
      path: segments.slice(0, index + 1).join("/"),
    })));

  current.innerHTML = `
    <div class="exp-current-main">
      <div class="exp-current-kicker">Active Root</div>
      <div class="exp-current-title">${escapeHtml(selectedUpload.displayName)}</div>
      <div class="exp-current-meta">${escapeHtml(selectedUpload.sourcePath)}</div>
      <div class="exp-current-meta mono">${escapeHtml(explorerRemoteRef(selectedUpload))}</div>
    </div>
    <div class="exp-current-side">
      <div class="exp-current-badge">${escapeHtml(modeLabel)}</div>
      <div class="exp-actions">
        ${selectedUpload.itemType === "file" && !state.explorer.currentPath ? `<button class="btn btn-sec btn-sm" data-exp-preview="">プレビュー</button>` : ""}
        <button class="btn btn-ghost btn-sm" data-exp-download="">ダウンロード</button>
      </div>
    </div>
  `;

  breadcrumbs.innerHTML = crumbs.map((crumb, index) => `
    <button class="exp-crumb ${index === crumbs.length - 1 ? "active" : ""}" data-exp-crumb="${escapeHtml(crumb.path)}">
      ${escapeHtml(crumb.label)}
    </button>
  `).join(`<span class="exp-crumb-sep">/</span>`);

  listedAt.textContent = state.explorer.listedAt
    ? `最終一覧取得: ${formatExplorerTime(state.explorer.listedAt)}`
    : "";
  up.disabled = !state.explorer.currentPath || selectedUpload.itemType === "file";

  if (selectedUpload.itemType === "file" && !state.explorer.currentPath) {
    list.innerHTML = `<div class="exp-empty">このアップロードは単体ファイルです。プレビューまたはダウンロードを実行してください。</div>`;
  } else if (state.explorer.entriesLoading) {
    list.innerHTML = `<div class="exp-empty">フォルダ内容を読み込み中…</div>`;
  } else if (state.explorer.entriesError) {
    list.innerHTML = `<div class="exp-empty">一覧の取得に失敗しました: ${escapeHtml(state.explorer.entriesError)}</div>`;
  } else if (!state.explorer.entries.length) {
    list.innerHTML = `<div class="exp-empty">この条件に一致する項目はありません。</div>`;
  } else {
    list.innerHTML = state.explorer.entries.map((entry) => `
      <div class="exp-row">
        <div class="exp-name">
          <div class="exp-icon">${explorerIcon(entry)}</div>
          <div style="min-width:0">
            <div class="exp-title">${escapeHtml(entry.displayName ?? entry.name)}</div>
            <div class="exp-sub">${escapeHtml(entry.path)}</div>
          </div>
        </div>
        <div class="exp-cell">
          <span class="exp-chip ${entry.isDir ? "dir" : "file"}">${escapeHtml(explorerKindLabel(entry))}</span>
        </div>
        <div class="exp-cell">${escapeHtml(formatBytes(entry.size))}</div>
        <div class="exp-cell">${escapeHtml(formatExplorerTime(entry.modTime))}</div>
        <div class="exp-actions">
          ${entry.isDir
            ? `<button class="btn btn-sec btn-sm" data-exp-open="${escapeHtml(entry.path)}">開く</button>`
            : `<button class="btn btn-sec btn-sm" data-exp-preview="${escapeHtml(entry.path)}">プレビュー</button>`}
          <button class="btn btn-ghost btn-sm" data-exp-download="${escapeHtml(entry.path)}">ダウンロード</button>
        </div>
      </div>
    `).join("");
  }

  const pageEnd = Math.min(state.explorer.totalCount, state.explorer.offset + state.explorer.entries.length);
  page.textContent = state.explorer.totalCount
    ? `${state.explorer.offset + 1}-${pageEnd} / ${state.explorer.totalCount}`
    : "0 / 0";
  prev.disabled = state.explorer.offset === 0;
  next.disabled = state.explorer.nextOffset == null;

  if (state.explorer.previewLoading) {
    preview.innerHTML = `<div class="exp-preview-card"><div class="exp-preview-title">プレビューを読み込み中…</div><div class="exp-preview-meta">${escapeHtml(pathLabel)}</div></div>`;
    return;
  }

  if (state.explorer.previewError) {
    preview.innerHTML = `<div class="exp-preview-card"><div class="exp-preview-title">プレビューの取得に失敗しました</div><div class="exp-preview-error">${escapeHtml(state.explorer.previewError)}</div></div>`;
    return;
  }

  if (!state.explorer.preview) {
    preview.innerHTML = `<div class="exp-preview-card empty">ファイルを選択するとプレビューが表示されます</div>`;
    return;
  }

  const previewData = state.explorer.preview;
  const previewMeta = `
    <div class="exp-preview-stat">
      <span class="exp-preview-label">場所</span>
      <span class="exp-preview-value mono">${escapeHtml(pathLabel)}</span>
    </div>
    <div class="exp-preview-stat">
      <span class="exp-preview-label">種別</span>
      <span class="exp-preview-value">${escapeHtml(explorerKindLabel(previewData))}</span>
    </div>
    <div class="exp-preview-stat">
      <span class="exp-preview-label">サイズ</span>
      <span class="exp-preview-value">${escapeHtml(formatBytes(previewData.size))}</span>
    </div>
    <div class="exp-preview-stat">
      <span class="exp-preview-label">表示</span>
      <span class="exp-preview-value">${escapeHtml(previewData.kind === "unsupported" ? "未対応" : modeLabel)}</span>
    </div>
  `;

  if (previewData.kind === "image") {
    preview.innerHTML = `
      <div class="exp-preview-card">
        <div class="exp-preview-top">
          <div class="exp-preview-title-wrap">
            <div class="exp-preview-title">${escapeHtml(previewData.name)}</div>
            <div class="exp-preview-meta">${escapeHtml(formatBytes(previewData.size))}</div>
          </div>
          <div class="exp-actions">
            <button class="btn btn-ghost btn-sm" id="btn-exp-meta">${state.explorer.previewMetaOpen ? "閉じる" : "情報"}</button>
            <button class="btn btn-sec btn-sm" data-exp-download="${escapeHtml(previewData.path ?? "")}">ダウンロード</button>
          </div>
        </div>
        ${state.explorer.previewMetaOpen ? `<div class="exp-preview-grid">${previewMeta}</div>` : ""}
        <div class="exp-preview-canvas image">
          <img class="exp-preview-image" src="${previewData.imageDataUrl}" alt="${escapeHtml(previewData.name)}" />
        </div>
      </div>
    `;
    return;
  }

  if (previewData.kind === "text") {
    preview.innerHTML = `
      <div class="exp-preview-card">
        <div class="exp-preview-top">
          <div class="exp-preview-title-wrap">
            <div class="exp-preview-title">${escapeHtml(previewData.name)}</div>
            <div class="exp-preview-meta">${escapeHtml(formatBytes(previewData.size))}${previewData.truncated ? " / 先頭のみ表示" : ""}</div>
          </div>
          <div class="exp-actions">
            <button class="btn btn-ghost btn-sm" id="btn-exp-meta">${state.explorer.previewMetaOpen ? "閉じる" : "情報"}</button>
            <button class="btn btn-sec btn-sm" data-exp-download="${escapeHtml(previewData.path ?? "")}">ダウンロード</button>
          </div>
        </div>
        ${state.explorer.previewMetaOpen ? `<div class="exp-preview-grid">${previewMeta}</div>` : ""}
        <div class="exp-preview-canvas">
          <pre class="exp-preview-text">${escapeHtml(previewData.text ?? "")}</pre>
        </div>
      </div>
    `;
    return;
  }

  preview.innerHTML = `
    <div class="exp-preview-card">
      <div class="exp-preview-top">
        <div class="exp-preview-title-wrap">
          <div class="exp-preview-title">${escapeHtml(previewData.name)}</div>
          <div class="exp-preview-meta">この形式はアプリ内プレビュー非対応です。</div>
        </div>
        <div class="exp-actions">
          <button class="btn btn-ghost btn-sm" id="btn-exp-meta">${state.explorer.previewMetaOpen ? "閉じる" : "情報"}</button>
          <button class="btn btn-sec btn-sm" data-exp-download="${escapeHtml(previewData.path ?? "")}">ダウンロード</button>
        </div>
      </div>
      ${state.explorer.previewMetaOpen ? `<div class="exp-preview-grid">${previewMeta}</div>` : ""}
      <div class="exp-preview-canvas unsupported">
        <div class="exp-preview-placeholder">${explorerIcon(previewData, true)}<span>プレビューできません</span></div>
      </div>
    </div>
  `;
}

async function startBackup() {
  const { sourcePath, baseRemote, remotePath, password } = state.wizard;
  if (!sourcePath.trim()) {
    toast("フォルダのパスを入力してください", "err");
    return;
  }
  if (!password.trim()) {
    toast("暗号化パスワードを入力してください", "err");
    return;
  }

  const btn = $("wiz-start");
  if (btn) {
    btn.disabled = true;
    btn.classList.add("btn-spin");
  }

  try {
    const suffix = state.runtimeConfig?.defaultCryptRemoteSuffix ?? "-crypt";
    const crypt = await bridge.createCryptRemote(baseRemote, suffix, password);
    if (!crypt.ok) throw new Error(crypt.error ?? "crypt remote の作成に失敗しました");

    const upload = await bridge.startUpload(sourcePath, `${baseRemote}${suffix}`, remotePath, "copy");
    rememberJobs([{
      jobId: upload.jobId,
      executeId: upload.executeId,
      kind: "upload",
      phase: "running",
      progress: { currentFile: sourcePath },
      error: null,
      result: null,
      startedAt: new Date().toISOString(),
      finishedAt: null,
    }]);
    renderHistory();
    renderJobBar();
    watchJob(upload.jobId);
    showView("dashboard");
    toast("バックアップを開始しました");
  } catch (error) {
    toast(String(error), "err");
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.classList.remove("btn-spin");
    }
  }
}

async function boot() {
  state.bridge = bridge.hasBridge();
  const dot = $("bridge-dot");
  const label = $("bridge-lbl");

  if (state.bridge) {
    dot?.classList.add("ok");
    if (label) label.textContent = "Tauri bridge 接続済み";
    await bridge.checkGuestMode().catch(() => {});
    state.runtimeConfig = await bridge.getRuntimeConfig().catch(() => null);
    if (state.runtimeConfig) {
      const cfgPath = $("cfg-path");
      if (cfgPath) cfgPath.value = state.runtimeConfig.configPath ?? "";
    }

    const providerStatuses = await bridge.getProviderStatuses().catch(() => null);
    providerStatuses?.providers?.forEach((provider) => {
      setProvider(
        provider.id,
        provider.connected ? "connected" : "unknown",
        provider.connected ? "保存済みの設定を読み込みました" : undefined,
      );
    });

    await loadJobs();
    await loadExplorerIndex();
  } else if (label) {
    label.textContent = "bridge なし（開発中）";
  }

  document.querySelectorAll(".nav-item[data-view]").forEach((el) => {
    el.addEventListener("click", () => showView(el.dataset.view));
  });

  on("btn-connect-drive", "click", () => connectProvider("drive"));
  on("btn-connect-r2", "click", () => connectProvider("r2"));
  on("btn-new-backup", "click", () => {
    showView("wizard");
    goStep(1);
  });
  on("btn-wiz-back-dash", "click", () => showView("dashboard"));
  on("btn-pick-folder", "click", () => { void pickFolder(); });
  on("exp-provider-drive", "click", () => setExplorerProvider("drive"));
  on("exp-provider-r2", "click", () => setExplorerProvider("r2"));
  on("exp-mode-encrypted", "click", () => setExplorerMode("encrypted"));
  on("exp-mode-decrypted", "click", () => setExplorerMode("decrypted"));
  on("btn-exp-refresh", "click", () => { void loadExplorerEntries({ refresh: true }); });
  on("btn-exp-up", "click", () => goExplorerUp());
  on("btn-exp-prev", "click", () => goExplorerPage("prev"));
  on("btn-exp-next", "click", () => goExplorerPage("next"));
  on("exp-search", "input", (event) => setExplorerQuery(event.target.value));

  $("exp-roots")?.addEventListener("click", (event) => {
    const target = event.target instanceof Element ? event.target.closest("[data-exp-root]") : null;
    if (!target) return;
    selectExplorerUpload(target.dataset.expRoot ?? "");
  });

  $("exp-breadcrumbs")?.addEventListener("click", (event) => {
    const target = event.target instanceof Element ? event.target.closest("[data-exp-crumb]") : null;
    if (!target) return;
    openExplorerDirectory(target.dataset.expCrumb ?? "");
  });

  const explorerActionHandler = (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const metaButton = target?.closest("#btn-exp-meta");
    if (metaButton) {
      state.explorer.previewMetaOpen = !state.explorer.previewMetaOpen;
      renderExplorer();
      return;
    }

    const openButton = target?.closest("[data-exp-open]");
    if (openButton) {
      openExplorerDirectory(openButton.dataset.expOpen ?? "");
      return;
    }

    const previewButton = target?.closest("[data-exp-preview]");
    if (previewButton) {
      void startExplorerPreview(previewButton.dataset.expPreview ?? "");
      return;
    }

    const downloadButton = target?.closest("[data-exp-download]");
    if (downloadButton) {
      void startExplorerDownload(downloadButton.dataset.expDownload ?? "");
    }
  };

  $("exp-list")?.addEventListener("click", explorerActionHandler);
  $("exp-current")?.addEventListener("click", explorerActionHandler);
  $("exp-preview")?.addEventListener("click", explorerActionHandler);

  on("wn1", "click", () => {
    const value = $("wiz-src")?.value.trim();
    if (!value) {
      toast("パスを入力してください", "err");
      return;
    }
    state.wizard.sourcePath = value;
    goStep(2);
  });
  on("wb2", "click", () => goStep(1));
  on("wn2", "click", () => {
    state.wizard.remotePath = $("wiz-dst")?.value.trim() || "backup";
    goStep(3);
  });
  on("wb3", "click", () => goStep(2));
  on("wiz-start", "click", async () => {
    state.wizard.password = $("wiz-pass")?.value ?? "";
    state.wizard.useKeychain = $("wiz-kc")?.checked ?? true;
    await startBackup();
  });

  document.querySelectorAll(".remote-opt").forEach((el) => {
    el.addEventListener("click", () => selectWizRemote(el.dataset.remote));
  });

  on("btn-dbg", "click", () => {
    const pre = $("dbg-out");
    if (!pre) return;
    if (pre.style.display === "none") {
      pre.style.display = "block";
      pre.textContent = JSON.stringify({
        bridge: state.bridge,
        runtimeConfig: state.runtimeConfig,
        providers: state.providers,
      }, null, 2);
    } else {
      pre.style.display = "none";
    }
  });

  renderRml();
  renderHistory();
  renderJobBar();
  renderExplorer();

  window.addEventListener("beforeunload", () => {
    if (state.pollTimer) clearInterval(state.pollTimer);
  });
}

void boot();
