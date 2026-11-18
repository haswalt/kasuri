# Kasuri

A visual workflow builder with a React/Vite frontend and a Rust backend compiled to WebAssembly. Run the same Rust Worker on Cloudflare or self-host it with the open-source workerd runtime.

<!-- deploy-button:start -->
[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/haswalt/kasuri)
<!-- deploy-button:end -->

There is no hosted demo. New installations start with an empty, persistent workspace.

## Local development

Use Node.js 24+, pnpm (pinned in `package.json`) and Rust. The build helper installs/checks the pinned Rust toolchain, Wasm target and official `worker-build` CLI.

```sh
pnpm install
pnpm dev
```

Open the local URL printed by Wrangler. For frontend-only development, run `pnpm dev:web` alongside the Worker.

```sh
pnpm build:selfhost
pnpm test
pnpm deploy:check
```

`deploy:check` packages the Worker without publishing it. `pnpm deploy` publishes to your own Cloudflare account. See [Cloudflare setup](docs/deployment.md), [one-click deployment](docs/cloudflare-one-click.md), and [Docker/Kubernetes self-hosting](docs/self-hosting.md).

## Architecture

- `apps/web`: React editor, draggable nodes, curved connections, run inspector and simulation controls.
- `packages/contracts`: frontend TypeScript types for JSON API responses.
- `crates/workflow-schema`: graph contracts and validation, including canvas positions.
- `crates/connector-sdk`: typed connector manifests and execution contracts.
- `crates/connectors-core`: pure core connectors.
- `crates/workflow-engine`: deterministic graph execution, retries, waits and checkpoints.
- `crates/workflow-service`: Rust workspace application, run management and timezone-aware cron scheduling.
- `crates/workflow-worker`: Rust HTTP handlers, session cookies, authentication, asset serving, SQLite Durable Objects and alarms using `workers-rs`.
- `deploy/workerd`: configuration for running the same Wasm Worker outside Cloudflare.

Cloudflare's official `worker-build` generates JavaScript bindings and a module entrypoint for the Wasm binary. This is generated tooling output; all backend application logic is Rust. There is no TypeScript API or Node server. See [Cloudflare's Rust documentation](https://developers.cloudflare.com/workers/languages/rust/).

## Current capabilities

Manual, webhook-payload and cron triggers; mapping, conditions, merge, delay, JSON utilities and error handling. Run executes durable waits; Simulate advances virtual time through the same Rust engine. Publishing a schedule registers a persistent alarm. The webhook node currently accepts run input; an external webhook ingress and HTTP/OAuth/SaaS dispatch adapters are not implemented.

A random HttpOnly cookie selects a browser workspace. Data persists until removed from storage; clearing the cookie loses access to that workspace. This is capability-based isolation, not user accounts or team membership. Protect private deployments with Cloudflare Access or the optional Basic authentication binding. External credential storage, account permissions and production operations remain future work.

Completed changes use Conventional Commits. See [engine details](docs/workflow-engine.md).
