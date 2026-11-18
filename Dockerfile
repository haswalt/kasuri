# syntax=docker/dockerfile:1
FROM rust:1.100-bookworm AS engine
WORKDIR /src
RUN rustup target add wasm32-unknown-unknown \
 && cargo install wasm-bindgen-cli --version 0.2.130 --locked
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build -p workflow-wasm --release --target wasm32-unknown-unknown --locked \
 && wasm-bindgen target/wasm32-unknown-unknown/release/workflow_wasm.wasm --target web --out-dir /out --out-name engine

FROM node:24-bookworm-slim AS application
WORKDIR /src
RUN npm install --global pnpm@12.21.0
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/web/package.json apps/web/package.json
RUN pnpm install --frozen-lockfile
COPY . .
COPY --from=engine /out ./apps/api/generated
RUN pnpm build:web && pnpm check:server && pnpm build:server

FROM node:24-bookworm-slim AS runtime
ENV NODE_ENV=production PORT=8080 HOST=0.0.0.0 DATA_DIR=/data ASSETS_DIR=/app/public
WORKDIR /app
COPY --from=application --chown=node:node /src/dist/server ./server
COPY --from=application --chown=node:node /src/apps/web/dist ./public
RUN mkdir /data && chown node:node /data
USER node
VOLUME ["/data"]
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
 CMD node -e "fetch('http://127.0.0.1:8080/api/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"
CMD ["node", "server/main.mjs"]
