# Report workflow errors to Sentry

Wire a node’s **error** output to **Error Handler → Sentry**. You can also connect the error output directly to Sentry. An IF node’s **false** output represents a normal business decision. Connect it to Sentry and set a short **Message** if that decision should be reported.

## Configure a project

1. In Sentry, copy the project DSN from **Settings → Projects → Client Keys (DSN)**.
2. In the Kasuri Worker, set the `SENTRY_PROJECTS` secret to a JSON object. Each entry names a project and lists the exact Kasuri workspace IDs allowed to report into it. The workspace ID is the segment after `/mcp/` in the workspace’s MCP endpoint. Set `HTTP_ALLOWED_HOSTS` to include the ingestion hostname if your installation uses that allowlist.

   ```json
   {
     "workflow-errors": {
       "dsn": "https://your-project-key@your-ingest-host.example/123",
       "workspaces": ["your-kasuri-workspace-id"]
     }
   }
   ```

3. Select the alias in the Sentry node. To group related failures, use a stable message and a dedicated Sentry project. Sentry receives the configured message or a generic failure summary, the safe error code when present, and the workflow and run IDs. Keep the message free of private data. Workflow input and response bodies are excluded.

Each report uses a stable event ID. The connector makes one attempt and routes a failed delivery to its error output. A successful result has `status: "accepted"` and an event ID.

## Try it without a project

Import [example 39](../examples/workflows/39-sentry-error-report.json) and select **Simulate → Play** with the default input. A deliberate parser failure reaches Error Handler and the Sentry node returns `status: "preview"`. Simulation never submits an event.
