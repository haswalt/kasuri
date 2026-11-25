# Cloudflare Email workflows

Use **Email Trigger → Clairic → IF → Send Email** to route booking enquiries. The [importable example](../examples/workflows/34-email-booking-enquiry.json) simulates with `{}` and a synthetic message. Simulation returns `status: "preview"` and makes no send or Workers AI call.

## Receive mail

1. Create and publish a workflow with an **Email Trigger**. Find its workflow ID in its editor URL and the workspace ID in the workspace's MCP endpoint (the segment after `/mcp/`).
2. Set the deployment's `EMAIL_ROUTES` Worker secret to a JSON object. Each lowercase envelope recipient maps to the workspace ID and workflow ID. Use values from your installation.

   ```json
   {
     "booking@example.com": {
       "workspaceId": "your-workspace-durable-object-id",
       "workflowId": "your-published-workflow-id"
     }
   }
   ```

3. Configure [Cloudflare Email Routing](https://developers.cloudflare.com/email-service/configuration/email-routing-addresses/) to send the corresponding addresses to the Kasuri Worker.

The trigger emits `from` and `to` from the SMTP envelope, the decoded `subject` and plain-text `body`, `messageId` (null if absent or invalid), and `automatic` for automated messages and return-path bounces. Email addresses and MIME content come from the message and should be treated as untrusted input. An unknown route or inactive workflow rejects the message. Every accepted delivery starts its own run. Use Context Store to deduplicate messages if your workflow needs it. The MIME message can be up to 1 MiB, and the decoded input must fit the existing 64 KiB workflow value budget. The first version processes the body; file references add attachment handling.

## Send mail

Onboard your sending domain in [Cloudflare Email Service](https://developers.cloudflare.com/email-service/get-started/send-emails/), enable the `EMAIL` send binding in `wrangler.jsonc`, and set the Worker variable `EMAIL_FROM` to an address on that domain. Send Email can run only in workspaces listed in `EMAIL_ROUTES`. Cloudflare's [send binding restrictions](https://developers.cloudflare.com/email-service/configuration/send-bindings/) can further restrict destinations.

Set `to`, `subject` and `text` in the node or bind those input fields to earlier results. A nonempty literal setting takes priority. For a threaded reply, supply the original `messageId` as `inReplyTo`. The node uses the Workers binding to send from durable runs, including after a wait. Threaded replies set `Auto-Submitted: auto-replied`; route messages marked `automatic` away from automatic replies to prevent loops.

A successful live call returns `status: "accepted"` and Cloudflare's message ID. This means Cloudflare accepted the send, and final delivery is tracked in Cloudflare. A failure routes to the node's error port. Automatic retries are disabled because a timeout can occur after Cloudflare has accepted a send.
