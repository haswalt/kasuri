# Kasuri

A Cloudflare-native workflow automation platform, starting with a React + TypeScript frontend prototype.

## Develop

Use Node.js 24+ and pnpm (the package manager version is pinned in `package.json`).

```sh
pnpm install
pnpm dev
```

Vite prints the local preview URL. Run `pnpm build` for a frontend production build and `pnpm test` for the frontend and Rust test suites (Rust is required for the latter).

## Current frontend

- Workflow list with search, status filters, creation, cloning and local publish/pause/resume state.
- Sequential workflow editor with a searchable step palette and configuration inspector.
- Versioned JSON graph with stable node IDs, explicit edges, validation and JSON import/export.
- Browser-local persistence for workflows and the latest 100 simulation records.
- Simulated run timelines that capture step names and revision at simulation time.
- Catalogue of all 50 initial selections in section 6.11 of the supplied requirements.
- Connections placeholder that does not collect credentials.

The UI is intentionally simple, with CSS variables and separate page, graph and catalogue styles so future designs can replace it. The app lives in `apps/web`; graph types and operations are in `src/lib/workflows.ts` and the selected catalogue is in `src/lib/catalogue.ts`.

## Prototype boundaries

Publishing only changes local UI state. Simulations preview the configured step order; they do not execute workflow logic, issue requests, start triggers or access external services. Catalogue entries are planned adapters, with a few illustrative configuration fields. No credentials are stored. There is no backend, authentication, Cloudflare provisioning or deployment in this iteration.

The first editor supports connected sequential graphs only. Branches, drag/drop, zoom/pan, undo/redo, schema-generated forms, immutable published revisions, real execution, and secure credentials remain future work. Browser data is specific to the current origin; export workflows for portable copies. Do not enter secrets or sensitive data into this prototype.

## Next implementation slice

Expose the Rust workflow contract through a TypeScript/Wasm boundary and authenticated API, add a durable Cloudflare host and HTTP adapter, then build the first D1/Slack demo in the spec's delivery order. Cloudflare runtime limits and connector operation contracts must be verified during the deployment spike.

## Rust engine

The engine foundation is implemented in `crates/workflow-schema`, `crates/connector-sdk`, `crates/connectors-core` and `crates/workflow-engine`. It supports validated DAGs, declarative core nodes, typed branches and joins, nonblocking waits, retries, timeouts, pause/cancel, bounded execution and serializable recovery checkpoints. The UI is not wired to the Rust engine yet.

```sh
cargo run -p workflow-engine --example run -- examples/basic-workflow.json examples/input.json
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The example executes core logic with a virtual clock and no external services. See [the engine guide](docs/workflow-engine.md) for host persistence requirements, configuration, limits and remaining integrations. Rust is not required to run the frontend alone.
