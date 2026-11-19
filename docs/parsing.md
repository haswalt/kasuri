# Request body parsing

Use **Request Context → raw_body → Parse Body → IF valid** to decode a request in the workflow. The webhook trigger needs no parser settings. Existing valid JSON deliveries still expose their decoded value as trigger input and Request Context `body`; other UTF-8 bodies, including malformed JSON, now enter as text instead of being rejected before the workflow. Non-UTF-8 bodies remain unsupported.

Choose JSON, XML, Text or Form from Parse Body's format selector. An optional JSON Pointer selects a string from an input object (empty means whole input). Output is `{ "valid": true, "value": ..., "errors": [] }` or `{ "valid": false, "value": null, "errors": [{ "code": "invalid_json" }] }`. Parsing errors are normal successful node results: IF decides whether to respond 400, continue, or record a rejection. Invalid node configuration prevents publication/execution.

- JSON parses text and accepts already-decoded JSON values. To parse an encoded JSON string scalar faithfully, use `raw_body`; `body` is already decoded.
- Text preserves whitespace and characters.
- Form decodes UTF-8 `application/x-www-form-urlencoded`, including `+` and percent escapes. Every field is an array of strings, preserving repeated names. Malformed escapes/UTF-8 fail explicitly. Bracket names remain literal; no implicit nested-object convention is applied. Multipart uploads are separate future work.
- XML returns an explicit tree: `{name, namespace, attributes: [{name, namespace, value}], children: [elementOrText]}`. Namespaces, repeated elements and mixed text/element order are preserved; comments and processing instructions are omitted. Numeric and boolean text stays text. DTDs/entities cannot retrieve resources; DTDs are rejected. This is XML parsing, not XSD validation.

Parsing is offline in native Rust and Wasm, with a 32 KiB serialized-input budget, 64 KiB output budget, at most 256 form fields, 1,024 XML nodes and 32 nested XML levels. Error codes never include raw parser messages or invalid body contents. Run input/output still contains workflow data under normal run permissions.

Raw body capture is enabled only when the published workflow contains Request Context `raw_body`, and stays pinned across waits/restarts. Metadata output excludes the raw body. Old runs without captured raw text use their original input string, or a JSON serialization of their original decoded input; original whitespace cannot be recovered from old checkpoints. Simulation uses the test input in the same way.

[Example 17](../examples/workflows/17-body-parsing.json) runs all four formats and validates parsed JSON without input or installation configuration. For a real endpoint, connect Webhook → Request Context (`raw_body`) → Parse Body → IF (`valid = true`) → Respond to Webhook. Place Validate Schema on the valid branch using pointer `/value`; choose responses after validation.

XML uses [roxmltree's bounded parsing options](https://docs.rs/roxmltree/0.21.1/roxmltree/struct.ParsingOptions.html), with DTD disabled, no entity resolver and an explicit node limit.
