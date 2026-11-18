// UI data contracts for the Kasuri screens. These describe what the UI renders;
// map them from the existing API types and don't duplicate backend models if they already exist.
// Secrets never cross into these types: values that came from credentials are pre-redacted ("••••••••").

import type {
  RunStatus, StepStatus, WorkflowStatus, ConnectionHealth, NodeCategory, ToolPolicy,
} from '../tokens/tokens';

export type ISODate = string;
export type Id = string;

// ── Workspace & user ────────────────────────────────────────────────
export type Role = 'owner' | 'admin' | 'editor' | 'viewer';

export interface Workspace {
  id: Id; name: string; letter: string; colour: NodeCategory; // tile colour
  role: Role; workflowCount: number; memberCount: number;
  sandbox: boolean;            // external calls recorded, not sent
  runRetentionDays: number;
  createdAt: ISODate;
}

export interface CurrentUser {
  id: Id; name: string; email: string; initials: string; role: Role;
  themePreference: 'light' | 'dark' | 'system';
  timeZone: string;
}

// ── Workflows ───────────────────────────────────────────────────────
export interface WorkflowSummary {
  id: Id; name: string; description: string;
  status: WorkflowStatus;
  liveVersion?: number; draftVersion?: number;
  trigger: { kind: 'webhook' | 'schedule' | 'email' | 'manual' | 'error' | 'queue'; display: string }; // e.g. "POST /hooks/leads", "cron 0 2 * * *"
  lastRun?: { id: Id; status: RunStatus | 'never'; startedAt: ISODate; durationMs?: number; note?: string };
  runs24h: number;
  updatedAt: ISODate;
}

export type WorkflowAction =
  | 'pause' | 'resume' | 'deactivate' | 'activate' | 'publishAndActivate'
  | 'runNow' | 'duplicate' | 'exportJson' | 'delete';

export interface DashboardStats {
  runs24h: number; successRate: number; successRateDelta: number;
  failed24h: number; deadLetter: number;
  awaitingApproval: number; oldestApprovalMs?: number;
  byState: Record<RunStatus, number>;
}

// ── Graph (editor) ──────────────────────────────────────────────────
export interface GraphNode {
  id: Id;
  type: string;                 // e.g. "core.if", "cf.d1.query", "app.slack.postMessage"
  version: number;
  kindLabel: string;            // "IF / CONDITION", shown uppercase on the node
  title: string;                // user-editable name
  category: NodeCategory;
  icon: string;                 // lucide icon name
  position: { x: number; y: number };
  params: Record<string, unknown>;
}

export interface GraphEdge {
  id: Id; source: Id; target: Id;
  sourceHandle?: 'out' | 'true' | 'false' | 'error' | string;
  label?: 'true' | 'false' | 'error' | string;
  kind?: 'default' | 'error';
}

export interface WorkflowGraph {
  workflowId: Id; version: number; draft: boolean;
  nodes: GraphNode[]; edges: GraphEdge[];
  validation: { valid: boolean; issues: { nodeId?: Id; message: string }[] };
}

export interface PaletteSection {
  name: string; total: number;
  items: { type: string; name: string; category: NodeCategory; icon: string }[];
}

// ── Simulation ──────────────────────────────────────────────────────
export interface TestFixture {
  id: Id; name: string; tag: string; tagTone: 'ok' | 'neutral' | 'fail';
  preview: string;              // short mono preview of the payload
  payload: unknown;
  mocks?: Record<string, unknown>; // e.g. { enrich: 503 }
  pinnedFromRunId?: Id;
}

export interface SimEvent {
  nodeId: Id;
  status: Extract<StepStatus, 'success' | 'retrying' | 'failed'>;
  via?: Id;                     // edge that led here
  message: string;
  tMs: number;
  skip?: Id[];                  // node/edge ids that become skipped
  input?: unknown; output?: unknown; // redacted
}

export interface SimulationResult {
  simId: Id; fixtureId: Id; events: SimEvent[];
  recordedSideEffects: { nodeId: Id; summary: string }[]; // "D1 insert", "Slack post"
  outcome: 'succeeded' | 'failed_handled' | 'failed';
}

