# IF conditions

The IF node uses field/operator/value rows. Add conditions and choose **All (AND)** or **Any (OR)**. Matching input follows `true`; other input follows `false`. Input passes through unchanged.

Fields accept dotted names (`customer.email`), JSON Pointers (`/headers/x-event-id`, with standard `~0`/`~1` escaping), or `$` for the whole input. The editor suggests fields from available simulation output or test input. Choose Text, Number, Boolean, Null or JSON explicitly; values are not coerced to the input's type. Invalid/incomplete conditions remain editable as drafts but cannot be published/executed.

Operators include equality/inequality, existence, emptiness, string/array containment, string prefixes/suffixes and numeric comparisons. Existence tests distinguish a missing property from a present JSON null. Empty means missing/null or an empty string/array/object; zero and false are not empty. A missing field or wrong input type does not satisfy a value comparison, including negated comparisons. Numeric operators require numbers; integer comparisons retain 64-bit precision. No regex or executable expression is evaluated.

```json
{"match":"all","conditions":[{"field":"headers.content-type","operator":"starts_with","valueType":"string","value":"application/json"},{"field":"amount","operator":"greater_or_equal","valueType":"number","value":"100"},{"field":"inserted","operator":"equals","valueType":"boolean","value":true}]}
```

`conditions` can also be JSON text. There must be 1–32 rows. `valueType` is optional for already typed JSON literals; when present, Number/Boolean/JSON accept encoded JSON text from the editor. Text stays text; JSON null is explicit. Unary operators (`exists`, `not_exists`, `is_empty`, `is_not_empty`) need no value.

Existing `{ "pointer": "/field", "equals": ... }` IF definitions keep their original strict equality and missing-input error behavior. The editor displays them as one typed row and only converts the saved configuration when edited. Mixing legacy keys with structured conditions is rejected.

For request validation, read selected headers through Request Context, then branch explicitly. For duplicate rejection, execute Context Store/Data Tables `insert_if_absent` before IF and test `inserted = true`. A lookup followed by an insert can race. A successful claim can proceed to Respond 201; an existing claim can Respond 409. These choices belong in the workflow, not the trigger. See [example 14](../examples/workflows/14-structured-conditions.json).
