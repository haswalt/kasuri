import { runStatusTone, workflowStatusTone } from './tokens.ts'
import type { NodeCategory } from './tokens.ts'
import type { Run } from '../../../../packages/contracts/src/index.ts'
export function category(kind: string): NodeCategory {
  if (['Manual Trigger', 'Webhook', 'Email Trigger', 'Schedule', 'Execute Workflow Trigger'].includes(kind)) return 'trigger'
  if (kind === 'Clairic' || /HTTP|AI|OpenAI|Claude|MCP|Embeddings/.test(kind)) return 'http'
  if (/Error|Sentry|PagerDuty/.test(kind)) return 'error'
  if (/Cloudflare|D1|R2|KV|Queues/.test(kind)) return 'cf'
  if (
    /Parse Body|Validate Schema|Respond to Webhook|Data Tables|Context Store|Request Context|Expressions|Switch|Split|Aggregate|Join|Map|IF|Sort|Filter|Merge|Wait|JSON|CSV|XML|Loop|Batch|Code|Execute/.test(
      kind
    )
  )
    return 'logic'
  return 'app'
}
export function nodeIcon(kind: string) {
  return kind === 'Schedule'
    ? 'clock'
    : category(kind) === 'trigger'
      ? 'bolt'
      : /IF|Merge|Filter|Sort/.test(kind)
        ? 'branch'
        : /HTTP/.test(kind)
          ? 'runs'
          : kind === 'Clairic' || /AI|MCP/.test(kind)
            ? 'spark'
            : category(kind) === 'cf'
              ? 'database'
              : category(kind) === 'error'
                ? 'warning'
                : category(kind) === 'app'
                  ? 'hash'
                  : 'command'
}
export function tone(status: string) {
  const aliases: Record<string, string> = {
    success: 'succeeded',
    retry: 'retrying',
    pending: 'queued',
    skipped: 'cancelled'
  }
  const original = status
  status = aliases[status] || status
  const result = (
    runStatusTone as Record<string, { fg: string; bg: string; label: string }>
  )[status] ||
    (
      workflowStatusTone as Record<
        string,
        { fg: string; bg: string; label: string }
      >
    )[status] || {
      fg: 'var(--text-3)',
      bg: 'var(--surface-2)',
      label: status === 'planned' ? 'Planned' : status
    }
  return {
    ...result,
    label:
      original === 'skipped'
        ? 'Skipped'
        : original === 'pending'
          ? 'Pending'
          : result.label
  }
}
export function runState(run: Run) {
  return run.status === 'running'
    ? run.nodes.some((n) => n.status === 'retry')
      ? 'retrying'
      : run.nodes.some((n) => n.status === 'waiting')
        ? 'waiting'
        : 'running'
    : run.status
}
export function download(
  name: string,
  value: unknown,
  type = 'application/json'
) {
  const url = URL.createObjectURL(
    new Blob(
      [
        type === 'application/json'
          ? JSON.stringify(value, null, 2)
          : String(value)
      ],
      { type }
    )
  )
  const a = document.createElement('a')
  a.href = url
  a.download = name
  a.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}
export const date = (value: string | number) =>
  new Date(value).toLocaleString('en-GB', {
    day: 'numeric',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit'
  })

export function csvCell(value: string) {
  const safe = /^\s*[=+\-@]/.test(value) ? "'" + value : value
  return '"' + safe.replaceAll('"', '""') + '"'
}
