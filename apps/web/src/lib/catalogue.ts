export const catalogue = [
  {"name":"Expressions","category":"Core platform"},
  {"name":"Aggregate","category":"Core platform"},
  {"name":"Split / Parallel","category":"Core platform"},
  {"name":"Join","category":"Core platform"},
  { "name": "Execute Workflow Trigger", "category": "Core platform" },
  { "name": "Parse Body", "category": "Core platform" },
  { "name": "Validate Schema", "category": "Core platform" },
  { "name": "Respond to Webhook", "category": "Core platform" },
  { "name": "Request Context", "category": "Core platform" },
  { "name": "Data Tables", "category": "Core platform" },
  { "name": "Context Store", "category": "Core platform" },
  { "name": "Classify with Clairic", "category": "AI and LLM" },
  { "name": "Cloudflare AI Gateway", "category": "AI and LLM" },
  { "name": "Cloudflare MCP Portals", "category": "AI and LLM" },
  { "name": "Sort", "category": "Core platform" },
  {
    "name": "HTTP Request",
    "category": "Core platform"
  },
  {
    "name": "Webhook",
    "category": "Core platform"
  },
  {
    "name": "Schedule",
    "category": "Core platform"
  },
  {
    "name": "Manual Trigger",
    "category": "Core platform"
  },
  {
    "name": "Execute Workflow",
    "category": "Core platform"
  },
  {
    "name": "Code",
    "category": "Core platform"
  },
  {
    "name": "OpenAI",
    "category": "AI and LLM"
  },
  {
    "name": "Anthropic Claude",
    "category": "AI and LLM"
  },
  {
    "name": "Cloudflare Workers AI",
    "category": "AI and LLM"
  },
  {
    "name": "MCP Client",
    "category": "AI and LLM"
  },
  {
    "name": "AI Agent",
    "category": "AI and LLM"
  },
  {
    "name": "Embeddings",
    "category": "AI and LLM"
  },
  {
    "name": "Cloudflare Vectorize",
    "category": "AI and LLM"
  },
  {
    "name": "AI Memory",
    "category": "AI and LLM"
  },
  {
    "name": "RAG / Document Retrieval",
    "category": "AI and LLM"
  },
  {
    "name": "Slack",
    "category": "Communication and messaging"
  },
  {
    "name": "Discord",
    "category": "Communication and messaging"
  },
  {
    "name": "Google Sheets",
    "category": "Productivity and collaboration"
  },
  {
    "name": "Google Drive",
    "category": "Productivity and collaboration"
  },
  {
    "name": "Google Calendar",
    "category": "Productivity and collaboration"
  },
  {
    "name": "GitHub",
    "category": "Productivity and collaboration"
  },
  {
    "name": "Cloudflare D1",
    "category": "Databases, storage and developer infrastructure"
  },
  {
    "name": "Cloudflare R2",
    "category": "Databases, storage and developer infrastructure"
  },
  {
    "name": "Cloudflare KV",
    "category": "Databases, storage and developer infrastructure"
  },
  {
    "name": "Cloudflare Queues",
    "category": "Databases, storage and developer infrastructure"
  },
  {
    "name": "Cloudflare Durable Objects",
    "category": "Databases, storage and developer infrastructure"
  },
  {
    "name": "Square",
    "category": "Business, CRM, ecommerce and marketing"
  },
  {
    "name": "IF / Condition",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Switch / Router",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Merge",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Filter",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Map / Transform",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Split / Batch",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Each / Loop",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Wait / Delay",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Error Handler",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "JSON Utilities",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "CSV Utilities",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "XML Utilities",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "File Utilities",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Cryptography",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Date / Time",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Regular Expressions",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "JWT Utilities",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Image Processing",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Sentry",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Grafana",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "PagerDuty",
    "category": "Utilities, data processing, security and monitoring"
  },
  {
    "name": "Native Cloudflare Email",
    "category": "Additional requested connectors"
  },
  {
    "name": "Square Teams",
    "category": "Additional requested connectors"
  }
]

