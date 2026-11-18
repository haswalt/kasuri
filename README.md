# Kasuri

A Cloudflare-native workflow automation platform with a React UI, Rust workflow engine and a live-demo Worker host.

## Run locally

Use Node.js 24+, pnpm (version pinned in `package.json`), Rust, and the matching Wasm binding tool:

```sh
pnpm install
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
pnpm dev
```

`pnpm dev` builds the Rust/Wasm engine and React UI, then serves the complete application on `http://127.0.0.1:8787`. Cloudflare bindings use local storage. For frontend hot reload, keep that server running and run `pnpm dev:web` separately; Vite proxies `/api` to port 8787. Rebuild/restart the full app after changing Rust code.

## Try the demos

- **Core data demo:** Select Run to exercise real mapping, a 1.5-second durable wait, JSON stringify and JSON parse. Simulate executes the same nodes with a virtual clock and skips the elapsed wait.
- **Conditional routing demo:** Change `priority` in the test input to `urgent` or another value. Inspect the selected branch, skipped nodes and merge output.
- **Scheduled heartbeat:** Publish to register an every-minute schedule in `Europe/London`. Open Runs to see scheduled executions. Pause the workflow to unregister it. Cron uses five fields, with IANA timezones; the minimum cadence is one minute.

Select a node to change its configuration and outgoing connections. Save persists a draft to the backend. Unsaved edits stay in memory; saving a draft pauses a previously published schedule. Run and Simulate save unsaved edits before starting. Export/import supports version 1 graphs and typed condition ports. Each run records its own workflow snapshot and revision.

Nine implemented core node types are discovered from Rust manifests and marked Ready in the catalogue. HTTP, SaaS, AI and Code entries remain Planned and cannot be executed. Webhook nodes consume manual test payloads; public webhook ingress is not implemented.

## Deployment

```sh
pnpm exec wrangler login  # only if not already logged in
pnpm deploy:check
pnpm deploy
```

The tracked `wrangler.jsonc` deploys the React assets and Worker API together as `kasuri-demo`, with a SQLite-backed `DemoWorkspace` Durable Object. Wrangler provisions its namespace and applies the `v1` class migration. No external database, container, D1/R2 resources or SaaS credentials are required for these core demos.

See [deployment and API details](docs/deployment.md) and [engine contracts](docs/workflow-engine.md).

## Verification

```sh
pnpm test           # frontend, native Rust, Worker integration, Wasm boundary
pnpm check:api      # generated binding types + Worker source
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
pnpm deploy:check   # full build and deployment dry run
```

`pnpm types` refreshes the generated binding declarations after configuration changes. Generated Wasm artifacts are ignored and rebuilt from Rust source; `wasm-bindgen-cli` must match the version pinned in `crates/workflow-wasm/Cargo.toml`.

## Workspace layout

- `apps/web`: React/Vite frontend and configuration editor.
- `apps/api`: Worker API, per-browser durable workspace, persisted runs and cron scheduler.
- `packages/contracts`: shared TypeScript API/view contracts.
- `crates/workflow-schema`: versioned graph, node/retry contracts and DAG validation.
- `crates/connector-sdk`: task/result boundary and node manifests.
- `crates/connectors-core`: bounded declarative node implementations.
- `crates/workflow-engine`: deterministic scheduler, branches/joins, waits, retries/timeouts, controls and checkpoints.
- `crates/workflow-wasm`: JSON bridge exposing the Rust engine to the Worker.

## Demo scope

This is an anonymous demo with capability-based, isolated browser sessions, not account authentication or a shared team workspace. A random HttpOnly cookie selects a private workspace. Sessions and schedules expire after 24 hours; export workflows to keep them. Clearing the cookie loses access to that session. Use sample data rather than credentials or private production data.

The demo supports at most 20 workflows, five active/paused runs and three active schedules per session, with 100 recent terminal run records. Engine/graph payload budgets still apply. Implemented core nodes are pure and call no external services. Durability covers persisted wait/retry state and cron timers. External side-effect dispatch journaling, OAuth/vault, RBAC, webhook verification, queue ingestion, revision rollback and production provisioning remain future work.
