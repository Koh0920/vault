# syntax=docker/dockerfile:1

# ---- frontend build stage ----
FROM node:20-alpine AS frontend-builder
WORKDIR /app
COPY frontend/package.json ./
COPY frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

# ---- backend build stage ----
FROM rust:1.80 AS server-builder
WORKDIR /build
COPY server/ ./server/
RUN cd server && cargo build --release

# ---- runtime image ----
FROM alpine:3.20
RUN apk add --no-cache rclone ca-certificates
WORKDIR /app
COPY --from=server-builder /build/server/target/release/vault-server /usr/local/bin/vault-server
COPY --from=frontend-builder /app/dist /app/frontend/dist

ENV VAULT_FRONTEND_DIR=/app/frontend/dist \
    VAULT_STATE_DIR=/data \
    VAULT_TEMP_DIR=/tmp \
    VAULT_LISTEN_ADDR=0.0.0.0:8080

EXPOSE 8080
VOLUME ["/data"]

ENTRYPOINT ["vault-server"]