export const fields: Record<string, { key: string; label: string; placeholder: string }[]> = {
  "Data Tables": [{ key: "command", label: "Data command (JSON)", placeholder: '{ "operation": "get", "tableId": "records", "key": "example" }' }],
  "Split / Batch": [{key:"arrayPointer",label:"Input array",placeholder:""},{key:"batchSize",label:"Items per batch",placeholder:"10"}],
  "Aggregate": [{key:"arrayPointer",label:"Input array",placeholder:""},{key:"operation",label:"Operation",placeholder:"collect"},{key:"pointer",label:"Item value",placeholder:""},{key:"missing",label:"Missing values",placeholder:"fail"}],
  "Each / Loop": [{key:"workflowId",label:"Workflow",placeholder:""},{key:"revision",label:"Revision",placeholder:""},{key:"arrayPointer",label:"Items",placeholder:""},{key:"concurrency",label:"Item concurrency",placeholder:"1"},{key:"failurePolicy",label:"Failures",placeholder:"fail"}],
  "Execute Workflow": [{key:"workflowId",label:"Workflow",placeholder:""},{key:"revision",label:"Published revision",placeholder:""},{key:"mode",label:"Completion",placeholder:"await"},{key:"inputPointer",label:"Input JSON Pointer",placeholder:""}],
  "Execute Workflow Trigger": [{key:"outputNodeId",label:"Output node",placeholder:""},{key:"inputSchema",label:"Input schema (JSON)",placeholder:"{}"},{key:"outputSchema",label:"Output schema (JSON)",placeholder:"{}"}],
  "Split / Parallel": [{key:"concurrency",label:"Maximum simultaneous tasks",placeholder:"4"}],
  "Join": [{key:"policy",label:"Branch completion policy",placeholder:"all"}],
  "Parse Body": [{ key: "format", label: "Format", placeholder: "json" },{ key: "pointer", label: "Input JSON Pointer (empty = whole input)", placeholder: "/body" }],
  "Validate Schema": [{ key: "schema", label: "Validation schema (JSON)", placeholder: '{ "type": "object", "required": ["eventId"], "properties": { "eventId": { "type": "string", "minLength": 1 } } }' }, { key: "pointer", label: "Input JSON Pointer (empty = whole input)", placeholder: "/body" }],
  "Respond to Webhook": [{ key: "status", label: "HTTP status", placeholder: "200" }, { key: "responseType", label: "Response body type", placeholder: "json" }, { key: "headers", label: "Response headers (JSON)", placeholder: '{ "X-Result": "accepted" }' }, { key: "body", label: "Response body (JSON or text)", placeholder: '{ "accepted": true }' }],
  "Request Context": [{ key: "part", label: "Read immutable trigger metadata, body, headers or query", placeholder: "metadata" }],
  "Context Store": [{ key: "command", label: "Context command (JSON)", placeholder: '{ "operation": "get", "tableId": "cache", "key": "example" }' }],
  "HTTP Request": [{ key: "url", label: "Public HTTPS URL", placeholder: "https://api.example.com/data" }, { key: "method", label: "Method: GET, HEAD, POST, PUT, PATCH, DELETE", placeholder: "GET" }, { key: "query", label: "Query parameters (JSON)", placeholder: '{ "q": { "$input": "/query" } }' }, { key: "headers", label: "Custom headers (JSON; use $secret for credentials)", placeholder: '{ "X-Tenant": "team-a", "X-Api-Key": { "$secret": "api_key" } }' }, { key: "authentication", label: "Authentication: none or basic", placeholder: "none" }, { key: "username", label: "Basic auth username", placeholder: "api-user" }, { key: "passwordSecret", label: "Basic auth password secret name", placeholder: "api_password" }, { key: "body", label: "JSON request body (write methods only)", placeholder: '{ "name": { "$input": "/name" } }' }, { key: "responseType", label: "Response: json or text", placeholder: "json" }, { key: "simulationResponse", label: "Simulation response (JSON; no network request)", placeholder: '{ "status": 200, "body": { "items": [] } }' }],
  Schedule: [{ key: "cron", label: "Cron expression", placeholder: "0 9 * * *" }, { key: "timezone", label: "Timezone", placeholder: "Europe/London" }],
  Webhook: [{ key: "contextHeaders", label: "Recorded headers (JSON array; default content-type and x-kasuri-event-id)", placeholder: '["content-type", "x-kasuri-event-id"]' }, { key: "contextQuery", label: "Recorded query fields (JSON array; default none)", placeholder: '["topic"]' }, { key: "authentication", label: "Authentication: none, bearer, query, hmac_sha256 or legacy hmac", placeholder: "none" }, { key: "queryParameter", label: "Secret query parameter (query only)", placeholder: "key" }, { key: "signingSecret", label: "HMAC signing secret name", placeholder: "incoming_webhook_key" }],
  "Map / Transform": [{ key: "mapping", label: "Mapping definition", placeholder: '{ "name": { "$input": "/name" } }' }],
  "IF / Condition": [{ key: "pointer", label: "Input JSON Pointer", placeholder: "/priority" }, { key: "equals", label: "Equals (text)", placeholder: "urgent" }],
  Filter: [{ key: "arrayPointer", label: "Array JSON Pointer (empty = input)", placeholder: "/body/items" }, { key: "pointer", label: "Item JSON Pointer", placeholder: "/active" }, { key: "equals", label: "Equals (JSON value)", placeholder: "true" }],
  Sort: [{ key: "arrayPointer", label: "Array JSON Pointer (empty = input)", placeholder: "/body/items" }, { key: "pointer", label: "Item JSON Pointer (empty = item)", placeholder: "/name" }, { key: "order", label: "Order: asc or desc", placeholder: "asc" }],
  "Wait / Delay": [{ key: "delayMs", label: "Delay in milliseconds", placeholder: "1500" }],
  "JSON Utilities": [{ key: "operation", label: "Operation: parse or stringify", placeholder: "parse" }],
  Slack: [{ key: "channel", label: "Channel", placeholder: "#notifications" }, { key: "message", label: "Message", placeholder: "A new event arrived" }],
}
