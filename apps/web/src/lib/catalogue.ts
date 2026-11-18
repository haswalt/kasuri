export const catalogue = [
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
    "name": "Loop",
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
  "HTTP Request": [{ key: "url", label: "Request URL", placeholder: "https://api.example.com/data" }, { key: "method", label: "Method", placeholder: "GET" }],
  Schedule: [{ key: "cron", label: "Cron expression (UTC)", placeholder: "0 9 * * *" }],
  Webhook: [{ key: "path", label: "Webhook path", placeholder: "/incoming-event" }],
  "Map / Transform": [{ key: "mapping", label: "Mapping definition", placeholder: '{ "name": "{{ input.name }}" }' }],
  Slack: [{ key: "channel", label: "Channel", placeholder: "#notifications" }, { key: "message", label: "Message", placeholder: "A new event arrived" }],
}
