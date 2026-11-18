import type {
  Json,
  Run,
  RunMode,
  Workflow,
  Workspace,
  WebhookEndpoint
} from '../../../../packages/contracts/src/index'
async function request<T>(
  path: string,
  method = 'GET',
  body?: unknown
): Promise<T> {
  const response = await fetch(`/api/${path}`, {
    method,
    credentials: 'same-origin',
    headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    ...(body === undefined ? {} : { body: JSON.stringify(body) })
  })
  if (response.status === 401)
    throw new Error(
      'Authentication required. Sign in through this installation’s configured authentication.'
    )
  if (!response.headers.get('content-type')?.includes('application/json'))
    throw new Error('Backend unavailable. Start the full app with pnpm dev.')
  const result = await response.json()
  if (!response.ok)
    throw new Error(result.error || `Request failed (${response.status}).`)
  return result as T
}
export const api = {
  mcp: () => request<McpStatus>('mcp'),
  createMcpCredential: (name: string, role: string, expiresDays: number) =>
    request<McpCredential & { token: string }>('mcp/credentials', 'POST', {
      name,
      role,
      expiresDays
    }),
  revokeMcpCredential: (id: string) =>
    request<McpCredential>(`mcp/credentials/${id}`, 'DELETE'),
  resetUnusedOAuthClients: () => request('mcp/oauth/clients/reset', 'POST', {}),
  oauthRequest: (id: string) => request<OAuthRequest>(`mcp/oauth/${id}`),
  oauthConsent: (id: string, scopes: string[], approve: boolean) =>
    request<{ redirect: string }>(
      `mcp/oauth/${id}/${approve ? 'approve' : 'deny'}`,
      'POST',
      { scopes }
    ),
  audit: (after = 0) => request<AuditPage>(`audit?after=${after}&limit=100`),
  webhook: (id: string) => request<WebhookEndpoint>(`workflows/${id}/webhook`),
  rotateWebhook: (id: string) =>
    request<WebhookEndpoint>(`workflows/${id}/webhook/rotate`, 'POST'),
  workspace: () => request<Workspace>('workspace'),
  save: (workflow: Workflow) =>
    request<Workflow>(`workflows/${workflow.id}`, 'PUT', workflow),
  workflowAction: (id: string, action: string, expectedVersion?: number) =>
    request<Workflow>(`workflows/${id}/${action}`, 'POST', { expectedVersion }),
  run: (
    workflowId: string,
    mode: RunMode,
    input: Json,
    expectedVersion?: number
  ) =>
    request<Run>('runs', 'POST', { workflowId, mode, input, expectedVersion }),
  getRun: (id: string) => request<Run>(`runs/${id}`),
  runAction: (id: string, action: string) =>
    request<Run>(`runs/${id}/${action}`, 'POST')
}

export type McpCredential = {
  id: string
  name: string
  scopes: string[]
  createdAt: number
  expiresAt: number
  revoked: boolean
  active?: boolean
  refreshAvailable?: boolean
  clientId?: string | null
  authentication: string
}
export type McpStatus = {
  endpoint: string
  issuer: string
  protocolVersion: string
  unusedClients: number
  scopes: string[]
  roles: Record<string, string[]>
  credentials: McpCredential[]
}
export type OAuthRequest = {
  id: string
  clientName: string
  clientId: string
  redirectUri: string
  scopes: string[]
  resource: string
}
export type AuditEvent = {
  id: number
  actor: string
  action: string
  target: string
  outcome: string
  at: string
  correlationId: string
}
export type AuditPage = {
  items: AuditEvent[]
  nextCursor: number | null
  oldestRetainedId: number | null
  totalRecorded: number
  retentionLimit: number
}
