# syntax=docker/dockerfile:1

# ---- frontend build stage ----
FROM node:20-alpine AS frontend-builder
WORKDIR /app
# package.json + lockfile live at the repo root (vite root is frontend/)
COPY package.json package-lock.json ./
COPY tsconfig.json vite.config.ts ./
COPY frontend/ ./frontend/
RUN npm ci && npm run build

# ---- rclone download stage (isolated; runtime gets only the binary) ----
# Pin rclone and verify its SHA-256 against the official checksums file.
# Note: the ARG is NOT named RCLONE_* because rclone reads env vars with that
# prefix as config overrides (RCLONE_VERSION would collide with --version).
FROM alpine:3.20 AS rclone-stage
ARG TARGETARCH
ARG RCLONE_RELEASE_VERSION=1.73.3
RUN apk add --no-cache curl unzip \
    && arch="${TARGETARCH:-amd64}" \
    && if [ "$arch" = "amd64" ]; then a=amd64; elif [ "$arch" = "arm64" ]; then a=arm64; else echo "unsupported arch $arch" && exit 1; fi \
    && url="https://downloads.rclone.org/v${RCLONE_RELEASE_VERSION}/rclone-v${RCLONE_RELEASE_VERSION}-linux-${a}.zip" \
    && curl -fsSL -o /tmp/rclone.zip "$url" \
    && curl -fsSL -o /tmp/rclone.sha256 "https://downloads.rclone.org/v${RCLONE_RELEASE_VERSION}/SHA256SUMS" \
    && expected=$(grep "rclone-v${RCLONE_RELEASE_VERSION}-linux-${a}.zip" /tmp/rclone.sha256 | awk '{print $1}') \
    && echo "${expected}  /tmp/rclone.zip" | sha256sum -c - \
    && unzip -o /tmp/rclone.zip -d /tmp/rclone-extract \
    && cp "/tmp/rclone-extract/rclone-v${RCLONE_RELEASE_VERSION}-linux-${a}/rclone" /usr/local/bin/rclone \
    && chmod +x /usr/local/bin/rclone

# ---- backend build stage ----
# The committed Cargo.lock resolves quinn-proto -> rand 0.10 -> rand_pcg
# (edition2024, needs rustc >= 1.85) and time 0.3.55 / icu 2.2 (need rustc
# >= 1.88). Use a recent stable toolchain that satisfies the lockfile MSRV.
FROM rust:1.96-bookworm AS server-builder
WORKDIR /build
COPY server/ ./server/
RUN cargo build --manifest-path server/Cargo.toml --release

# ---- runtime image ----
# Minimal: only ca-certificates and the two binaries, run as a non-root user.
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 vault \
    && useradd --uid 10001 --gid vault --no-create-home --shell /usr/sbin/nologin vault

COPY --from=rclone-stage /usr/local/bin/rclone /usr/local/bin/rclone
COPY --from=server-builder /build/server/target/release/vault-server /usr/local/bin/vault-server
COPY --from=frontend-builder /app/frontend/dist /app/frontend/dist

ENV VAULT_FRONTEND_DIR=/app/frontend/dist \
    VAULT_STATE_DIR=/data \
    VAULT_TEMP_DIR=/tmp \
    VAULT_LISTEN_ADDR=0.0.0.0:8080

RUN mkdir -p /data /app/frontend/dist \
    && chown -R vault:vault /data

USER vault
EXPOSE 8080
VOLUME ["/data"]

ENTRYPOINT ["vault-server"]
