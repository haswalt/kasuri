# syntax=docker/dockerfile:1
FROM rust:1.100-bookworm AS worker
WORKDIR /src
RUN rustup target add wasm32-unknown-unknown \
 && cargo install worker-build --version 0.9.0 --locked
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN worker-build crates/workflow-worker --release --no-opt --locked

FROM node:24-bookworm-slim AS application
WORKDIR /src
RUN npm install --global pnpm@12.21.0
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/web/package.json apps/web/panda.config.ts apps/web/
COPY apps/web/theme ./apps/web/theme
RUN pnpm install --frozen-lockfile
COPY apps/web ./apps/web
COPY packages ./packages
RUN pnpm build:web \
 && cp node_modules/workerd/bin/workerd /tmp/workerd

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --gid 1000 kasuri && useradd --uid 1000 --gid 1000 kasuri \
 && mkdir /data && chown kasuri:kasuri /data
WORKDIR /app
COPY --from=application /tmp/workerd /usr/local/bin/workerd
COPY --from=application /src/apps/web/dist ./public
COPY --from=worker /src/crates/workflow-worker/build/index.js ./worker/index.js
COPY --from=worker /src/crates/workflow-worker/build/index_bg.wasm ./worker/index_bg.wasm
COPY deploy/workerd/workerd.capnp ./workerd.capnp
COPY --chmod=755 scripts/container-entrypoint.sh /usr/local/bin/kasuri
USER kasuri
VOLUME ["/data"]
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
 CMD curl --fail --silent http://127.0.0.1:8080/api/health > /dev/null || exit 1
ENTRYPOINT ["/usr/local/bin/kasuri"]
