import type { ExplorerEntry, JobState, PreviewResult, UploadIndexEntry } from "../types";

export function formatBytes(bytes: number | null | undefined): string {
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

export function formatTime(value: string | null | undefined): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString("ja-JP");
}

export function explorerTypeLabel(entry: Partial<ExplorerEntry & UploadIndexEntry & PreviewResult> & { isDir?: boolean; itemType?: string }): string {
  if (entry.isDir || entry.itemType === "directory") return "folder";
  const path = (entry.displayName || entry.name || "").toLowerCase();
  if (path.endsWith(".pdf")) return "pdf";
  if (path.endsWith(".md") || path.endsWith(".markdown")) return "markdown";
  if (path.endsWith(".csv")) return "csv";
  if (path.endsWith(".json") || path.endsWith(".js") || path.endsWith(".ts") || path.endsWith(".tsx") || path.endsWith(".jsx") || path.endsWith(".py") || path.endsWith(".rs")) return "code";
  if (path.endsWith(".png") || path.endsWith(".jpg") || path.endsWith(".jpeg") || path.endsWith(".gif") || path.endsWith(".webp") || path.endsWith(".svg")) return "image";
  return "file";
}

export function explorerKindLabel(entry: Partial<ExplorerEntry & UploadIndexEntry & PreviewResult> & { isDir?: boolean; itemType?: string }): string {
  const type = explorerTypeLabel(entry);
  switch (type) {
    case "folder":
      return "Folder";
    case "pdf":
      return "PDF Document";
    case "markdown":
      return "Markdown";
    case "csv":
      return "CSV File";
    case "code":
      return "Code File";
    case "image":
      return "Image File";
    default:
      return "File";
  }
}

export function fileGlyphLabel(type: string): string {
  if (type === "folder") return "▣";
  if (type === "pdf") return "PDF";
  if (type === "markdown") return "MD";
  if (type === "csv") return "CSV";
  if (type === "code") return "</>";
  if (type === "image") return "IMG";
  return "FILE";
}

export function progressPercent(job: JobState | null): number | null {
  if (!job?.progress.bytesTotal || job.progress.bytesTotal <= 0) return null;
  return Math.max(0, Math.min(100, (job.progress.bytesDone / job.progress.bytesTotal) * 100));
}

export function historyJobs(jobsById: Record<string, JobState>, jobOrder: string[]): JobState[] {
  return jobOrder
    .map((jobId) => jobsById[jobId])
    .filter((job): job is JobState => Boolean(job) && job.kind !== "preview");
}
