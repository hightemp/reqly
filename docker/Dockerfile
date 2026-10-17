# syntax=docker/dockerfile:1.7
# Reqly server: web UI embedded in a single binary.

FROM node:22-bookworm-slim AS ui
WORKDIR /src
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY ui/package.json ui/package.json
COPY apps/desktop/package.json apps/desktop/package.json
RUN pnpm install --frozen-lockfile --filter @reqly/ui... --ignore-scripts
COPY ui ui
RUN pnpm -C ui build

FROM rust:1-bookworm AS server
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends cmake perl && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY .cargo .cargo
COPY crates crates
# All workspace manifests must exist; only the server is compiled.
COPY apps apps
COPY spikes spikes
COPY --from=ui /src/ui/dist ui/dist
RUN cargo build --release -p reqly-server --locked \
 && strip target/release/reqly-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /data reqly && mkdir -p /data && chown reqly /data
COPY --from=server /src/target/release/reqly-server /usr/local/bin/reqly-server
USER reqly
ENV REQLY_BIND=0.0.0.0:8484 REQLY_DATA_DIR=/data
VOLUME ["/data"]
EXPOSE 8484
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD ["reqly-server", "healthcheck"]
ENTRYPOINT ["reqly-server"]
