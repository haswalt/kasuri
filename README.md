# Kasuri

A visual workflow builder with a React/Vite frontend and a Rust backend compiled to WebAssembly. Run the same Rust Worker on Cloudflare or self-host it with the open-source workerd runtime.

> **Self-hosting is single-node.** The Docker, Compose and Kubernetes packages run one workerd process with all workspace state in SQLite on one local volume. Scale it up with more CPU, memory and disk, not by adding replicas: replicas cannot share the volume, and separate volumes become separate installations. On Cloudflare each workspace runs as its own Durable Object, so workspaces spread across machines automatically. On both platforms a single workspace runs at most 5 active runs at a time. See [scaling and availability](docs/self-hosting.md#scaling-and-availability).

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

Try the [importable example workflows](examples/README.md) after installing Kasuri. They cover every supported connector, range from simple to combined scenarios, and include live public API examples with offline simulation fixtures.

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

Manual, optionally authenticated webhook and cron triggers; mapping, conditions, merge, delay, JSON utilities and error handling. Run executes durable waits; Simulate advances virtual time through the same Rust engine. Publishing a schedule registers a persistent alarm. Webhook endpoints support public access, bearer/query keys and timestamped HMAC signatures. Business-event deduplication is explicit workflow logic using atomic data claims; see [webhook setup](docs/webhooks.md). OAuth and SaaS adapters are not implemented. Data Tables and Context Store provide transactional persistent data and isolated simulation snapshots. HTTP Request supports public HTTPS APIs by default, Basic auth through named secrets, and custom headers; Filter and Sort process response arrays. See [connector setup and examples](docs/connectors.md).

An HttpOnly cookie identifies a browser ownership session with a separate random session token. The workspace menu creates, renames and switches isolated workspaces; the URL selects a workspace per tab, without changing the cookie. Existing Personal workspaces remain accessible. Workflows, runs, MCP credentials and webhook endpoints are isolated per workspace. Clearing the cookie loses access to all workspaces owned by that session. This is capability-based ownership, not user accounts or shared team membership. Protect private deployments with Cloudflare Access or the optional Basic authentication binding. External credential storage, account permissions and production operations remain future work.

## Agent management through MCP

Kasuri exposes a Rust/Wasm MCP endpoint for agents to build and manage workflows, simulate or execute runs, and read retained audit records. The MCP portal creates revocable, scoped bearer credentials and approves OAuth connections using PKCE. Read, edit, simulation, publication, execution and run control are separate permissions. See [connection and authorization setup](docs/mcp.md) and the [remaining MCP requirements](docs/agent-mcp.md). This inbound endpoint is separate from the planned MCP Client workflow connector. [Agentic connector requirements](docs/agentic-connectors.md) include Cloudflare AI Gateway and Cloudflare MCP Portals as model-routing and tool-access options.

Completed changes use Conventional Commits. See [engine details](docs/workflow-engine.md).

The [UI implementation notes](docs/ui-implementation.md) describe the design handoff, implemented screens and remaining backend dependencies. See [styling the UI](docs/styling.md) for Panda CSS tokens and recipes.

The [internal platform implementation plan](docs/internal-platform-plan.md) covers reported product issues, publication/history/workspace semantics, flexible webhooks, Data Tables, parallel/items, expressions/Code and workflow composition. External service connectors are excluded from that implementation sequence.

The [HTTP management API v2](docs/http-api.md) requires edit versions for existing workflow updates, publication/activation and execution requests. This removes the previous unversioned-write compatibility bypass; older REST integrations must fetch and submit workflow versions.
