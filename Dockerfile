FROM rust:1.88-bookworm AS builder

WORKDIR /usr/src/reasondb

COPY Cargo.toml Cargo.lock ./

# The Tauri desktop app workspace member is excluded via .dockerignore,
# so strip it from the workspace to avoid a Cargo resolution error.
RUN sed -i '/"apps\/reasondb-client\/src-tauri"/d' Cargo.toml

COPY crates/ crates/

RUN cargo build --release -p reasondb-server

# ---------------------------------------------------------------------------

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    gosu \
    python3 \
    python3-pip \
    && pip3 install --no-cache-dir --break-system-packages 'markitdown[all]' \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -r reasondb && useradd -r -g reasondb reasondb

RUN mkdir -p /data && chown reasondb:reasondb /data

COPY --from=builder /usr/src/reasondb/target/release/reasondb-server /usr/local/bin/reasondb-server
COPY docker-entrypoint.sh /usr/local/bin/

ENV REASONDB_HOST=0.0.0.0
ENV REASONDB_PORT=4444
ENV REASONDB_PATH=/data/reasondb.redb

EXPOSE 4444

VOLUME /data

HEALTHCHECK --interval=10s --timeout=5s --retries=3 --start-period=5s \
    CMD curl -f http://localhost:4444/health || exit 1

ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["reasondb-server"]
