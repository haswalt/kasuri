// Kasuri design tokens as typed values. Components should prefer the CSS variables
// (var(--accent) etc.); use these maps to pick the right variable for a status or category.

export const cssVar = (name: string) => `var(--${name})`;

export type RunStatus =
  | 'queued' | 'running' | 'waiting' | 'retrying' | 'succeeded' | 'failed' | 'cancelled';

export type StepStatus =
  | 'pending' | 'running' | 'retrying' | 'waiting' | 'success' | 'failed' | 'skipped' | 'cancelled';

export type WorkflowStatus = 'active' | 'paused' | 'inactive' | 'draft';

export type ConnectionHealth = 'ok' | 'warn' | 'fail' | 'idle';

export type NodeCategory = 'trigger' | 'http' | 'logic' | 'cf' | 'app' | 'error';

export type ToolPolicy = 'auto' | 'ask' | 'blocked';

/** Foreground + soft background CSS variables plus the user-facing label. */
export interface Tone { fg: string; bg: string; label: string }

export const runStatusTone: Record<RunStatus, Tone> = {
  queued:    { fg: 'var(--skip)',   bg: 'var(--surface-2)', label: 'Queued' },
  running:   { fg: 'var(--run)',    bg: 'var(--run-soft)',  label: 'Running' },
  waiting:   { fg: 'var(--wait)',   bg: 'var(--wait-soft)', label: 'Waiting' },
  retrying:  { fg: 'var(--warn)',   bg: 'var(--warn-soft)', label: 'Retrying' },
  succeeded: { fg: 'var(--ok)',     bg: 'var(--ok-soft)',   label: 'Succeeded' },
  failed:    { fg: 'var(--fail)',   bg: 'var(--fail-soft)', label: 'Failed' },
  cancelled: { fg: 'var(--text-2)', bg: 'var(--surface-2)', label: 'Cancelled' },
};

export const stepStatusTone: Record<StepStatus, Tone> = {
  pending:   { fg: 'var(--text-3)', bg: 'var(--surface-2)', label: 'Pending' },
  running:   { fg: 'var(--run)',    bg: 'var(--run-soft)',  label: 'Running' },
  retrying:  { fg: 'var(--warn)',   bg: 'var(--warn-soft)', label: 'Retrying' },
  waiting:   { fg: 'var(--wait)',   bg: 'var(--wait-soft)', label: 'Waiting' },
  success:   { fg: 'var(--ok)',     bg: 'var(--ok-soft)',   label: 'Succeeded' },
  failed:    { fg: 'var(--fail)',   bg: 'var(--fail-soft)', label: 'Failed' },
  skipped:   { fg: 'var(--skip)',   bg: 'var(--surface-2)', label: 'Skipped' },
  cancelled: { fg: 'var(--text-2)', bg: 'var(--surface-2)', label: 'Cancelled' },
};

export const workflowStatusTone: Record<WorkflowStatus, Tone> = {
  active:   { fg: 'var(--ok)',     bg: 'var(--ok-soft)',   label: 'Active' },
  paused:   { fg: 'var(--warn)',   bg: 'var(--warn-soft)', label: 'Paused' },
  inactive: { fg: 'var(--text-2)', bg: 'var(--surface-2)', label: 'Inactive' },
  draft:    { fg: 'var(--text-2)', bg: 'var(--surface-2)', label: 'Draft' },
};

export const connectionTone: Record<ConnectionHealth, Tone> = {
  ok:   { fg: 'var(--ok)',     bg: 'var(--ok-soft)',   label: 'Healthy' },
  warn: { fg: 'var(--warn)',   bg: 'var(--warn-soft)', label: 'Expiring' },
  fail: { fg: 'var(--fail)',   bg: 'var(--fail-soft)', label: 'Needs reconnect' },
  idle: { fg: 'var(--text-2)', bg: 'var(--surface-2)', label: 'Unused' },
};

export const toolPolicyTone: Record<ToolPolicy, Tone> = {
  auto:    { fg: 'var(--ok)',   bg: 'var(--ok-soft)',   label: 'Auto' },
  ask:     { fg: 'var(--wait)', bg: 'var(--wait-soft)', label: 'Ask first' },
  blocked: { fg: 'var(--fail)', bg: 'var(--fail-soft)', label: 'Blocked' },
};
export const toolPolicyCycle: ToolPolicy[] = ['auto', 'ask', 'blocked'];

export const categoryTone: Record<NodeCategory, { fg: string; bg: string }> = {
  trigger: { fg: 'var(--c-trigger)', bg: 'var(--c-trigger-bg)' },
  http:    { fg: 'var(--c-http)',    bg: 'var(--c-http-bg)' },
  logic:   { fg: 'var(--c-logic)',   bg: 'var(--c-logic-bg)' },
  cf:      { fg: 'var(--c-cf)',      bg: 'var(--c-cf-bg)' },
  app:     { fg: 'var(--c-app)',     bg: 'var(--c-app-bg)' },
  error:   { fg: 'var(--c-error)',   bg: 'var(--c-error-bg)' },
};

export const layout = {
  sidebarWidth: 240,
  headerHeight: 56,
  statusBarHeight: 28,
  pagePaddingX: 40,
  pagePaddingY: 32,
  editor: { paletteWidth: 240, inspectorWidth: 320, simToolbarHeight: 48 },
  node: { width: 200, height: 56, radius: 8, tile: 30 },
  minViewportWidth: 1280,
} as const;

/** Logo colours are fixed and work on both themes. */
export const logoColours = {
  weft: '#4CC3CB', weftFold: '#2A9AA4', warp: '#138A96', warpFold: '#0B6470', shadow: '#04363C',
} as const;
