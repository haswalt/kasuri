import { readRoute } from './navigation'
import type {
  Json,
  Run,
  RunMode,
  Workflow,
  Workspace,
  WebhookEndpoint,
  DataCommand, DataResponse, DataTableSummary
} from '../../../../packages/contracts/src/index'
async function request<T>(
  path: string,
  method = 'GET',
  body?: unknown,
  raw=false
): Promise<T> {
  const response = await fetch(`/api/${path}`, {
    method,
    credentials: 'same-origin',
    headers: { 'X-Kasuri-Workspace': readRoute().workspaceKey, ...(body === undefined ? {} : { 'Content-Type': raw?'text/plain;charset=utf-8':'application/json' }) },
    ...(body === undefined ? {} : { body: raw?body as string:JSON.stringify(body) })
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
export type WorkflowRevision = { workflowId: string; revision: number; graph: Workflow; fingerprint: string; publishedAt: string; publishedBy: string; legacyBaseline: boolean }
export type WorkspaceMembership = { id: string; name: string }
export const api = {
  exportData: (id:string,format:"json"|"csv",cursor?:string,revision?:number) => request<{content:string;count:number;nextCursor:string|null;dataRevision:number}>(`tables/${id}/export?`+new URLSearchParams({format,...(cursor?{cursor}:{}),...(revision===undefined?{}:{dataRevision:String(revision)})})),
  importData: (id:string,input:{format:"json"|"csv";content:string;mode:"insert"|"upsert";expectedDataRevision:number;commit?:boolean;idempotencyKey?:string}) => request<{status:string;rows?:number;imported?:number;skippedExpired:number;dataRevision:number}>(`tables/${id}/import?`+new URLSearchParams({format:input.format,mode:input.mode,expectedDataRevision:String(input.expectedDataRevision),commit:String(input.commit??false),...(input.idempotencyKey?{idempotencyKey:input.idempotencyKey}:{})}),"POST",input.content,true),
  dataTables: (cursor?: string) => request<{tables: DataTableSummary[]; nextCursor: string | null}>('tables?limit=100' + (cursor ? '&cursor=' + encodeURIComponent(cursor) : '')),
  data: (command: DataCommand, idempotencyKey?: string) => request<DataResponse>('data', 'POST', {command, ...(idempotencyKey ? {idempotencyKey} : {})}),
  dataBatch: (commands: DataCommand[], idempotencyKey: string) => request<DataResponse>('data', 'POST', {commands, idempotencyKey}),
  trash: () => request<{workflows: Workflow[]}>('workflows/trash'),
  revisions: (id: string) => request<{revisions: WorkflowRevision[]; needsReview: boolean}>(`workflows/${id}/revisions`),
  restoreRevision: (id: string, revision: number, expectedVersion?: number) => request<Workflow>(`workflows/${id}/revisions/${revision}/restore`, 'POST', {expectedVersion}),
  workspaces: () => request<{ workspaces: WorkspaceMembership[] }>('workspaces'),
  createWorkspace: (name: string) => request<WorkspaceMembership>('workspaces', 'POST', { name }),
  renameWorkspace: (id: string, name: string) => request<WorkspaceMembership>(`workspaces/${id}`, 'PATCH', { name }),
  mcpClientConnections: () => request<{connections:McpClientConnection[]}>('mcp-client/connections'),
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
  audit: (before = 0) => request<AuditPage>(`audit?order=desc&before=${before}&limit=100`),
  webhook: (id: string) => request<WebhookEndpoint>(`workflows/${id}/webhook`),
  rotateWebhook: (id: string) =>
    request<WebhookEndpoint>(`workflows/${id}/webhook/rotate`, 'POST'),
  invocable: () => request<{workflows:{id:string;name:string;revision:number}[]}>("workflows/invocable"),
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

export type McpClientConnection = { alias: string; tools: string[] }
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
  summary?: string
  entity?: { type: string; id: string; name: string }
  before?: Json
  after?: Json
  changes?: Array<{field: string; description?: string; nodeId?: string; nodeName?: string; before?: Json; after?: Json; beforeCount?: number; afterCount?: number}>
  reason?: Json
  attemptedAction?: string
  requiredScope?: string
  context?: Json
}
export type AuditPage = {
  items: AuditEvent[]
  nextCursor: number | null
  oldestRetainedId: number | null
  totalRecorded: number
  retentionLimit: number
}
