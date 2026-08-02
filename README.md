# Vault — Encrypted Backup to Google Drive

Vault is a standalone, local-first web application that backs up files to your
own Google Drive, encrypted with `rclone crypt`. It is a single Axum HTTP
server (Rust) that serves a React frontend and talks to an rclone binary with
a `crypt` remote.

The restore key is a recovery key derived strictly from your master key and
held only on the server / your session, so the encrypted contents are
unreadable to any service other than you.

## Architecture

```
Browser (React) ── HTTP ──► Axum server (Rust) ── rclone ──► Google Drive
                              │  crypto: HKDF + XChaCha20-Poly1305 envelopes
                              │  key envelope + recovery key
                              └  session cookie (HMAC-signed)
```

- **Frontend** — `frontend/` (Vite + React + TypeScript); talks only to the
  local HTTP API over `fetch`.
- **Backend** — `server/` (Rust, Axum); owns sessions, the key envelope, the
  recovery key, and rclone invocation. Files are streamed to temp and then into
  the crypt remote; they are never held fully in memory.
- **Transfer** — rclone `crypt` remote over Google Drive. File contents and
  names are both encrypted.
- **Packaging** — multi-stage `Dockerfile` bundles the server, the built
  frontend, and rclone. `capsule.toml` exposes it as an OCI web capsule on
  port 8080.

## Key & recovery design

- A **master key** is generated for the vault and wrapped into a **key
  envelope** (HKDF-derived XChaCha20-Poly1305 AEAD key) stored on Drive as
  `Vault/key-envelope.v2.json`.
- The envelope schema is strictly validated (version, algorithm, KDF, salt /
  nonce / ciphertext lengths) before decryption, and the vault id is bound
  into the AEAD as associated data so an envelope can't be replayed against a
  different vault.
- Legacy `key-envelope.v1.json` (pre-AAD) envelopes are still readable on
  unlock and are migrated to the current v2 format automatically, so existing
  vaults keep working.
- The **recovery key** is shown exactly once at vault creation and can be
  exchanged for the master key later. It is persisted to IndexedDB for this
  browser origin and can be downloaded as a recovery kit JSON for import on a
  new device.
- The master key's fingerprint is verified against the manifest on unlock.

## Security model

- **Session isolation** — every session owns its own rclone config
  (`state/rclone/<session-id>/rclone.conf`), so parallel sessions can never
  overwrite each other's Google OAuth token or crypt password. All rclone work
  for a session is serialized by a per-session lock so concurrent requests
  can't lose a config section. Disconnect removes only the owning session's
  config.
- **Path resolution** — plain reads/writes resolve directly to `drive:<path>`;
  encrypted ops resolve into the crypt root `drive-crypt:<path>` (which maps to
  `drive:Vault/cipher`). No implicit prefix is added, so metadata never lands
  doubled under `Vault/cipher/Vault/...`.
- **Session cookies** are HMAC-signed. Off loopback the server refuses to run
  without a `VAULT_COOKIE_SECRET` of at least 32 bytes; `Secure` is set when
  `VAULT_COOKIE_SECRET`/`VAULT_COOKIE_SECURE` indicate HTTPS. Expired sessions
  are removed and their key material zeroized.
- **Resource limits** — request body, per-upload bytes, file count, and preview
  size are capped. Temp files are removed via RAII guards, and blocking rclone /
  filesystem work runs on the blocking pool, not the async runtime.
- There is no permissive CORS layer; the frontend is served same-origin.

## Development

Prerequisites: Rust (stable), Node 20+, and `rclone` on the `PATH`.

Create a Google OAuth Desktop/Web app with a redirect URI of
`http://localhost:8080/api/v1/drive/oauth/callback`, then:

```bash
cp .env.example .env        # fill in GOOGLE_CLIENT_ID / SECRET
cd server && cargo run      # backend on :8080, serves ./frontend/dist
```

In a second terminal, for hot reload:

```bash
npm install && npm run dev  # vite dev server on :1420, proxies /api to :8080
```

### Test / lint

```bash
cd server
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

```bash
npm run build && npx tsc --noEmit
```

## Docker

```bash
docker build -t vault .
docker run --rm -p 8080:8080 \
  -v vault-data:/data \
  -e GOOGLE_CLIENT_ID=... \
  -e GOOGLE_CLIENT_SECRET=... \
  -e GOOGLE_REDIRECT_URI=http://localhost:8080/api/v1/drive/oauth/callback \
  -e VAULT_COOKIE_SECRET=$(openssl rand -hex 32) \
  vault
# open http://localhost:8080
```

The multi-stage build is verified: it builds the Rust server and React
frontend, downloads rclone with a SHA-256 check against the official
checksums, and the resulting image boots and serves the app. The release
workflow (`release.yml`) pushes the image to `ghcr.io/koh0920/vault` and
pins the `@sha256:...` digest into `capsule.toml`.

## Environment variables

See `.env.example`. Notable keys:

| Variable | Description |
| --- | --- |
| `VAULT_LISTEN_ADDR` | Bind address, default `0.0.0.0:8080` |
| `VAULT_FRONTEND_DIR` | Built frontend directory, default `../frontend/dist` |
| `VAULT_STATE_DIR` | Persistent state (rclone config, metadata) |
| `VAULT_TEMP_DIR` | Temp dir for streaming uploads |
| `RCLONE_BINARY` | Path/name of the rclone binary |
| `GOOGLE_CLIENT_ID` | Google OAuth client id |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret |
| `GOOGLE_REDIRECT_URI` | OAuth redirect (must match the app config) |
| `VAULT_COOKIE_SECRET` | HMAC secret for session cookies |

## HTTP API

All routes are under `/api/v1` and use a same-origin session cookie.

| Method | Path | Description |
| --- | --- | --- |
| GET | `/api/v1/runtime` | Public runtime config |
| GET | `/api/v1/drive/status` | Public Drive connection status |
| GET | `/api/v1/drive/oauth/start` | Begin Drive OAuth (sets session cookie) |
| GET | `/api/v1/drive/oauth/callback` | OAuth callback (server-side exchange) |
| POST | `/api/v1/drive/disconnect` | Drop Drive credentials |
| GET | `/api/v1/vault` | Vault status (exists / initialized) |
| POST | `/api/v1/vault/initialize` | Create vault, returns recovery key |
| POST | `/api/v1/vault/unlock` | Unlock with recovery key |
| GET | `/api/v1/files?path=` | List encrypted files in a folder |
| POST | `/api/v1/files/preview?path=` | Preview a text file |
| POST | `/api/v1/uploads` | Multipart upload, encrypted to vault |

Uploads are synchronous within the request (rclone `copyto` completes before
the response). There is no job/cancel API.

## Notes

- This is the standalone web evolution of the original
  `byok-encrypted-r2-drop` Tauri sample: Google Drive is now the only
  provider, there is no R2, and there is no desktop shell.