// ── Runs ────────────────────────────────────────────────────────────
export interface RunSummary {
  id: Id; workflowId: Id; workflowName: string;
  status: RunStatus;
  note?: string;                // "Enrich company · 503 · next attempt in 40 s"
  trigger: string;              // "Webhook", "Schedule", "Manual", "Workflow error"
  startedAt: ISODate; durationMs: number;
  nodesDone: number; nodesTotal: number;
  retries: number;
  correlationId?: string;
  revision: number;
  replayOf?: { runId: Id; fromStep: number };
}

export interface RunStep {
  index: number;                // 1-based
  nodeId: Id; name: string; kindLabel: string; category: NodeCategory;
  status: StepStatus;
  durationMs?: number;
  note?: string;
  attempts: { n: number; status: StepStatus; message: string; backoffMs?: number; at: ISODate }[];
  input?: unknown; inputBytes?: number; inputArtifact?: string;   // r2://… for large payloads
  output?: unknown; outputBytes?: number; outputArtifact?: string;
  logs: string[];               // already redacted
  sideEffect?: { kind: string; idempotent: boolean };
  timeline?: { startMs: number; endMs: number; kind: 'ok' | 'backoff' | 'fail' }[]; // for the Gantt
}

export interface RunDetail extends RunSummary {
  steps: RunStep[];
  checkpoints: { stepIndex: number; label: string; recommended?: boolean }[];
}

export interface ReplayRequest {
  runId: Id; fromStep: number;
  input: { kind: 'recorded' } | { kind: 'edited'; payload: unknown };
  target: 'production' | 'sandbox';
}

// ── Connectors & connections ────────────────────────────────────────
export interface ConnectorCard {
  id: Id; name: string; categoryLabel: string; // "MESSAGING"
  tab: 'core' | 'logic' | 'ai' | 'cf' | 'apps' | 'mon';
  category: NodeCategory; icon: string; description: string;
  capabilities: string[];       // ["OAuth 2.0", "Trigger", "Actions"]
  state: 'connected' | 'attention' | 'builtin' | 'none';
  stateLabel: string;           // "1 connection", "Reconnect", "Built in", "Not connected"
}

export interface McpTool {
  name: string; description: string;
  kind: 'read' | 'write' | 'destructive';
  policy: ToolPolicy;
}

export interface Connection {
  id: Id; name: string; account: string; // "ops@example.com", "tickets.example.com/mcp"
  connectorId: Id; category: NodeCategory; icon: string;
  authLabel: string;            // "OAuth 2.0 + PKCE", "MCP · OAuth 2.1"
  type: 'oauth' | 'apiKey' | 'header' | 'webhook' | 'app' | 'mcp';
  health: ConnectionHealth; statusLabel: string; // "Refresh failing", "Expires in 6 days"
  alert?: string;
  lastUsedAt?: ISODate; createdAt: ISODate;
  tokenLabel: string;           // "Refresh failed 02:00 · invalid_grant"
  keyVersion: string;           // "k3"
  scopes: string[];
  usedBy: { workflowId: Id; name: string }[];
  tools?: McpTool[];            // MCP connections only
}

// ── MCP server (Kasuri as a provider) ───────────────────────────────
export interface McpServerInfo {
  endpoint: string; transport: 'streamable-http'; auth: string; enabled: boolean;
  tools: { name: string; workflowId: Id; workflowName: string; inputSummary: string; policy: ToolPolicy; calls24h: number }[];
  clients: { id: Id; name: string; how: string; scope: string; lastUsedAt?: ISODate }[];
}

// ── Settings ────────────────────────────────────────────────────────
export interface ApiToken { id: Id; name: string; prefix: string; scopes: string[]; lastUsedAt?: ISODate; expiresAt?: ISODate }
export interface Session { id: Id; device: string; location: string; lastSeen: ISODate; current: boolean }
export interface Member { id: Id; name: string; email: string; initials: string; role: Role; lastActive: string }
export interface DeploymentResource { kind: string; names: string[]; metric: string }
export interface EncryptionKey { id: string; state: 'active' | 'retired'; note: string; from: ISODate; to?: ISODate }
