# Expressions, routing and merge policies

These nodes run entirely in Rust on Cloudflare. Real runs and simulations use the same bounded evaluator. There is no JavaScript evaluation, implicit network, filesystem, environment or secret-store access.

## Expressions

The editor builds output fields from operation selectors, typed fixed values and field pickers. Choose input, original trigger input, an upstream step, or run information as a source. Known fields are suggested from schemas, mappings and simulation output; custom paths remain available. A result is one JSON object, not implicit item fan-out. Use Each / Loop for item execution.

```json
{
  "fields": {
    "total": {
      "op": "multiply",
      "args": [
        { "op": "input", "path": "/price" },
        { "op": "input", "path": "/quantity" }
      ]
    },
    "message": {
      "op": "concat",
      "args": [
        { "op": "literal", "value": "Hello " },
        { "op": "node", "nodeId": "customer", "path": "/name" }
      ]
    }
  }
}
```

References use JSON Pointers (empty means the whole selected value). `nodeId` is a stable node ID, not its display label; the node must be an ancestor in the graph. Descendants, unrelated parallel siblings and other runs/workspaces are unavailable. A skipped node or missing path is missing, distinct from JSON null. Normally a missing result fails with `missing_input`; `exists` tests presence, including null, while `coalesce` picks the first present, non-null value. All-null/missing coalesce returns null. Literal objects are data, never recursively evaluated.

`trigger` reads original run input; request metadata must be explicitly exposed through Request Context and referenced as an upstream output. `run` exposes `/id` and `/startedAtMs`. The start timestamp is persisted, not a changing wall clock; simulations use their run's controlled start time. Referenced outputs and context are persisted with dispatched tasks. Restart cannot turn references into newer workflow data.

| Operations | Behavior |
| --- | --- |
| `literal`, `input`, `node`, `trigger`, `run` | Typed data and explicitly approved references. |
| `add`, `subtract`, `multiply`, `divide` | Two numeric arguments, no string coercion. Integer add/subtract/multiply preserve signed/unsigned 64-bit representable results; overflow fails. Division/fractional calculations use finite floating point, with division by zero rejected. |
| `concat`, `to_text`, `lower`, `upper`, `trim`, `length` | Explicit text composition; concat requires strings. Use to_text to interpolate numbers or other JSON values explicitly (strings stay unchanged; other values use JSON text). Length counts Unicode scalar values for text or entries for arrays/objects. |
| `equals`, `greater_than`, `less_than` | Numeric comparisons preserve integer precision; ordered text comparison is lexical. Unsupported mixed ordered types fail. |
| `exists`, `coalesce`, `and`, `or`, `not`, `if` | Presence/defaults and typed Boolean logic. `if` evaluates only its selected result; and/or/coalesce short circuit. |
| `date_parse`, `date_add_ms`, `date_iso` | Parse RFC 3339 with explicit offset to epoch milliseconds; add signed integer milliseconds; format as UTC RFC 3339 with milliseconds. No implicit locale/timezone or clock access. |

Bounds: 32 output fields, 256 expression nodes across fields, nesting depth 12, up to 16 arguments for concat/coalesce/and/or, 256-byte paths and 64 KiB intermediate/final values. Existing graph/config/checkpoint limits also apply. Static validation rejects unknown operations/keys, invalid argument counts and non-upstream references; data-dependent type errors fail through the normal error port. Arithmetic never silently truncates overflowing integers. No expression language compatibility with n8n or arbitrary script execution is implied.

## Switch / Router

Configure one to eight named routes, each with the same guided All/Any condition rows as IF. The first matching rule chooses exactly one output and passes input unchanged. Otherwise `fallback` runs. Output names use lowercase letters, digits, underscores or hyphens, must be unique and cannot be `fallback` or `error`. `error` remains the normal failure port.

Canvas handles and connection inspectors use the configured names. Disconnect an output before renaming/removing it; adding/reordering rules does not rename existing ports. Order is explicit and can be changed in the builder. Non-selected paths are skipped; downstream merges wait for all incoming paths to settle.

## Merge

An empty config preserves the existing behavior: one incoming edge passes its value through; multiple incoming edges produce an object keyed by active source node IDs. New policies always consume an object keyed by active connected source IDs, including when only one incoming edge exists.

| Policy | Result |
| --- | --- |
| `select` | Return one named source unchanged. A skipped/unavailable source fails with missing input. |
| `append` | Concatenate selected arrays in explicit `sources` order. `arrayPointer` selects the array within each source (empty means whole source). |
| `by_key` | Full outer combination of object rows, matched using `keyPointer`. Unmatched rows stay in first-seen order; matching rows combine top-level fields. Nested values are treated as whole values, not recursively merged. |

The source selector lists connected input nodes. Add sources in the desired order; incoming edge order and completion timing cannot change the result. Up to 16 unique sources and 1,000 visited collection entries are allowed; output is bounded to 64 KiB. Collection policies default to failing on unavailable branches; `missing: "skip"` explicitly omits them. Empty input arrays yield an empty array.

By-key values must be text, numbers or booleans and use their typed JSON representation as identity (for example string `"1"` differs from numeric `1`). Duplicate matching keys combine in source/item order. Equal overlapping fields are accepted; different overlapping fields fail by default. `conflict: "last"` explicitly makes later fields win. Missing/invalid keys fail. These policies do not enable collect-on-error: use Join with its collect policy for failed branch records.

## Public examples

- [26: calculate, reference an upstream output and route](../examples/workflows/26-expressions-router.json).
- [27: append collections and combine rows by key](../examples/workflows/27-merge-collections.json).
- [28: deterministic UTC date calculation](../examples/workflows/28-date-expressions.json).

All three import and simulate with `{}` and no credentials or configuration. The runtime suite also exercises real runs, a restart between source and expression, fallback routing, skipped-node defaults, invalid reference rejection and MCP simulation.

## Building a response from several earlier nodes

Place Expressions immediately before Respond to Webhook. Each output field may reference a different graph ancestor or the original trigger input. The response node can then send the assembled object using its existing whole-input body reference. This is available now; placing Merge nodes between otherwise sequential steps is unnecessary.

For example, expression fields can use `{"op":"trigger","path":"/orderId"}`, `{"op":"node","nodeId":"calculate","path":"/total"}` and `{"op":"node","nodeId":"shipping","path":"/output/body"}`. For raw request text, parse it with Parse Body first and reference that node's `/value/orderId` instead of assuming the trigger is already an object. Paths depend on those nodes' actual outputs: Execute Workflow wraps an awaited child's result in `output`; inspect runs_get or the child contract rather than assuming an HTTP envelope. Validate Schema followed by IF can check the assembled response and route to an explicit error response before sending it.

Direct expression bindings inside arbitrary connector configuration are not implemented yet. A planned shared engine input-binding layer will reuse this evaluator for HTTP requests, response bodies, conditions and child-workflow inputs. It must preserve typed values, graph dependencies, explicit missing/error handling and simulation/restart behavior. Secret stores and private host/auth context remain unavailable; sensitive business data in an allowed upstream output remains the workflow author's responsibility to select appropriately.
