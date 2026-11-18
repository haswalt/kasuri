# Kasuri

A Cloudflare-native workflow automation platform, starting with a React + TypeScript frontend prototype.

## Develop

Use Node.js 24+ and pnpm (the package manager version is pinned in `package.json`).

```sh
pnpm install
pnpm dev
```

Vite prints the local preview URL. Run `pnpm build` for a production build and `pnpm test` for graph validation/import tests.

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

Introduce a shared workflow-schema package and validated API, then build the execution foundation (manual/webhook/schedule, HTTP, mapping and conditions), followed by durable execution and the first D1/Slack demo in the spec's delivery order. Cloudflare runtime limits and connector operation contracts must be verified before implementing the backend.

Existing Rust workspace scaffolding is retained; it is not required to run the frontend.
