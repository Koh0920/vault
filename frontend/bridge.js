function coreInvoke(command, payload = {}) {
  const invoke = window.__TAURI_INTERNALS__?.invoke;
  if (typeof invoke !== "function") {
    throw new Error("No Tauri invoke bridge detected.");
  }
  return invoke(command, payload);
}

export function createBackupBridge() {
  return {
    hasBridge() {
      return typeof window.__TAURI_INTERNALS__?.invoke === "function";
    },

    async checkGuestMode() {
      const result = await coreInvoke("check_env", {});
      return {
        mode: result.ato_guest_mode ?? result.result?.ato_guest_mode ?? null,
        raw: result,
      };
    },

    async ping() {
      return coreInvoke("ping", {
        payload: { message: "hello from encrypted-r2-drop" },
      });
    },

    async getRuntimeConfig() {
      return coreInvoke("get_runtime_config", {});
    },

    async pickFolder() {
      return coreInvoke("pick_folder", {});
    },

    async getProviders() {
      return coreInvoke("get_providers", {});
    },

    async getProviderStatuses() {
      return coreInvoke("get_provider_statuses", {});
    },

    async listUploadIndex() {
      return coreInvoke("list_upload_index", {});
    },

    async findUploadIndexEntry(provider, remoteItemPath) {
      return coreInvoke("find_upload_index_entry", {
        payload: {
          provider,
          remoteItemPath,
        },
      });
    },

    async listExplorerEntries(uploadId, path = "", mode = "decrypted", query = "", offset = 0, limit = 200, refresh = false) {
      return coreInvoke("list_explorer_entries", {
        payload: {
          uploadId,
          path,
          mode,
          query,
          offset,
          limit,
          refresh,
        },
      });
    },

    async startDownloadExplorerItem(uploadId, path = "") {
      return coreInvoke("start_download_explorer_item", {
        payload: {
          uploadId,
          path,
        },
      });
    },

    async startPreviewExplorerItem(uploadId, path = "") {
      return coreInvoke("start_preview_explorer_item", {
        payload: {
          uploadId,
          path,
        },
      });
    },

    async listJobs(kind = null, status = null, limit = 50) {
      return coreInvoke("list_jobs", {
        payload: {
          kind,
          status,
          limit,
        },
      });
    },

    async connectProvider(provider) {
      return coreInvoke("connect_provider", { payload: { provider } });
    },

    async createCryptRemote(baseRemote, suffix = "-crypt", password = "") {
      return coreInvoke("create_crypt_remote", {
        payload: {
          baseRemote,
          cryptSuffix: suffix,
          password,
        },
      });
    },

    async startUpload(sourcePath, remoteName, remotePath, mode = "copy") {
      return coreInvoke("start_upload", {
        payload: {
          sourcePath,
          remoteName,
          remotePath,
          mode,
        },
      });
    },

    async getJobStatus(jobId) {
      return coreInvoke("get_job_status", { payload: { jobId } });
    },
  };
}
