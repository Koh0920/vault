import { formatTime } from "../lib/format";
import type { JobState } from "../types";

export function HistoryView({ jobs }: { jobs: JobState[] }) {
  return (
    <section className="view active">
      <div className="sec-hd">
        <div className="sec-eye">履歴</div>
        <h1 className="sec-title">バックアップ履歴</h1>
      </div>
      <div>
        {!jobs.length ? (
          <p style={{ color: "var(--t3)", fontSize: 13, marginTop: 12 }}>まだバックアップを実行していません。</p>
        ) : jobs.map((job) => (
          <div key={job.jobId} className="hist-row">
            <span className={`phase-badge ${job.phase === "done" ? "done" : job.phase === "failed" ? "fail" : "run"}`}>
              {job.phase === "done" ? "完了" : job.phase === "failed" ? "失敗" : "実行中"}
            </span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 12.5, fontWeight: 500, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {job.progress.currentFile ?? job.jobId}
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
