# Importable example workflows

Download a JSON file from [workflows](workflows/) (use GitHub's **Download raw file**), open your Kasuri installation, choose **Workflows → Import JSON**, and select it. The files are ordinary workflow exports committed with the project, not preloaded workspace data or an application demo mode.

Every example accepts the default `{}` input. No accounts, API keys, environment variables or installation-specific URLs are required. For example 19, import and publish the reusable workflow 18 first; for examples 24/25, import and publish 23 first; importing the caller automatically binds both calls to that imported child, without ID edits. Use **Simulate → Play** for the Clairic and email examples. The other examples can immediately use **Run now** or **Simulate → Play**. Imported workflows are drafts with fresh IDs, so importing does not activate recurring jobs or webhook endpoints.

| Example | Level | What to inspect |
| --- | --- | --- |
| [01 Hello](workflows/01-hello.json) | Simple | Manual Trigger → Map; output contains `Hello from Kasuri`. |
| [02 JSON round trip](workflows/02-json-roundtrip.json) | Simple | Create an object, stringify it, then parse the same object back. |
| [03 Filter and sort](workflows/03-filter-sort.json) | Intermediate | Filter synthetic records by a boolean and sort the result by rank: Alder, then Cedar. |
| [04 Branch, wait and merge](workflows/04-branch-wait-merge.json) | Intermediate | IF selects the urgent branch, skips the other branch, waits one second and merges. The merged object is keyed by the incoming node ID, `wait`. Simulation advances the clock instantly. |
| [05 Error recovery](workflows/05-error-recovery.json) | Intermediate | Parse receives an object instead of JSON text. Its error port feeds Error Handler and produces `{ "recovered": true, "reason": "invalid_config" }`. Handled errors are recorded as successful error-port completions in the current engine. |
| [06 Read a public API](workflows/06-public-http.json) | Simple, online | HTTP GET reads a public sample post and maps its ID and user ID. Expected final output: `{ "postId": 1, "userId": 1 }`. |
| [07 Public API pipeline](workflows/07-public-api-pipeline.json) | More involved, online | HTTP query → Filter completed todos → Sort by ID → JSON stringify/parse. Inspect intermediate HTTP, array and JSON outputs. |
| [08 Scheduled heartbeat](workflows/08-schedule.json) | Trigger | Try immediately with Run now or Simulate. **Publish** activates an hourly UTC heartbeat. **Pause** stops it. |
| [09 Webhook acknowledgement](workflows/09-webhook.json) | Trigger | Try immediately with Run now or Simulate. Real inbound events additionally require generating this installation's endpoint in the trigger inspector and publishing (public by default); see [webhook instructions](../docs/webhooks.md). |
| [10 Context cache](workflows/10-context-cache.json) | Persistent data | Explicitly creates a context namespace, caches a synthetic value, reads it and waits past TTL. Simulation advances virtual time and leaves live context unchanged. |
| [11 Atomic event claim](workflows/11-table-claim.json) | Persistent data | Explicitly creates a typed table and demonstrates inserted/existing claim outcomes. Resets only its fixed synthetic key for repeatable experimentation; do not reset real deduplication keys. |
| [12 Request Context](workflows/12-request-context.json) | Immutable input | Replaces an intermediate payload, then recovers the original trigger body independently of that transform. |
| [13 Acknowledge then continue](workflows/13-webhook-response.json) | Response timing | Previews a JSON `201`, passes input through the response node and continues processing. Publishing an installation-local webhook sends that response before background work. |
| [14 Structured conditions](workflows/14-structured-conditions.json) | Conditional routing | Checks a header, amount and boolean using All matching and typed values, then follows the matching branch. |
| [15 Schema validation](workflows/15-schema-validation.json) | Validation | Checks required fields, types, numeric ranges and email format, then IF chooses a branch. |
| [16 Request validation and duplicate gate](workflows/16-request-validation-gate.json) | More involved | Preview the missing-header `400` path immediately. Published requests check selected headers/body, atomically claim event IDs, and respond `201`, `400` or `409`, then continue background work. See [live test instructions](../docs/validation.md#workflow-patterns-and-examples). |
| [17 Body parsing](workflows/17-body-parsing.json) | Combined formats | Decode synthetic JSON, XML, form and text, then validate the parsed JSON. No configuration required. |
| [18 Reusable workflow](workflows/18-reusable-workflow.json) | Simple contract | A standalone simulated child with explicit input/output schemas and a durable wait. |
| [19 Invoke workflow](workflows/19-invoke-workflow.json) | Parent/child | Import and publish example 18 first, then await a result and launch background work. No secrets or configuration edits. |
| [20 Parallel context](workflows/20-parallel-context.json) | Parallel branches | Record context alongside processing, then join both results. No input or configuration required. |
| [21 Bulk import and counter](workflows/21-bulk-import-counter.json) | Data operations | Atomically import synthetic records and increment an integer without a read/write race. No configuration required. |
| [22 Batch and aggregate](workflows/22-batch-aggregate.json) | Collection adapters | Split six values into three batches and sum the original values exactly. |
| [23 Item worker](workflows/23-item-worker.json) | Reusable child | Tag one input object; runs independently from {}. Publish before importing 24/25. |
| [24 Each items](workflows/24-each-items.json) | Durable items | Process three items with concurrency two, then aggregate ordered results. |
| [25 Collect item errors](workflows/25-collect-item-errors.json) | Failure handling | Reject an invalid item, preserve successful results and aggregate them explicitly. |
| [32 Clairic booking reply](workflows/32-clairic-booking-reply.json) | Offline AI extraction | Extract typed booking details from a synthetic email and prepare a reply. |
| [33 Clairic booking handoff](workflows/33-clairic-booking-handoff.json) | Offline AI extraction | Route an enquiry with missing details to a person, with the original email and extracted fields. |

| [34 Email booking enquiry](workflows/34-email-booking-enquiry.json) | Offline email routing | Route a synthetic email through Clairic and preview a threaded reply, without sending mail. |

Together these cover every currently implemented node: Manual Trigger, Webhook, Schedule, Map / Transform, IF / Condition, Merge, Wait / Delay, JSON Utilities, Error Handler, HTTP Request, Filter, Sort, Clairic, Email Trigger and Send Email.

## Public APIs and simulation

Examples 06 and 07 use [JSONPlaceholder](https://jsonplaceholder.typicode.com/), a public sample-data API requiring no signup or key. Live execution sends read-only GET requests containing fixed sample parameters, not your run input. No private data, credential references, personal infrastructure or real customer records are included.

**Run now** calls that public service. **Simulate** never calls it: the HTTP nodes include clearly synthetic `simulationResponse` fixtures in their own configuration. The pipeline's small simulation dataset deliberately differs from the live dataset; both demonstrate filtering and ordering. Live requests need internet access and the installation's default public-HTTPS policy. Upstream outages or a host's custom network restrictions can affect live execution; offline simulation remains available.

The scheduled and webhook examples need no configuration for an immediate trial run. Activating their actual external triggers is an explicit action: schedules must be published, and secure webhook credentials must be generated locally. Import files cannot safely ship reusable webhook secrets or automatically enable a timer.

`basic-workflow.json` and `input.json` are the original paired fixtures for the command-line engine walkthrough and its branching tests. Keep using them together for that walkthrough; the importable files in `workflows/` are self-contained and do not depend on that input file. `scenarios.json` contains test expectations, not an importable workflow.

## Keeping examples working

The tests load the published JSON files directly. They verify editor import, backend validation/publication, execution from empty input, expected outputs, checkpointed waits and real schedule/webhook dispatch. A connector-coverage assertion compares the examples with the backend's supported connector manifests: adding a connector without a corresponding example fails CI. Every workflow file must also have a scenario, so untested examples cannot silently accumulate.

When adding or changing a connector:

1. Add or update an importable, self-contained workflow in `workflows/`, using only supported node versions and synthetic data.
2. Add meaningful outputs/status assertions in `scenarios.json`. Include an offline response fixture for HTTP nodes. Keep authentication-dependent features out of this no-configuration pack unless they can be demonstrated without shared secrets.
3. Update this table and the live test assertions if the example uses a public API.
4. Run `pnpm test:web` and `cargo test -p workflow-service --test examples`. Both already run through the project's normal CI test command.
5. For public HTTP examples, also run `pnpm build` followed by `pnpm test:examples:live`. This opt-in check executes the unchanged files through a temporary, isolated Rust/Wasm runtime with actual public HTTPS access. It removes its local test storage afterward. It is separate from deterministic CI because it depends on an external service.

For dashboard import/export, [sample JSON records](datasets/records.json) and equivalent [CSV records](datasets/records.csv) contain public synthetic values. See [transfer format and bounds](../docs/data-transfer.md).

### Expressions and routing

| Example | Demonstrates |
| --- | --- |
| [26-expressions-router](workflows/26-expressions-router.json) | Typed arithmetic/text, prior-output references, named routing and selected-branch Merge. |
| [27-merge-collections](workflows/27-merge-collections.json) | Explicit append order and full outer combination by key. |
| [28-date-expressions](workflows/28-date-expressions.json) | RFC 3339 parsing, millisecond arithmetic, UTC formatting and missing-value checks. |

These examples need only import and simulation with `{}`; they contain synthetic data and no credentials.
