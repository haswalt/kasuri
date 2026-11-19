export type Json = null | boolean | number | string | Json[] | { [key: string]: Json }
export type WorkflowNode = { id: string; type: string; label: string; position?: { x: number; y: number }; version?: number; timeoutMs?: number; retry?: { maxAttempts: number; delayMs: number; maxDelayMs: number }; config: Record<string, Json> }
export type Workflow = { schemaVersion: 1; id: string; name: string; description: string; status: 'draft' | 'active' | 'paused'; lifecycle?: { publishedRevision: number | null; enabled: boolean; needsReview: boolean }; hasUnpublishedChanges?: boolean; archivedAt?: string | null; revision: number; updatedAt: string; version?: number; nodes: WorkflowNode[]; edges: { id: string; source: string; target: string; sourcePort?: string }[]; nextScheduledAt?: number | null }
export type RunStatus = 'running' | 'paused' | 'succeeded' | 'failed' | 'cancelled'
export type RunMode = 'run' | 'simulation' | 'schedule' | 'webhook'
export type RunNode = { id: string; label: string; type: string; status: string; attempts: number; output?: Json; error?: string; untilMs?: number }
export type Run = { id: string; workflowId: string; name: string; createdAt: string; revision: number; mode: RunMode; status: RunStatus; nodes: RunNode[]; input: Json; context?: Json; error?: string; wakeAt?: number | null }
export type NodeManifest = { kind: string; version: number; configSchema: Record<string, unknown>; outputPorts: string[]; hasExternalSideEffects: boolean }
export type Workspace = { workflows: Workflow[]; runs: Run[]; connectors: NodeManifest[] }

export type WebhookEndpoint = { configured: boolean; endpointId?: string | null; active: boolean; authentication: string; path?: string; token?: string }

export type DataKind = 'table' | 'context'
export type DataField = { name: string; kind: 'string' | 'number' | 'boolean' | 'object' | 'array' | 'any'; required?: boolean }
export type DataDefinition = { id: string; name: string; kind: DataKind; fields: DataField[] }
export type DataTable = { definition: DataDefinition; version: number; dataRevision: number }
export type DataTableSummary = { id: string; name: string; kind: DataKind; fieldCount: number; version: number; dataRevision: number }
export type DataRow = { key: string; value: Json; version: number; expiresAt: number | null }
export type DataFilter = { pointer: string; operator: 'eq' | 'ne' | 'gt' | 'lt' | 'exists'; value?: Json }
export type DataQuery = { cursor?: string; limit?: number; filters?: DataFilter[] }
export type DataCommand =
  | { operation: 'create_table'; definition: DataDefinition }
  | { operation: 'rename_table'; tableId: string; name: string; expectedVersion: number }
  | { operation: 'get_table'; tableId: string }
  | { operation: 'get'; tableId: string; key: string }
  | { operation: 'insert_if_absent'; tableId: string; key: string; value: Json; ttlMs?: number }
  | { operation: 'put'; tableId: string; key: string; value: Json; expectedVersion?: number; ttlMs?: number }
  | { operation: 'patch'; tableId: string; key: string; patch: Record<string, Json>; removeFields?: string[]; expectedVersion?: number }
  | { operation: 'delete'; tableId: string; key: string; expectedVersion?: number }
  | { operation: 'query'; tableId: string; query?: DataQuery }
export type DataResult = { status?: string; table?: DataTable; row?: DataRow | null; rows?: DataRow[]; nextCursor?: string | null; scanned?: number; inserted?: boolean; deleted?: boolean; results?: DataResult[] }
export type DataResponse = { result: DataResult; replayed: boolean }
