# Kasuri

<!-- deploy-button:start -->
[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/haswalt/kasuri)
<!-- deploy-button:end -->

[![Build and test](https://github.com/haswalt/kasuri/actions/workflows/ci.yaml/badge.svg?branch=develop)](https://github.com/haswalt/kasuri/actions/workflows/ci.yaml)

Kasuri is a visual workflow automation platform that runs on Cloudflare. You build workflows on a canvas in the browser, trigger them manually, on a schedule or from webhooks, and watch every run step by step. The execution engine is written in Rust and compiled to WebAssembly, and it runs on Cloudflare Workers and Durable Objects.

There is no hosted demo: every installation is your own, and starts with an empty workspace.

## Features

- **Visual editor.** Drag nodes onto a canvas, connect them, and configure each step with guided forms.
- **Durable execution.** Runs survive restarts. Waits, retries and schedules are persisted, and each step's input and output can be inspected.
- **Simulation.** Simulate a workflow against the real engine with virtual time and isolated data, without calling external services.
- **Triggers.** Manual runs, cron schedules (timezone-aware), and webhooks with bearer, query-key or HMAC authentication.
- **27 built-in connectors.** Data mapping, expressions, conditions and routing, parallel branches, item loops, HTTP requests, persistent data tables and calling other workflows.
- **Publication and history.** Drafts are separate from published revisions, so editing never changes what is running. Past revisions can be restored.
- **Agent access.** A built-in [MCP](https://modelcontextprotocol.io) server lets AI agents build, validate, simulate and run workflows with scoped, revocable credentials.
- **Isolated workspaces.** Workflows, runs, data and credentials are separated per workspace.

## Quick start

### Deploy to Cloudflare

Select **Deploy to Cloudflare** above, or deploy from a clone with Wrangler. See [Cloudflare deployment](docs/deployment.md) for configuration, authentication and limits, and [one-click deployment](docs/cloudflare-one-click.md) for how the button works.

### Run locally

Requirements: Node.js 24 or later, [pnpm](https://pnpm.io) (the version is pinned in `package.json`) and [Rust](https://rustup.rs). The build installs the pinned Rust toolchain, WebAssembly target and `worker-build` if they are missing.

```sh
pnpm install
pnpm dev
```

Open the URL Wrangler prints. For frontend-only work, run `pnpm dev:web` alongside it.

| Command | What it does |
| --- | --- |
| `pnpm dev` | Build and run the Worker and UI locally with Wrangler |
| `pnpm test` | Run the web, Rust and runtime test suites |
| `pnpm deploy:check` | Package the Worker without publishing it |
| `pnpm deploy` | Publish to your Cloudflare account |

## Building workflows

Start with the [example workflows](examples/README.md). They cover every connector, need no accounts or API keys, and can be imported from **Workflows → Import JSON**.

| Category | Connectors | Guide |
| --- | --- | --- |
| Triggers | Manual Trigger, Schedule, Webhook, Execute Workflow Trigger | [Webhooks](docs/webhooks.md), [Calling workflows](docs/workflow-invocation.md) |
| Shaping data | Map / Transform, Expressions, JSON Utilities, Parse Body, Validate Schema, Request Context | [Expressions and routing](docs/expressions.md), [Parsing](docs/parsing.md), [Validation](docs/validation.md) |
| Flow control | IF / Condition, Switch / Router, Merge, Wait / Delay, Error Handler | [Conditions](docs/conditions.md), [Expressions and routing](docs/expressions.md) |
| Parallelism and items | Split / Parallel, Join, Filter, Sort, Split / Batch, Aggregate, Each / Loop | [Parallel branches](docs/parallel.md), [Item processing](docs/items.md) |
| Integration | HTTP Request, Execute Workflow, Respond to Webhook | [Connectors](docs/connectors.md), [Webhooks](docs/webhooks.md) |
| Persistent data | Data Tables, Context Store | [Data storage](docs/data-storage.md), [Import and export](docs/data-transfer.md) |

The [connector reference](docs/connectors.md) describes every connector's configuration, and the [workflow engine](docs/workflow-engine.md) explains how runs execute.

## Agents and automation

Agents connect to each workspace's MCP endpoint with scoped bearer tokens or OAuth (with PKCE). Reading, editing, simulation, publication, execution and run control are separate permissions. See [connecting agents](docs/mcp.md).

Programs can also use the versioned [HTTP management API](docs/http-api.md).

## Architecture

```text
Browser / agents / webhook senders
        │
        ▼
Worker (Rust → Wasm)       authentication and routing
        │
        ▼
Workspace Durable Object   workflows, runs, schedules, data (SQLite)
        │
        ▼
Workflow engine            deterministic execution, retries, waits, checkpoints
```

All backend logic is Rust. Cloudflare's `worker-build` only generates the JavaScript glue that loads the WebAssembly module; there is no Node server.

| Path | Purpose |
| --- | --- |
| `apps/web` | React editor, run inspector and simulation controls |
| `packages/contracts` | TypeScript types for API responses |
| `crates/workflow-schema` | Workflow graph format and validation |
| `crates/workflow-engine` | Graph execution, retries, waits and checkpoints |
| `crates/workflow-expressions` | Expression and input binding evaluation |
| `crates/workflow-service` | Workspaces, publication, runs and scheduling |
| `crates/workflow-worker` | HTTP handlers, authentication, Durable Objects and alarms |
| `crates/workflow-mcp` | MCP server, credentials and OAuth |
| `crates/workflow-data` | Data Tables and Context Store |
| `crates/connector-sdk` | Connector manifest and execution contracts |
| `crates/connectors-*` | Built-in connectors (core, HTTP, webhook, data, validation) |

## Documentation

**Using Kasuri**

- [Example workflows](examples/README.md)
- [Connector reference](docs/connectors.md)
- [Webhooks](docs/webhooks.md), [Conditions](docs/conditions.md), [Expressions and routing](docs/expressions.md)
- [Parallel branches](docs/parallel.md), [Item processing](docs/items.md), [Calling workflows](docs/workflow-invocation.md)
- [Parsing](docs/parsing.md), [Validation](docs/validation.md)
- [Data storage](docs/data-storage.md), [Data import and export](docs/data-transfer.md)

**Running Kasuri**

- [Cloudflare deployment](docs/deployment.md), [One-click deployment](docs/cloudflare-one-click.md)

**Integrating**

- [Connecting agents (MCP)](docs/mcp.md)
- [HTTP management API](docs/http-api.md)

**Internals and design**

- [Workflow engine](docs/workflow-engine.md)
- [UI implementation](docs/ui-implementation.md), [Styling](docs/styling.md)
- [Scalable workspace storage and execution](docs/workspace-storage.md)
- [Third-party platform authentication](docs/platform-auth.md) (planned)
- [Agent management requirements](docs/agent-mcp.md), [Agentic connectors](docs/agentic-connectors.md) (planned)
- [Internal platform plan](docs/internal-platform-plan.md)

## Project status

Kasuri is under active development. Current limitations:

- **No user accounts.** A browser cookie identifies who owns which workspaces; clearing it loses access to them. Protect private installations with [Cloudflare Access](https://developers.cloudflare.com/cloudflare-one/policies/access/) or the optional Basic authentication. Third-party sign-in is [planned](docs/platform-auth.md).
- **Workspace capacity.** Each workspace runs as one Durable Object. Finished runs are stored as [history](docs/deployment.md#run-history) under configurable retention; workflows, active runs and audit entries share a 6 MB in-memory budget, which limits how many large runs can be active at once. See [concurrent runs](docs/deployment.md#concurrent-runs) and the [storage redesign](docs/workspace-storage.md).
- **No SaaS connectors yet.** Integrations use HTTP Request. OAuth-based connectors are not implemented.

## Contributing

Contributions are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers setup, the checks CI runs, and how commits and pull requests are named.

## Security

Please report vulnerabilities privately through [GitHub security advisories](https://github.com/haswalt/kasuri/security/advisories/new), not in public issues.

## License

Kasuri is licensed under the [Apache License, Version 2.0](LICENSE).
