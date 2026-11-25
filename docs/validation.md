# Validate Schema

Validate Schema checks data and returns a normal result. It does not branch, reject a webhook or stop background processing. Follow it with IF on Boolean `valid`, then choose a response or other action on each path.

```json
{"valid":false,"errors":[{"instancePath":"","schemaPath":"/required","keyword":"required","message":"Value does not satisfy required","missingProperty":"eventId","pathTruncated":false}],"errorsTruncated":false,"value":{}}
```

`value` is the selected input, unchanged. `errors` contain instance/schema JSON Pointers, the failed keyword and a safe diagnostic. Diagnostics do not echo failing values. This prevents a private value from leaking through a generic error message; normal run input/value still contains the actual data under the existing run permissions. A missing required property is identified separately. `errorsTruncated` reports partial diagnostics and never changes an invalid result into a valid one.

Configuration has `schema` (JSON object/boolean or JSON text from the editor) and optional `pointer` (JSON Pointer; empty selects the whole input). An absent selected input is a node `missing_input` error. Malformed/unsupported schemas are configuration errors; invalid data is a successful node result with `valid: false`.

```json
{"schema":{"type":"object","required":["eventId","amount"],"additionalProperties":false,"properties":{"eventId":{"type":"string","minLength":1},"amount":{"type":"number","minimum":0}}}}
```

## Supported offline profile

The connector uses the [Rust jsonschema validator](https://docs.rs/jsonschema/0.61.2/jsonschema/) with default network/filesystem features disabled and offline resolution enforced. It applies a bounded JSON Schema 2020-12 profile, rather than claiming every JSON Schema feature is supported:

- Types, including arrays of allowed types and integer/null types.
- `properties`, `required`, `additionalProperties`, `items` and boolean schemas.
- `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`.
- `minLength`/`maxLength`, `minItems`/`maxItems`, `minProperties`/`maxProperties`.
- `enum` and `const`.
- Asserted formats: `date`, `date-time`, `time`, `email`, `hostname`, `ipv4`, `ipv6`, `uri`, `uuid`.
- `title`, `description`, `default` annotations; defaults do not mutate input. An explicit `$schema` must be `https://json-schema.org/draft/2020-12/schema`.

Unknown keywords/formats are rejected, not silently ignored. References (local or external), regex patterns, schema combinators, dynamic schema retrieval and content decoding are not included in this profile. Schemas have the existing 64 KiB configuration budget, at most 512 schema nodes and 16 nesting levels. This bounds traversal and avoids recursive references or user-supplied regex backtracking. Error reporting is limited to 32 entries/8 KiB; paths can be shortened with `pathTruncated: true`. The result, including the unchanged value, must fit the engine's existing 64 KiB node-output budget.

No network, filesystem, secrets or executable code are available to a validation schema. The same Rust implementation runs in native tests and Cloudflare Wasm. Tests include actual format assertions, required fields, numeric ranges/64-bit integer bounds, invalid schemas, unsafe retrieval rejection and bounded diagnostics.

## Workflow patterns and examples

[Example 15](../examples/workflows/15-schema-validation.json) validates a synthetic body, then IF selects its branch. Import and run with default `{}` input.

[Example 16](../examples/workflows/16-request-validation-gate.json) combines explicit nodes:

1. Ensure its context namespace exists.
2. Request Context reads selected headers; IF checks `x-event-type = order.created`.
3. Request Context recovers the original body; Validate Schema checks the body; IF handles `valid`.
4. An atomic context claim uses the validated event ID. IF on `inserted` selects `201` or duplicate `409`.
5. Processing continues after the successful response.

Run now/Simulate preview the missing-header `400` path without opening a webhook or requiring configuration. To try real requests, generate this installation's endpoint and publish. Send JSON with `X-Event-Type: order.created` and body `{"eventId":"order-123","amount":10}`. The first event returns `201`; the same ID returns `409` until its 60-second claim expires. Invalid headers/body return workflow-selected `400`s. This example does not reset real claims. All example values are synthetic and no credentials or deployment URLs are embedded.

Schema validation does not parse XML or raw request text. Webhook ingress currently parses JSON; additional parsers/methods remain separate planned work.
