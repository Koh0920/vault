import React, { useCallback, useEffect, useState } from "react";
import { api } from "./lib/api";
import { formatBytes, formatTime } from "./lib/format";
import type { JobStatus, ObjectEntry, VaultStatus } from "./types";
import "./styles.css";

type Phase = "onboarding" | "locked" | "connected";

export default function App() {
  const [phase, setPhase] = useState<Phase>("onboarding");
  const [banner, setBanner] = useState<string | null>(null);
  const [vault, setVault] = useState<VaultStatus | null>(null);

  const refresh = useCallback(async () => {
    try {
      const drive = await api.driveStatus();
      if (!drive.connected) {
        setPhase("onboarding");
        return;
      }
      try {
        const v = await api.vaultStatus();
        setVault(v);
        setPhase(v.initialized === true ? "connected" : "locked");
      } catch {
        setVault(null);
        setPhase("locked");
      }
    } catch {
      setPhase("onboarding");
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
        <Locked vault={vault} onUnlocked={refresh} onDisconnect={refresh} />
      )}
      {phase === "connected" && <Connected onDisconnect={refresh} />}
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
  onUnlocked,
  onDisconnect,
}: {
  vault: VaultStatus | null;
  onUnlocked: () => void;
  onDisconnect: () => void;
}) {
  const [recoveryKey, setRecoveryKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [revealed, setRevealed] = useState<string | null>(null);

  async function create() {
    setBusy(true);
    setErr(null);
    try {
      const resp = await api.initialize();
      setRevealed(resp.recoveryKey);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
    setBusy(false);
  }

  async function unlock() {
    setBusy(true);
    setErr(null);
    try {
      await api.unlock(recoveryKey.trim());
      onUnlocked();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
    setBusy(false);
  }

  return (
    <div className="panel center">
      <h2>{vault?.initialized ? "Unlock your Vault" : "Create your Vault"}</h2>

      {vault?.initialized ? (
        <>
          <p>Enter your recovery key to unlock this Vault.</p>
          <input
            className="input mono"
            type="password"
            placeholder="Recovery key"
            value={recoveryKey}
            onChange={(e) => setRecoveryKey(e.target.value)}
          />
          <button className="btn" onClick={unlock} disabled={busy || !recoveryKey.trim()}>
            {busy ? "Unlocking…" : "Unlock"}
          </button>
        </>
      ) : !revealed ? (
        <>
          <p>Create a new encrypted Vault.</p>
          <button className="btn" onClick={create} disabled={busy}>
            {busy ? "Creating…" : "Create Vault"}
          </button>
        </>
      ) : (
        <div className="recovery-box">
          <h3>Your Recovery Key</h3>
          <p className="warn">Save this now — it cannot be retrieved again.</p>
          <code className="recovery">{revealed}</code>
          <div className="row">
            <button className="btn" onClick={() => navigator.clipboard.writeText(revealed)}>
              Copy
            </button>
            <button className="btn ghost" onClick={() => setRevealed(null)}>
              Done
            </button>
          </div>
        </div>
      )}

      {err && <p className="error-text">{err}</p>}
      <button className="link ghost" onClick={onDisconnect}>
        Disconnect Drive
      </button>
    </div>
  );
}

function Connected({ onDisconnect }: { onDisconnect: () => void }) {
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
          <button className="btn ghost" onClick={onDisconnect}>
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