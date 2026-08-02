import type {
  DriveStatus,
  InitializeResponse,
  ListFilesResponse,
  PreviewResponse,
  RuntimeConfig,
  UnlockResponse,
  UploadResult,
  VaultStatus,
} from "../types";

const BASE = "/api/v1";

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(BASE + path, {
    credentials: "same-origin",
    ...init,
  });
  if (!resp.ok) {
    let message: string;
    try {
      const body = await resp.json();
      message = typeof body?.error === "string" ? body.error : `HTTP ${resp.status}`;
    } catch {
      message = `HTTP ${resp.status}`;
    }
    throw new Error(message);
  }
  return (await resp.json()) as T;
}

export const api = {
  runtime(): Promise<RuntimeConfig> {
    return request("/runtime");
  },

  driveStatus(): Promise<DriveStatus> {
    return request("/drive/status");
  },

  async startDrive(redirectUrl?: string): Promise<string> {
    const qs = redirectUrl ? `?redirectUrl=${encodeURIComponent(redirectUrl)}` : "";
    const resp = await request<{ ok: boolean; url: string }>(`/drive/oauth/start${qs}`);
    return resp.url;
  },

  async disconnect(): Promise<void> {
    await request("/drive/disconnect", { method: "POST" });
  },

  vaultStatus(): Promise<VaultStatus> {
    return request("/vault");
  },

  initialize(): Promise<InitializeResponse> {
    return request("/vault/initialize", { method: "POST", headers: { "Content-Type": "application/json" }, body: "{}" });
  },

  unlock(recoveryKey: string): Promise<UnlockResponse> {
    return request("/vault/unlock", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ recoveryKey }),
    });
  },

  listFiles(path: string): Promise<ListFilesResponse> {
    const qs = path ? `?path=${encodeURIComponent(path)}` : "";
    return request(`/files${qs}`);
  },

  preview(path: string): Promise<PreviewResponse> {
    const qs = path ? `?path=${encodeURIComponent(path)}` : "";
    return request(`/files/preview${qs}`, { method: "POST" });
  },

  async uploadFiles(files: File[], dir: string): Promise<UploadResult[]> {
    const form = new FormData();
    for (const file of files) {
      const name = dir ? `${dir}/${file.name}` : file.name;
      form.append("file", file, name);
    }
    const resp = await request<{ uploaded: UploadResult[] }>("/uploads", { method: "POST", body: form });
    return resp.uploaded;
  },
};