# Implemented connectors

The runtime advertises 15 node types: Manual Trigger, Webhook, Schedule, Map / Transform, IF / Condition, Merge, Wait / Delay, JSON Utilities, Error Handler, Filter, Sort, HTTP Request, Data Tables, Context Store and Request Context. A catalogue entry marked Planned is not executable.

## Data Tables and Context Store

Use typed `command` JSON for explicit dataset creation, reads, writes, atomic claims, version checks, TTL and queries. Simulations use isolated snapshots; real runs persist their effects transactionally with checkpoints. See [data semantics](data-storage.md) and [importable examples](../examples/README.md).

## Request Context

Read immutable trigger metadata, original body, selected headers or query fields. Webhook capture is opt-in and excludes consumed credentials. Context survives waits/restarts and stays separate from persistent Context Store values. See [capture configuration](webhooks.md#request-context) and [example 12](../examples/workflows/12-request-context.json).

## Webhook

Public or explicitly authenticated JSON delivery, generated/rotatable endpoints, publish/pause activation, bearer/query keys and timestamped HMAC signatures are implemented. Accepted deliveries start independent runs; business-event deduplication belongs in workflow logic. See [webhook setup and API](webhooks.md).

## HTTP Request

HTTP Request supports GET, HEAD, POST, PUT, PATCH and DELETE against public HTTPS destinations by default. Requests run asynchronously from persisted checkpoints. The node emits:

```json
{"status":200,"contentType":"application/json","body":{"items":[]}}
```

Configure `url`, optional `method` (GET by default), `query` (JSON object), `headers` (JSON object), `body` (JSON value for write methods), and `responseType` (`json` or `text`). The editor accepts JSON text for object fields. Query and body values can reference incoming data with an object containing exactly one `$input` JSON Pointer:

```json
{
  "query": {"search": {"$input": "/search"}},
  "body": {"customer": {"$input": "/customer"}}
}
```

Pointers refer to the node's incoming payload. A single parent passes its output directly; multiple active parents produce an object keyed by parent node ID. Put a Map / Transform node before HTTP to prepare fields and after HTTP to extract `/body` when needed.

Custom headers accept ordinary string values, for example `{"X-Tenant":"team-a"}`. Sensitive headers such as Authorization, Cookie and API keys use named secret references instead of literal credentials:

```json
{"Authorization":{"$secret":"service_bearer"},"X-Api-Key":{"$secret":"service_key"}}
```

The referenced secret contains the complete header value (`Bearer ...` when required). Header names are case-insensitive; transport and proxy headers such as Host, Content-Length, Connection and X-Forwarded-For are managed by the runtime. Header names and values are checked for injection. A JSON body gets Content-Type automatically unless a custom Content-Type is supplied. Idempotency-Key remains managed by the workflow engine.

### Basic authentication and named secrets

Set the node's `authentication` to `basic`, enter `username`, and set `passwordSecret` to a secret alias. The editor has these fields. For example:

```json
{"authentication":"basic","username":"api-user","passwordSecret":"service_password"}
```

Configure aliases in the installation's `HTTP_SECRETS` JSON map:

```json
{"service_password":"YOUR_PASSWORD","service_bearer":"Bearer YOUR_TOKEN","service_key":"YOUR_API_KEY"}
```

Named HTTP secrets require the installation’s existing `BASIC_AUTH` protection to be configured, so anonymous workflow authors cannot use shared installation credentials. Without it, secret-backed requests fail with `egress_denied`; unauthenticated public HTTP requests still work.

On Cloudflare, store that JSON with `pnpm exec wrangler secret put HTTP_SECRETS`. Locally, put it in ignored `.dev.vars`. For self-hosting supply `HTTP_SECRETS` from the environment or a Kubernetes Secret; the container also accepts `HTTP_SECRETS_FILE` pointing at a mounted JSON file. Compose reads `HTTP_SECRETS` from the ignored `.env`. Keep these values out of Git.

The workflow contains the username and secret aliases. Password/header secret values are resolved only after dispatch is persisted, are not returned by the workspace API, and are not copied into task checkpoints. Known secret values and Basic authorization encodings are redacted from successful response bodies if echoed by an upstream. Missing aliases fail with `missing_input`; there is no fallback to plaintext passwords. Non-sensitive literal custom headers remain part of workflow JSON. Do not put credentials in URL queries, run input, ordinary literals or fixtures. OAuth and a connection-management vault UI remain future work.

### Optional destination restrictions

`HTTP_ALLOWED_HOSTS` is optional. Unset or empty allows public HTTPS destinations without per-API deployment changes. Setting a nonempty comma-separated list restricts outbound requests to those exact hostnames (case-insensitive). Wildcards and implicit subdomain matching are unsupported.

```json
{"vars":{"HTTP_ALLOWED_HOSTS":"api.example.com,other.example.com"}}
```

URLs cannot contain credentials or fragments. Private, loopback, link-local, metadata, special-use addresses and local hostnames are blocked by default. Before dispatch, the Rust host checks A and AAAA DNS answers through Cloudflare's public DNS-over-HTTPS resolver and rejects the destination if any answer is nonpublic. DNS lookup failures fail closed. The trusted DNS endpoint is independent of the optional application host allowlist.

Redirects are rejected, including redirects to another public host, so credentials are never automatically forwarded to a new destination. The connection layer must also enforce public-only networking: self-hosted workerd explicitly allows only publicly routable destination addresses, protecting against DNS rebinding between lookup and connect. Preserve this network policy when modifying the deployment. Cloudflare runs through its platform's outbound networking; the application does not provision private connectivity.

### Explicit private-host opt-in

An operator can set `HTTP_PRIVATE_HOSTS` to exact destination hostnames or IP addresses that may bypass the public-destination/DNS checks. Leave it empty unless deliberately enabling a trusted private service. If `HTTP_ALLOWED_HOSTS` is also set, the destination must satisfy that restriction too. HTTPS and header/redirect protections still apply.

This setting grants permission; it does not create connectivity. Self-hosted operators must additionally add the specific private IP/CIDR to the `internet` service's `network.allow` list in workerd configuration. Keep `public` and add only the required ranges, rather than enabling all private networks. Cloudflare private connectivity requires separately configured platform networking; a host opt-in alone does not enable it.

### Simulations and failures

Simulate makes no outbound request. Supply `simulationResponse` in the node configuration, for example:

```json
{"status":200,"body":{"items":[{"name":"Ada","active":true}]}}
```

This is an explicit author-supplied fixture, not a seeded workspace or deployment mode. Missing fixtures fail with `missing_input`. Simulations use the same status classification and response shape as real requests.

GET/HEAD can retry network failures, 408, 429 and server errors according to the node's retry policy. Other status codes fail without retrying. Non-success upstream bodies and upstream response headers are not stored. JSON parsing failures use `invalid_response`; excessive bodies use `budget_exceeded`. Connect the `error` output to an Error Handler or recovery branch.

Writes require `maxAttempts: 1`. A stable Idempotency-Key is sent, but support depends on the upstream API. A restart after dispatch leaves the attempt pending until its deadline and then fails it rather than sending it again. A transport failure on a write reports `outcome_unknown`: the remote service may have committed it. Cancel prevents further graph execution and ignores late results; it cannot undo an external write already in progress. Deadlines abort network I/O when the host is still running. There is no exactly-once guarantee.

Request bodies and response bodies are limited to 32 KiB, within the engine's existing value/checkpoint budgets. Non-JSON text output can also reach the encoded value budget due to escaping. Large/binary streaming and artifact storage remain future work.

## Filter

Select an array using `arrayPointer` (empty means the input itself). `pointer` selects a value within each item; `equals` is a JSON value, or JSON text in the editor. Missing keys do not match explicit null. Output is the filtered array.

For an HTTP response containing `/body/items`, use:

```json
{"arrayPointer":"/body/items","pointer":"/active","equals":true}
```

In the editor enter `true` for a boolean or `"active"` including quotes for a string.

## Sort

Select an array with `arrayPointer` and a sort key with `pointer`; both default to the input/item itself. Set `order` to `asc` (default) or `desc`. Sorting is stable, numeric for number keys and lexicographic for strings. Missing keys, mixed key types, booleans and objects fail instead of silently coercing values. Output is the sorted array.

A useful flow is Manual/Schedule → HTTP Request → Filter → Sort → Map / Transform.
