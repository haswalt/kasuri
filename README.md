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
- `crates/connectors-core`: pure core connectors, including filtering and sorting.
- `crates/connectors-webhook`: inbound event authentication and signature verification.
- `crates/connectors-http`: HTTP request configuration, outbound policy and response handling.
- `crates/workflow-engine`: deterministic graph execution, retries, waits and checkpoints.
- `crates/workflow-service`: Rust workspace application, run management and timezone-aware cron scheduling.
- `crates/workflow-worker`: Rust HTTP handlers, session cookies, authentication, asset serving, SQLite Durable Objects and alarms using `workers-rs`.
- `deploy/workerd`: configuration for running the same Wasm Worker outside Cloudflare.

Cloudflare's official `worker-build` generates JavaScript bindings and a module entrypoint for the Wasm binary. This is generated tooling output; all backend application logic is Rust. There is no TypeScript API or Node server. See [Cloudflare's Rust documentation](https://developers.cloudflare.com/workers/languages/rust/).

## Current capabilities

Manual, authenticated webhook and cron triggers; mapping, conditions, merge, delay, JSON utilities and error handling. Run executes durable waits; Simulate advances virtual time through the same Rust engine. Publishing a schedule registers a persistent alarm. Webhook endpoints support bearer authentication, timestamped HMAC signatures and durable event deduplication; see [webhook setup](docs/webhooks.md). OAuth and SaaS adapters are not implemented. HTTP Request supports public HTTPS APIs by default, Basic auth through named secrets, and custom headers; Filter and Sort process response arrays. See [connector setup and examples](docs/connectors.md).

An HttpOnly cookie with a workspace routing ID and a separate random session token selects a browser workspace. Data persists until removed from storage; clearing the cookie loses access to that workspace. This is capability-based isolation, not user accounts or team membership. Protect private deployments with Cloudflare Access or the optional Basic authentication binding. External credential storage, account permissions and production operations remain future work.

## Planned agent interface

Kasuri must expose an authenticated MCP server so agents can build and manage workflows, execute and inspect runs, and audit changes and execution history. This is a planned Rust backend capability, with workspace-scoped permissions, revision checks, redacted results and attributable audit records. See [agent MCP requirements](docs/agent-mcp.md). It is separate from the planned MCP Client workflow connector. [Agentic connector requirements](docs/agentic-connectors.md) include Cloudflare AI Gateway and Cloudflare MCP Portals as model-routing and tool-access options.

Completed changes use Conventional Commits. See [engine details](docs/workflow-engine.md).
