# Cloudflare demo host

## Live demo

The demo is deployed at [your-worker.your-subdomain.workers.dev](https://your-worker.your-subdomain.workers.dev). Each browser receives its own isolated session and the three starter workflows. Run or simulate the core data demo, try both condition branches, or publish the scheduled heartbeat and inspect its next occurrence.

The hosted smoke check verifies core-node output, both condition branches, error reporting, a real persisted wait and an actual cron-triggered run. It pauses its test schedule afterward:

```sh
node scripts/smoke-demo.mjs https://your-worker.your-subdomain.workers.dev
```

This uses sample data in a new isolated session and can take about one minute while waiting for cron.

## Components

One Worker serves static Vite assets and same-origin `/api/*` requests. The actual Rust engine is compiled to Wasm; TypeScript does not reimplement its graph scheduler. A SQLite-backed Durable Object per browser workspace owns workflow documents, run checkpoints and cron registration. There is no shared global workspace.

The implemented core adapters are pure. A bounded batch of core execution transitions is persisted synchronously before a response or the next alarm. This is safe for current nodes because they perform no external side effects. HTTP/SaaS adapters must use a persisted dispatch/outcome journal and connector-level idempotency before being enabled; the current batch loop must not simply be extended to issue external requests.

Alarms resume real waits even if the browser is closed. Simulations invoke the same Rust code with a virtual clock. Step outputs and failures appear in the run inspector; the run list uses bounded summaries rather than returning all checkpoint payloads.

## Commands

- `pnpm dev`: build and run the whole app locally on port 8787.
- `pnpm dev:web`: optional Vite HMR frontend with API proxy to the local Worker.
- `pnpm build`: release Rust/Wasm bridge and frontend production assets.
- `pnpm deploy:check`: full build, API type check, Wrangler dry run.
- `pnpm deploy`: full build and deploy to the logged-in Cloudflare account.
- `pnpm types`: regenerate binding declarations from `wrangler.jsonc`.

Install Rust's `wasm32-unknown-unknown` target and `wasm-bindgen-cli` 0.2.130 before building. The CLI version is intentionally pinned to the Rust library ABI. Deploy from the repository root. Build outputs under `apps/api/generated`, `apps/web/dist` and local `.wrangler` state are ignored; source, lockfiles, migrations and configuration are tracked.

The default Worker name is `kasuri-demo`. Use a distinct name/account configuration for a separate deployment and keep the Durable Object class name/migration history stable for existing data. This first demo uses one deployment environment; add isolated staging/production resources before production use.

## API

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/api/health` | Rust/Wasm host health |
| GET | `/api/workspace` | Start/restore session; get workflows, run summaries, manifests and expiry |
| PUT | `/api/workflows/:id` | Save a draft and unregister any active schedule |
| POST | `/api/workflows/:id/publish` | Validate graph/support, publish a new revision, register schedule |
| POST | `/api/workflows/:id/pause` | Pause workflow and unregister schedule |
| POST | `/api/workflows/:id/resume` | Revalidate/reactivate workflow and register its next future schedule |
| POST | `/api/runs` | Start `{ workflowId, mode: "run" or "simulation", input }` |
| GET | `/api/runs/:id` | Full run snapshot and node output/status |
| POST | `/api/runs/:id/pause` | Pause dispatch; stop scheduling wake-ups |
| POST | `/api/runs/:id/resume` | Resume and re-arm timers |
| POST | `/api/runs/:id/cancel` | Terminal cancellation |

API bodies use JSON and are bounded to 128 KiB; demo run input is limited to 16 KiB. Session cookies are HttpOnly, SameSite=Strict and Secure on HTTPS. Mutation requests with a foreign Origin or cross-site Fetch Metadata are rejected. There is no cross-origin API access and no token in frontend bundles. Session cookies are bearer capabilities; this is not user authentication or production authorization.

## Cron behavior

The Schedule node has `config.cron` (five-field minute/hour/day/month/weekday) and `config.timezone` (IANA name, default UTC). Publishing computes the next occurrence and persists the published snapshot and due timestamp. A single workspace alarm chooses the earliest run wake-up, cron timestamp or session expiry.

For each due schedule, its run ID derives from workflow ID, revision and scheduled timestamp. The run record and next scheduled timestamp are written in one SQLite transaction. Alarm replay does not duplicate the same occurrence. If the Worker was unavailable over several ticks, the scheduler coalesces them into one due run and computes the next future occurrence; it does not backfill every missed event.

Scheduled input contains `trigger: "schedule"`, `scheduledAt`, `firedAt`, `cron` and `timezone`. At session concurrency capacity, the occurrence is recorded as a failed run with `concurrency_limit`, rather than dispatched. Runs already started continue if the workflow is paused; pausing unregisters future trigger events only.

Editing in the UI is a local draft until saved. Saving a draft unregisters the existing schedule, and publishing re-registers it with a new revision. Session expiry stops all schedules and clears workflow/run data. A cron occurrence beyond the session's 24-hour expiry is not registered.

## Tests and remaining work

Worker integration tests execute in workerd with real SQLite Durable Objects and the compiled Rust module. They cover session isolation, actual transformations, virtual simulation, persisted waits/alarms, failures, unsupported adapters, cancellation, publication, cron timezones, deduplication and unregistration. Native Rust tests continue to cover graph validation, retries, timeouts, branch skipping, controls and recovery.

Before production: authenticated workspace membership, persistent user identity, credential vault and redaction, account-level budgets/rate limits, deployment environments, API optimistic concurrency, external dispatch journaling, metrics/retention/backup, and the managed-Workflows versus scheduler architecture spike. No workflow or credential data is copied from local browser storage into the hosted demo automatically.
