import { formatBytes, formatTime } from "../lib/format";
import type { BackupJobResult, JobState } from "../types";

function backupSummary(job: JobState): string {
  if (job.kind !== "upload" || !job.result || typeof job.result !== "object" || !("snapshotId" in job.result)) {
    return job.progress.currentFile ?? job.jobId;
  }
  const result = job.result as BackupJobResult;
  return `snapshot ${result.snapshotId.slice(0, 8)} · +${result.filesNew} new / ${formatBytes(result.totalBytesAdded)}`;
}

export function HistoryView({ jobs }: { jobs: JobState[] }) {
  return (
    <section className="view active">
      <div className="sec-hd">
        <div className="sec-eye">履歴</div>
        <h1 className="sec-title">スナップショット履歴</h1>
      </div>
      <div>
        {!jobs.length ? (
          <p style={{ color: "var(--t3)", fontSize: 13, marginTop: 12 }}>まだ snapshot backup を実行していません。</p>
        ) : jobs.map((job) => (
          <div key={job.jobId} className="hist-row">
            <span className={`phase-badge ${job.phase === "done" ? "done" : job.phase === "failed" ? "fail" : "run"}`}>
              {job.phase === "done" ? "完了" : job.phase === "failed" ? "失敗" : "実行中"}
            </span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 12.5, fontWeight: 500, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {backupSummary(job)}
              </div>
              <div style={{ fontSize: 11, color: "var(--t2)", marginTop: 2 }}>
                {formatTime(job.startedAt ?? job.finishedAt)}
              </div>
            </div>
            <div style={{ fontFamily: "'JetBrains Mono', monospace", fontSize: 11, color: "var(--t3)" }}>{job.kind}</div>
          </div>
        ))}
      </div>
    </section>
  );
}
