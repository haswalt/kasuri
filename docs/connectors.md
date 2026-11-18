# Implemented connectors

The runtime advertises 12 node types: Manual Trigger, Webhook (supplied payload only), Schedule, Map / Transform, IF / Condition, Merge, Wait / Delay, JSON Utilities, Error Handler, Filter, Sort and HTTP Request. A catalogue entry marked Planned is not executable.

## HTTP Request

HTTP Request supports GET, HEAD, POST, PUT, PATCH and DELETE against approved public HTTPS hosts on port 443. Requests run asynchronously from persisted checkpoints. The node emits:

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

Only `accept`, `content-type` and `user-agent` custom headers are currently accepted. JSON bodies set Content-Type automatically. URLs cannot contain user credentials, fragments, IP literals or private/local hostnames. Redirects are rejected. Credential headers, OAuth, secret connections and pagination are pending; do not put secrets in workflow JSON or URL query parameters.

### Approve outbound hosts

The operator must set `HTTP_ALLOWED_HOSTS` to a comma-separated list of exact hostnames. An empty list denies all real requests with `egress_denied`. Wildcards and automatic subdomain matching are not supported. Approve only public services you trust.

For Cloudflare, add a deployment variable (or a local ignored `.dev.vars` entry):

```json
{"vars":{"HTTP_ALLOWED_HOSTS":"api.example.com,other.example.com"}}
```

Keep deployment-specific configuration outside the public repository when appropriate. For Compose, set `HTTP_ALLOWED_HOSTS` in your ignored `.env`; for a standalone workerd process or Kubernetes container, set the environment variable. Self-hosted workerd's default outbound network policy allows publicly routable addresses only; preserve that restriction.

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
