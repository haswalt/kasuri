# Data import, export and atomic counters

The Data tables page supports JSON/CSV imports, exports, structured filters and integer counter updates. Operations stay in Rust on Cloudflare. No deployment-specific settings or credentials are embedded in exported files.

## Import

Choose a data set, select **Import records**, paste records or choose a UTF-8 file, then **Preview import**. The preview checks the complete file against the table's schema and current data revision without writing records or audit events. **Insert new records only** rejects an import if any live key already exists. **Insert or replace records** replaces matching records only after a fresh preview. Commit rechecks the revision and writes all records plus its receipt in one transaction. A stale revision, duplicate key, invalid field or other error leaves every record unchanged. A retry with the same idempotency key returns the original result.

Each import supports up to 100 records, a 128 KiB file and 64 KiB of decoded row JSON. Empty imports are safe no-ops. Larger transfers use bounded import batches with a current revision per batch; there is no promise that multiple batches commit atomically. Existing 32 KiB per-value limits apply.

JSON accepts a row array or this versioned document:

```json
{"schemaVersion":1,"rows":[{"key":"sample-a","value":{"count":1,"active":true},"expiresAt":null}]}
```

CSV uses exactly `key,value,expiresAt` headers. Both key and value cells contain JSON literals: the key must be a JSON string, and value can be any JSON supported by the target data set. CSV quoting then wraps those literals. This preserves string/number/null distinctions, arrays, nested objects, commas, quotes and Unicode. JSON-encoded keys also avoid turning arbitrary record keys into spreadsheet formulas. This is a lossless Kasuri interchange format, not automatic column/type inference from arbitrary spreadsheets.

`expiresAt` is an absolute Unix timestamp in milliseconds, or empty/null for no expiry. Importing an already-expired record skips it; it never restores old data with a fresh TTL. Imported records receive new installation-local versions. The selected target table controls routing; an optional document `tableId` is informational and does not select another workspace or create a table.

Try the public [JSON sample](../examples/datasets/records.json) or equivalent [CSV sample](../examples/datasets/records.csv) in a flexible object table or context namespace.

## Export

**Export JSON** and **Export CSV** include all live records, regardless of dashboard view filters. Exports retain keys, values and absolute expiry timestamps, excluding installation-local row versions. The browser concatenates native JSON fragments without parsing and reserializing numbers, preserving integers beyond JavaScript's safe-integer range.

The API reads bounded pages in key order. Each page is transactional; a data revision check detects mutations between pages. The browser checks the revision again before downloading. TTL expiration is evaluated at each page's read time, so a multi-page export is not a frozen point-in-time snapshot. Quiesce writers and avoid expiring records when you need a stable full transfer. Browser exports stop at 10 MiB or 1,000 pages; larger exports can consume the same paged API directly. An export larger than the per-import bounds must be split into import batches.

## Integer counters

The row's **Increment** action offers known number fields and an explicit custom-field option. Choose a whole-number amount; the backend adds it atomically, preserving unrelated fields and the existing expiry. The dashboard uses the row version it displayed to reject intervening edits. Workflow/API counters can omit that precondition to support independent concurrent increments without a lookup/write race.

The native operation supports signed 64-bit integers with checked overflow. A missing row/field starts at `initial` (default zero); existing non-object rows, non-integer fields and overflow fail without writes. Creation still has to satisfy the table schema. Receipt replay does not increment twice. The dashboard limits entered amounts to JavaScript-safe integers; native API/workflow values retain signed 64-bit precision.

```json
{"operation":"increment","tableId":"counters","key":"requests","field":"count","amount":1,"initial":0}
```

```json
{"operation":"import_rows","tableId":"records","mode":"upsert","expectedDataRevision":0,"rows":[{"key":"sample-a","value":{"active":true}}]}
```

Both commands work through Data Tables/Context Store, owner APIs and MCP `data_apply` under existing explicit dataset scopes. Upsert imports require `expectedDataRevision`; insert-only commands may omit it, while dashboard imports always carry it. MCP mutation results return compact counts/version metadata, and read redaction remains unchanged. Formatted file transfer endpoints belong to the authenticated dashboard API; agents can manage typed rows through MCP without gaining an unredacted export tool.

[Workflow example 21](../examples/workflows/21-bulk-import-counter.json) imports synthetic records using the current data revision and increments a counter. It runs or simulates from `{}` without configuration. Simulations leave live data unchanged.

## Owner API

- `GET /api/tables/{id}/export?format=json|csv` returns `content`, `count`, `nextCursor`, `dataRevision` and `tableVersion`. Follow the cursor and carry `dataRevision` on every subsequent page; a concurrent mutation returns 409. JSON page content is a row array. CSV pages each include a header.
- `POST /api/tables/{id}/import` accepts `{format,content,mode,expectedDataRevision}` for preview. Add `commit:true` and an 8–128 character `idempotencyKey` to commit.
- For raw UTF-8 file bodies, place `format`, `mode`, `expectedDataRevision`, `commit` and `idempotencyKey` in query parameters instead. This avoids expanding CSV quotes inside a JSON wrapper. The dashboard uses this form.

These routes use normal workspace session/authentication and isolation. They are separate from webhook ingress and do not set cookies or enable public access themselves. Reads/previews create no management audit noise; committed changes record table/operation metadata, never imported values. Native CSV behavior uses the pinned [Rust csv crate](https://docs.rs/csv/1.4.0/csv/).

Schema alteration/migrations, secondary query indexes and full spreadsheet column mapping remain separate work.
