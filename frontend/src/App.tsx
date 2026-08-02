import React, { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./lib/api";
import { formatBytes, formatTime } from "./lib/format";
import {
  RecoveryKit,
  downloadRecoveryKit,
  loadRecoveryKits,
  parseRecoveryKit,
  removeRecoveryKit,
  saveRecoveryKit,
} from "./lib/recoveryKit";
import type { JobStatus, ObjectEntry, VaultStatus } from "./types";
import "./styles.css";

type Phase = "onboarding" | "locked" | "connected";

export default function App() {
  const [phase, setPhase] = useState<Phase>("onboarding");
  const [banner, setBanner] = useState<string | null>(null);
  const [vault, setVault] = useState<VaultStatus | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    setBusy(true);
    try {
      const drive = await api.driveStatus();
      if (!drive.connected) {
        setPhase("onboarding");
        return;
      }
      try {
        const v = await api.vaultStatus();
        setVault(v);
        if (v.unlocked && v.vaultExists) setPhase("connected");
        else if (v.vaultExists) setPhase("locked");
        else setPhase("locked"); // drive connected, no vault yet -> create
      } catch {
        setVault(null);
        setPhase("locked");
      }
    } catch {
      setPhase("onboarding");
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    refresh();
    const params = new URLSearchParams(window.location.search);
    const d = params.get("drive");
    if (d === "connected") setBanner("Google Drive connected.");
    if (d === "error") setBanner(`Connection failed: ${params.get("reason") ?? "unknown"}`);
    if (params.has("drive")) window.history.replaceState({}, "", "/");
  }, [refresh]);

  async function handleDisconnect() {
    setBusy(true);
    try {
      await api.disconnect();
      setVault(null);
      setPhase("onboarding");
    } catch (e) {
      setBanner(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="app">
      <div className="brand">
        <div className="brand-mark">◆</div>
        <div>
          <h1>Vault</h1>
          <p>Encrypted Google Drive backup</p>
        </div>
      </div>

      {banner && (
        <div className="banner">
          <span>{banner}</span>
          <button className="banner-dismiss" onClick={() => setBanner(null)}>
            ✕
          </button>
        </div>
      )}

      {phase === "onboarding" && <Onboarding onConnected={refresh} />}
      {phase === "locked" && (
        <Locked vault={vault} busy={busy} onUnlocked={refresh} onDisconnect={handleDisconnect} />
      )}
      {phase === "connected" && <Connected busy={busy} onDisconnect={handleDisconnect} />}
    </div>
  );
}

function Onboarding({ onConnected }: { onConnected: () => void }) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function connect() {
    setBusy(true);
    setErr(null);
    try {
      const url = await api.startDrive();
      window.location.href = url;
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
    setBusy(false);
  }

  return (
    <div className="panel center">
      <h2>Connect Google Drive</h2>
      <p>
        Vault stores your files encrypted on your own Google Drive. Connect to
        continue.
      </p>
      <button className="btn" onClick={connect} disabled={busy}>
        {busy ? "Opening Google…" : "Connect Google Drive"}
      </button>
      {err && <p className="error-text">{err}</p>}
    </div>
  );
}

function Locked({
  vault,
  busy,
  onUnlocked,
  onDisconnect,
}: {
  vault: VaultStatus | null;
  busy: boolean;
  onUnlocked: () => void;
  onDisconnect: () => void;
}) {
  const [recoveryKey, setRecoveryKey] = useState("");
  const [localBusy, setLocalBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [revealed, setRevealed] = useState<RecoveryKit | null>(null);
  const [savedKit, setSavedKit] = useState<RecoveryKit | null>(null);
  const [importErr, setImportErr] = useState<string | null>(null);
  const [importFileName, setImportFileName] = useState<string | null>(null);
  const importInput = useRef<HTMLInputElement>(null);

  // Offer a locally saved recovery kit that matches this vault (by vault id or
  // key fingerprint) rather than unconditionally the first kit stored.
  useEffect(() => {
    const vaultId = vault?.vaultId;
    const fingerprint = vault?.keyFingerprint;
    loadRecoveryKits().then((kits) => {
      if (kits.length === 0) return;
      const match =
        kits.find((k) => vaultId && k.vaultId === vaultId) ??
        kits.find((k) => fingerprint && k.keyFingerprint === fingerprint) ??
        null;
      setSavedKit(match);
    });
  }, [vault?.vaultId, vault?.keyFingerprint]);

  async function create() {
    setLocalBusy(true);
    setErr(null);
    try {
      const resp = await api.initialize();
      const kit: RecoveryKit = {
        vaultId: resp.vaultId,
        keyFingerprint: resp.keyFingerprint,
        recoveryKey: resp.recoveryKey,
      };
      await saveRecoveryKit(kit);
      setSavedKit(kit);
      setRevealed(kit);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setLocalBusy(false);
    }
  }

  async function unlockWithKey(key: string) {
    setLocalBusy(true);
    setErr(null);
    try {
      await api.unlock(key.trim());
      onUnlocked();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setLocalBusy(false);
    }
  }

  async function unlockSaved() {
    if (!savedKit) return;
    await unlockWithKey(savedKit.recoveryKey);
  }

  async function unlock() {
    await unlockWithKey(recoveryKey);
  }

  async function importKit(file: File | null) {
    if (!file) return;
    setImportErr(null);
    try {
      const text = await file.text();
      const kit = parseRecoveryKit(text);
      if (!kit) {
        setImportErr("That file is not a valid Vault recovery kit.");
        return;
      }
      await saveRecoveryKit(kit);
      setSavedKit(kit);
      setImportFileName(file.name);
    } catch (e) {
      setImportErr(e instanceof Error ? e.message : String(e));
    }
  }

  const hasVault = vault?.vaultExists === true;

  return (
    <div className="panel center">
      {revealed ? (
        <div className="recovery-box">
          <h3>Your Recovery Key</h3>
          <p className="warn">Save this now — it cannot be retrieved again.</p>
          <p className="muted small">
            It is stored in this browser for convenience. Download a recovery
            kit for a new device, then continue.
          </p>
          <code className="recovery">{revealed.recoveryKey}</code>
          <div className="row">
            <button className="btn" onClick={() => navigator.clipboard.writeText(revealed.recoveryKey)}>
              Copy
            </button>
            <button className="btn" onClick={() => downloadRecoveryKit(revealed)}>
              Download Kit
            </button>
            <button className="btn ghost" onClick={onUnlocked}>
              Continue
            </button>
          </div>
        </div>
      ) : hasVault ? (
        <>
          <h2>Unlock your Vault</h2>
          <p>Enter your recovery key to unlock this Vault.</p>
          {savedKit && (
            <button className="btn" onClick={unlockSaved} disabled={localBusy || busy}>
              Unlock with saved key
            </button>
          )}
          <div className="or-sep">{savedKit ? "or" : ""}</div>
          <input
            className="input mono"
            type="password"
            placeholder="Recovery key"
            value={recoveryKey}
            onChange={(e) => setRecoveryKey(e.target.value)}
          />
          <button className="btn" onClick={unlock} disabled={localBusy || busy || !recoveryKey.trim()}>
            {localBusy ? "Unlocking…" : "Unlock"}
          </button>
          <div className="import-row">
            <button
              type="button"
              className="link-like"
              onClick={() => importInput.current?.click()}
            >
              Import recovery kit
            </button>
            <input
              ref={importInput}
              type="file"
              accept="application/json,.json"
              hidden
              onChange={(e) => importKit(e.target.files?.[0] ?? null)}
            />
            {importFileName && <span className="muted small">Imported {importFileName}</span>}
            {importErr && <span className="error-text small">{importErr}</span>}
          </div>
        </>
      ) : (
        <>
          <h2>Create your Vault</h2>
          <p>Create a new encrypted Vault on your Google Drive.</p>
          <button className="btn" onClick={create} disabled={localBusy || busy}>
            {localBusy ? "Creating…" : "Create Vault"}
          </button>
        </>
      )}

      {err && <p className="error-text">{err}</p>}
      <button className="link ghost" onClick={onDisconnect} disabled={busy}>
        Disconnect Drive
      </button>
    </div>
  );
}

function Connected({
  busy,
  onDisconnect,
}: {
  busy: boolean;
  onDisconnect: () => void;
}) {
  const [entries, setEntries] = useState<ObjectEntry[]>([]);
  const [path, setPath] = useState("");
  const [jobs, setJobs] = useState<JobStatus[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ name: string; content: string } | null>(null);
  const [fileInput, setFileInput] = useState<HTMLInputElement | null>(null);

  const reload = useCallback(async () => {
    setErr(null);
    try {
      const [files, j] = await Promise.all([api.listFiles(path), api.listJobs()]);
      setEntries(files.entries);
      setJobs(j.jobs);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }, [path]);

  useEffect(() => {
    reload();
  }, [reload]);

  useEffect(() => {
    const id = setInterval(() => {
      api.listJobs().then((r) => setJobs(r.jobs)).catch(() => {});
    }, 2000);
    return () => clearInterval(id);
  }, []);

  async function openDir(entry: ObjectEntry) {
    if (!entry.isDir) return;
    setPath(path ? `${path}/${entry.name}` : entry.name);
  }

  async function previewFile(entry: ObjectEntry) {
    if (entry.isDir) return;
    try {
      const full = path ? `${path}/${entry.name}` : entry.name;
      const resp = await api.preview(full);
      setPreview({
        name: entry.name,
        content: resp.text ?? "(binary file — no text preview)",
      });
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }

  async function onUpload(files: FileList | null) {
    if (!files || files.length === 0) return;
    try {
      const results = await api.uploadFiles(Array.from(files), path);
      const failed = results.filter((r) => !r.ok);
      if (failed.length) setErr(`${failed.length} file(s) failed to upload.`);
      reload();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
    if (fileInput) fileInput.value = "";
  }

  return (
    <div className="workspace">
      <div className="topbar">
        <Breadcrumbs path={path} onNavigate={setPath} />
        <div className="topbar-actions">
          <label className="btn">
            Upload
            <input
              ref={(el) => setFileInput(el)}
              type="file"
              multiple
              hidden
              onChange={(e) => onUpload(e.target.files)}
            />
          </label>
          <button className="btn ghost" onClick={onDisconnect} disabled={busy}>
            Disconnect
          </button>
        </div>
      </div>

      {err && <div className="error-text pad">{err}</div>}

      <section className="explorer">
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Type</th>
              <th>Size</th>
              <th>Modified</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {entries.map((entry) => (
              <tr key={entry.path}>
                <td>
                  <button
                    className="file-link"
                    onClick={() => (entry.isDir ? openDir(entry) : previewFile(entry))}
                  >
                    <span className={entry.isDir ? "kind-dir" : "kind-file"}>
                      {entry.isDir ? "▸" : "·"}
                    </span>
                    {entry.name}
                  </button>
                </td>
                <td className="muted">{entry.isDir ? "folder" : entry.mimeType ?? "file"}</td>
                <td className="muted">{entry.isDir ? "—" : formatBytes(entry.size)}</td>
                <td className="muted">{entry.modTime ? formatTime(entry.modTime) : "—"}</td>
                <td className="row-actions">
                  {entry.isDir && (
                    <button className="tiny" onClick={() => openDir(entry)}>
                      open
                    </button>
                  )}
                </td>
              </tr>
            ))}
            {entries.length === 0 && (
              <tr>
                <td colSpan={5} className="empty">
                  This folder is empty.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </section>

      {preview && <PreviewModal preview={preview} onClose={() => setPreview(null)} />}

      <section className="jobs">
        <h3>Uploads</h3>
        {jobs.length === 0 && <p className="muted pad">No uploads recorded.</p>}
        <ul>
          {jobs.map((job) => (
            <li key={job.jobId}>
              <div className="job-row">
                <span className="job-name">{job.kind}</span>
                <span className={`badge ${job.phase}`}>{job.phase}</span>
                {job.error && <span className="muted">{job.error}</span>}
              </div>
              {job.phase === "running" && (
                <div className="job-progress">
                  <span>{formatBytes(job.progress.bytesDone)} transferred</span>
                  <button className="mini" onClick={() => api.cancelJob(job.jobId).catch(() => {})}>
                    cancel
                  </button>
                </div>
              )}
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}

function Breadcrumbs({
  path,
  onNavigate,
}: {
  path: string;
  onNavigate: (path: string) => void;
}) {
  const segs = path ? path.split("/") : [];
  return (
    <div className="crumbs">
      <button className={path === "" ? "crumb active" : "crumb"} onClick={() => onNavigate("")}>
        Vault
      </button>
      {segs.map((seg, i) => {
        const full = segs.slice(0, i + 1).join("/");
        return (
          <span key={i}>
            <span className="sep">/</span>
            <button
              className={full === path ? "crumb active" : "crumb"}
              onClick={() => onNavigate(full)}
            >
              {seg}
            </button>
          </span>
        );
      })}
    </div>
  );
}

function PreviewModal({
  preview,
  onClose,
}: {
  preview: { name: string; content: string };
  onClose: () => void;
}) {
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <strong>{preview.name}</strong>
          <button className="tiny" onClick={onClose}>
            ✕
          </button>
        </div>
        <pre className="modal-pre">{preview.content}</pre>
      </div>
    </div>
  );
}
