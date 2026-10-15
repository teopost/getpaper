# ---------- frontend ----------
FROM node:22-alpine AS web
WORKDIR /web
COPY web/package.json web/package-lock.json* ./
RUN npm install --no-audit --no-fund
COPY web/ ./
RUN npm run build

# ---------- backend ----------
FROM rust:1-bookworm AS server
WORKDIR /app
COPY server/Cargo.toml server/Cargo.lock* ./
# pre-build dependencies for better layer caching
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src
COPY server/ ./
RUN touch src/main.rs && cargo build --release

# ---------- runtime ----------
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 1001 -m getpaper \
    && mkdir -p /data/uploads && chown -R getpaper /data
WORKDIR /app
COPY --from=server /app/target/release/getpaper /app/getpaper
COPY --from=web /web/dist /app/public
ENV STATIC_DIR=/app/public \
    UPLOAD_DIR=/data/uploads \
    PORT=3000
USER getpaper
EXPOSE 3000
HEALTHCHECK --interval=15s --timeout=3s --start-period=20s CMD curl -fsS -X POST http://localhost:3000/api/auth.config || exit 1
CMD ["/app/getpaper"]
