FROM rust:1.88-alpine AS chef

RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static perl make curl
RUN cargo install cargo-chef --locked

WORKDIR /usr/src/reasondb

# ---------- Plan: extract dependency info (cached unless Cargo.toml/lock change) ----------

FROM chef AS planner

COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Strip the Tauri desktop app (excluded via .dockerignore)
RUN sed -i '/"apps\/reasondb-client\/src-tauri"/d' Cargo.toml

RUN cargo chef prepare --recipe-path recipe.json

# ---------- Cook: build only dependencies (cached Docker layer) ----------

FROM chef AS builder

COPY --from=planner /usr/src/reasondb/recipe.json recipe.json

RUN cargo chef cook --release --recipe-path recipe.json -p reasondb-server

# ---------- Build: compile source (only this re-runs on code changes) ----------

COPY Cargo.toml Cargo.lock ./
RUN sed -i '/"apps\/reasondb-client\/src-tauri"/d' Cargo.toml
COPY crates/ crates/

RUN cargo build --release -p reasondb-server

# ---------------------------------------------------------------------------

FROM alpine:3.21

# Supported plugin runtimes: python3, node, bash/sh, compiled binaries
RUN apk add --no-cache \
    ca-certificates \
    curl \
    su-exec \
    bash \
    python3 \
    py3-pip \
    nodejs \
    npm \
    && pip3 install --no-cache-dir --break-system-packages 'markitdown[all]' \
    && rm -rf /root/.cache

RUN addgroup -S reasondb && adduser -S -G reasondb reasondb

RUN mkdir -p /data /plugins && chown -R reasondb:reasondb /data /plugins

COPY --from=builder /usr/src/reasondb/target/release/reasondb-server /usr/local/bin/reasondb-server
COPY docker-entrypoint.sh /usr/local/bin/

# Copy built-in plugins
COPY plugins/ /plugins/

ENV REASONDB_HOST=0.0.0.0
ENV REASONDB_PORT=4444
ENV REASONDB_PATH=/data/reasondb.redb
ENV REASONDB_PLUGINS_DIR=/plugins

EXPOSE 4444

VOLUME /data

HEALTHCHECK --interval=10s --timeout=5s --retries=3 --start-period=5s \
    CMD curl -f http://localhost:4444/health || exit 1

ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["reasondb-server"]
