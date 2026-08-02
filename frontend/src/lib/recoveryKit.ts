export interface RecoveryKit {
  vaultId: string;
  keyFingerprint: string;
  recoveryKey: string;
}

const DB_NAME = "vault";
const DB_VERSION = 1;
const STORE = "recoveryKits";

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      if (!req.result.objectStoreNames.contains(STORE)) {
        req.result.createObjectStore(STORE, { keyPath: "vaultId" });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error("indexeddb open failed"));
  });
}

export async function saveRecoveryKit(kit: RecoveryKit): Promise<void> {
  const db = await openDb();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE, "readwrite");
      tx.objectStore(STORE).put(kit);
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error ?? new Error("save failed"));
    });
  } finally {
    db.close();
  }
}

export async function loadRecoveryKits(): Promise<RecoveryKit[]> {
  const db = await openDb();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(STORE, "readonly");
      const req = tx.objectStore(STORE).getAll();
      req.onsuccess = () => resolve((req.result as RecoveryKit[]) ?? []);
      req.onerror = () => reject(req.error ?? new Error("load failed"));
    });
  } finally {
    db.close();
  }
}

export async function removeRecoveryKit(vaultId: string): Promise<void> {
  const db = await openDb();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE, "readwrite");
      tx.objectStore(STORE).delete(vaultId);
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error ?? new Error("delete failed"));
    });
  } finally {
    db.close();
  }
}

export function downloadRecoveryKit(kit: RecoveryKit) {
  const blob = new Blob([JSON.stringify(kit, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `vault-recovery-kit-${kit.vaultId.slice(0, 8)}.json`;
  a.click();
  URL.revokeObjectURL(url);
}

export function parseRecoveryKit(text: string): RecoveryKit | null {
  try {
    const parsed = JSON.parse(text) as Partial<RecoveryKit>;
    if (
      typeof parsed.vaultId === "string" &&
      typeof parsed.keyFingerprint === "string" &&
      typeof parsed.recoveryKey === "string"
    ) {
      return parsed as RecoveryKit;
    }
    return null;
  } catch {
    return null;
  }
}
