import { progressPercent } from "../lib/format";
import type { JobState } from "../types";
import { iconByName } from "./Icons";

export function JobBar({ job }: { job: JobState | null }) {
  const percent = progressPercent(job);
  if (!job) return null;

  return (
    <div id="job-bar" className="vis">
      <div className="jb-icon">{iconByName("upload")}</div>
      <div className="jb-info">
        <div className="jb-file">{job.progress.currentFile ?? job.jobId}</div>
        <div className="jb-track">
          <div
            className={`jb-fill ${percent == null ? "indeterminate" : ""}`}
            style={percent == null ? undefined : { width: `${percent}%` }}
          />
        </div>
      </div>
      <div className="jb-meta">
        <span>{job.progress.speed ? `${job.progress.speed} B/s` : "—"}</span>
        <span>ETA {job.progress.eta ? `${job.progress.eta}s` : "—"}</span>
      </div>
      <span className={`phase-badge ${job.phase === "done" ? "done" : job.phase === "failed" ? "fail" : "run"}`}>
        {job.phase === "done" ? "完了" : job.phase === "failed" ? "失敗" : "実行中"}
      </span>
    </div>
  );
}
