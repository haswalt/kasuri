import type { Json, Run, RunMode, Workflow, Workspace } from '../../../packages/contracts/src/index'
import type { Engine } from './engine-bridge'
import { runView } from './run-view'
import type { WorkspaceStorage } from './storage'
import { scheduleFor } from './schedule'
import { demoWorkflows } from './demos'
import { assert, HttpError } from './http'

type WorkflowRow = { id: string; document: string; published: string | null; next_at: number | null }
type RunRow = { id: string; checkpoint: string; mode: RunMode; created_at: number; wake_at: number | null; host_error: string | null }
const DAY = 86_400_000
const terminal = new Set(['succeeded', 'failed', 'cancelled'])

export class WorkspaceService {
  constructor(private storage: WorkspaceStorage, private engine: Engine, private sessionTtlMs = DAY) {
    this.storage.sql.exec('CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value INTEGER NOT NULL)')
    this.storage.sql.exec('CREATE TABLE IF NOT EXISTS workflows (id TEXT PRIMARY KEY, document TEXT NOT NULL, published TEXT, next_at INTEGER)')
    this.storage.sql.exec('CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, checkpoint TEXT NOT NULL, mode TEXT NOT NULL, created_at INTEGER NOT NULL, wake_at INTEGER, host_error TEXT, summary TEXT NOT NULL)')
  }
  async api(operation: string, payloadJson = '{}'): Promise<string> {
    try {
      const body = JSON.parse(payloadJson)
      let result: string
      switch (operation) {
        case 'bootstrap': result = await this.bootstrap(); break
        case 'saveWorkflow': result = await this.saveWorkflow(body.id, body.workflow); break
        case 'workflowAction': result = await this.workflowAction(body.id, body.action); break
        case 'createRun': result = await this.createRun(body.workflowId, body.mode, JSON.stringify(body.input)); break
        case 'getRun': result = await this.getRun(body.id); break
        case 'runAction': result = await this.runAction(body.id, body.action); break
        default: throw new HttpError(404, 'Unknown workspace operation.')
      }
      return JSON.stringify({ status: 200, value: JSON.parse(result) })
    } catch (error) {
      return JSON.stringify({ status: error instanceof HttpError ? error.status : 500, error: error instanceof HttpError ? error.message : 'The engine request could not be completed.' })
    }
  }
  private expiresAt(): number | undefined { return this.storage.sql.exec<{ value: number }>("SELECT value FROM meta WHERE key='expires_at'").toArray()[0]?.value }
  private ensureActive() { const expiry = this.expiresAt(); assert(expiry && Date.now() < expiry, 'This demo session has expired.', 410) }
  private workflow(id: string): WorkflowRow {
    const row = this.storage.sql.exec<WorkflowRow>('SELECT * FROM workflows WHERE id=?', id).toArray()[0]
    assert(row, 'Workflow not found.', 404); return row
  }
  private run(id: string): RunRow {
    const row = this.storage.sql.exec<RunRow>('SELECT * FROM runs WHERE id=?', id).toArray()[0]
    assert(row, 'Run not found.', 404); return row
  }
  private view(row: RunRow): Run { return runView(row.checkpoint, row.mode, row.created_at, row.wake_at, row.host_error) }
  private saveRun(row: RunRow) {
    const view = this.view(row)
    const summary = JSON.stringify({ ...view, input: null, nodes: view.nodes.map(({ output: _output, ...node }) => node) })
    this.storage.sql.exec('INSERT OR REPLACE INTO runs(id,checkpoint,mode,created_at,wake_at,host_error,summary) VALUES(?,?,?,?,?,?,?)', row.id, row.checkpoint, row.mode, row.created_at, row.wake_at, row.host_error, summary)
  }
  private async setNextAlarm() {
    const expiry = this.expiresAt(); if (!expiry) { await this.storage.deleteAlarm(); return }
    const run = this.storage.sql.exec<{ at: number | null }>('SELECT MIN(wake_at) AS at FROM runs WHERE host_error IS NULL').one().at
    const schedule = this.storage.sql.exec<{ at: number | null }>('SELECT MIN(next_at) AS at FROM workflows').one().at
    await this.storage.setAlarm(Math.max(Date.now() + 1, Math.min(expiry, run ?? expiry, schedule ?? expiry)))
  }
  private async bootstrap(): Promise<string> {
    if (!this.expiresAt()) {
      const now = Date.now()
      this.storage.transactionSync(() => {
        this.storage.sql.exec("INSERT INTO meta(key,value) VALUES('expires_at',?)", now + this.sessionTtlMs)
        for (const workflow of demoWorkflows(now)) this.storage.sql.exec('INSERT INTO workflows(id,document) VALUES(?,?)', workflow.id, JSON.stringify(workflow))
      })
      await this.setNextAlarm()
    }
    this.ensureActive()
    return JSON.stringify(this.snapshot())
  }
  private snapshot(): Workspace {
    return { workflows: this.storage.sql.exec<WorkflowRow>('SELECT * FROM workflows ORDER BY rowid').toArray().map(row => ({ ...JSON.parse(row.document), nextScheduledAt: row.next_at })), runs: this.storage.sql.exec<{ summary: string }>('SELECT summary FROM runs ORDER BY created_at DESC, rowid DESC LIMIT 100').toArray().map(row => JSON.parse(row.summary)), connectors: this.engine.connectors, expiresAt: this.expiresAt()! }
  }
  private async saveWorkflow(id: string, value: unknown): Promise<string> {
    this.ensureActive()
    assert(value && typeof value === 'object', 'Expected a workflow.')
    const workflow = value as Workflow
    assert(workflow.schemaVersion === 1 && workflow.id === id && id.length <= 160 && typeof workflow.name === 'string' && workflow.name.length <= 160 && typeof workflow.description === 'string', 'Invalid workflow metadata.')
    assert(Array.isArray(workflow.nodes) && workflow.nodes.length <= 100 && Array.isArray(workflow.edges) && workflow.edges.length <= 500, 'Invalid graph size.')
    for (const n of workflow.nodes) assert(n && typeof n.id === 'string' && typeof n.type === 'string' && typeof n.label === 'string' && n.config && typeof n.config === 'object' && !Array.isArray(n.config), 'Invalid node configuration.')
    for (const n of workflow.nodes) assert(n.position === undefined || (n.position && Number.isFinite(n.position.x) && Number.isFinite(n.position.y)), 'Invalid node position.')
    for (const e of workflow.edges) assert(e && typeof e.id === 'string' && typeof e.source === 'string' && typeof e.target === 'string' && (e.sourcePort === undefined || typeof e.sourcePort === 'string'), 'Invalid edge.')
    const existing = this.storage.sql.exec<WorkflowRow>('SELECT * FROM workflows WHERE id=?', id).toArray()[0]
    const count = this.storage.sql.exec<{ count: number }>('SELECT COUNT(*) AS count FROM workflows').one().count
    assert(existing || count < 20, 'This demo workspace supports 20 workflows.', 409)
    const saved: Workflow = { ...workflow, status: 'draft', revision: existing ? JSON.parse(existing.document).revision : 0, updatedAt: new Date().toISOString(), nextScheduledAt: null }
    this.storage.sql.exec('INSERT INTO workflows(id,document) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET document=excluded.document,next_at=NULL', id, JSON.stringify(saved))
    await this.setNextAlarm()
    return JSON.stringify(saved)
  }
  private validateExecutable(workflow: Workflow) {
    try { this.engine.validateGraph(workflow) } catch (error) { throw new HttpError(400, String(error)) }
    for (const node of workflow.nodes) assert(this.engine.connectors.some(c => c.kind === node.type && c.version === (node.version ?? 1)), `${node.type} is not implemented in this demo.`)
  }
  private async workflowAction(id: string, action: string): Promise<string> {
    this.ensureActive()
    const row = this.workflow(id); const workflow: Workflow = JSON.parse(row.document)
    if (action === 'pause') { workflow.status = 'paused' }
    else {
      assert(action === 'publish' || action === 'resume', 'Unknown workflow action.', 404)
      this.validateExecutable(workflow)
      workflow.status = 'active'; if (action === 'publish') workflow.revision += 1
    }
    workflow.updatedAt = new Date().toISOString()
    const scheduled = workflow.status === 'active' ? scheduleFor(workflow, Date.now()) : null
    const activeSchedules = this.storage.sql.exec<{ count: number }>('SELECT COUNT(*) AS count FROM workflows WHERE next_at IS NOT NULL AND id<>?', id).one().count
    assert(!scheduled || activeSchedules < 3, 'This demo supports three active schedules per workspace.', 409)
    const nextAt = scheduled && scheduled.at < this.expiresAt()! ? scheduled.at : null
    this.storage.sql.exec('UPDATE workflows SET document=?,published=?,next_at=? WHERE id=?', JSON.stringify(workflow), workflow.status === 'active' ? JSON.stringify(workflow) : row.published, nextAt, id)
    await this.setNextAlarm(); return JSON.stringify({ ...workflow, nextScheduledAt: nextAt })
  }
  private async createRun(workflowId: string, mode: RunMode, inputJson: string): Promise<string> {
    const input: Json = JSON.parse(inputJson)
    this.ensureActive(); assert(mode === 'run' || mode === 'simulation', 'Invalid run mode.')
    assert(JSON.stringify(input).length <= 16_384, 'Demo input is limited to 16 KiB.')
    const workflow: Workflow = JSON.parse(this.workflow(workflowId).document)
    this.validateExecutable(workflow)
    const activeCount = this.storage.sql.exec<{ count: number }>("SELECT COUNT(*) AS count FROM runs WHERE json_extract(summary,'$.status') IN ('running','paused')").one().count
    assert(activeCount < 5, 'Wait for an active demo run to finish before starting another.', 429)
    const row = this.newRun(workflow, mode, input, Date.now())
    this.saveRun(row)
    await this.setNextAlarm()
    this.prune()
    return JSON.stringify(this.view(row))
  }
  private newRun(workflow: Workflow, mode: RunMode, input: Json, now: number, id = crypto.randomUUID()): RunRow {
    const row: RunRow = { id, checkpoint: this.engine.startRun(id, workflow, input, now), mode, created_at: now, wake_at: null, host_error: null }
    this.advance(row, now)
    return row
  }
  private advance(row: RunRow, now: number) {
    // Core adapters are pure: no external effect occurs before this batch is persisted.
    // External adapters must use a persisted dispatch/outcome journal before being enabled.
    try {
      for (let i = 0; i < (row.mode === 'simulation' ? 260 : 1); i++) {
        const result = this.engine.advanceRun(row.checkpoint, now)
        row.checkpoint = result.checkpoint; row.wake_at = result.wakeAt
        if (!result.wakeAt || row.mode !== 'simulation') return
        now = result.wakeAt
      }
      row.host_error = 'simulation_budget_exceeded'; row.wake_at = null
    } catch { row.host_error = 'engine_transition_failed'; row.wake_at = null }
  }
  private prune() {
    // Retain 100 recent records; never prune active/paused runs.
    const rows = this.storage.sql.exec<{ id: string; summary: string }>('SELECT id,summary FROM runs ORDER BY created_at DESC,rowid DESC').toArray()
    for (const row of rows.slice(100)) if (terminal.has(JSON.parse(row.summary).status)) this.storage.sql.exec('DELETE FROM runs WHERE id=?', row.id)
  }
  private async getRun(id: string): Promise<string> { this.ensureActive(); return JSON.stringify(this.view(this.run(id))) }
  private async runAction(id: string, action: string): Promise<string> {
    this.ensureActive(); assert(['pause','resume','cancel'].includes(action), 'Unknown run action.', 404)
    const row = this.run(id)
    assert(!row.host_error, 'Run has already failed.', 409)
    try { row.checkpoint = this.engine.controlRun(row.checkpoint, action) } catch (error) { throw new HttpError(409, String(error)) }
    row.wake_at = action === 'resume' ? Date.now() + 1 : null
    if (action === 'resume') this.advance(row, Date.now())
    this.saveRun(row); await this.setNextAlarm(); return JSON.stringify(this.view(row))
  }
  async alarm(): Promise<void> {
    if ((this.expiresAt() ?? 0) <= Date.now()) {
      this.storage.sql.exec('DELETE FROM runs'); this.storage.sql.exec('DELETE FROM workflows')
      // Retain the expiry marker so an old session cannot recreate an expired workspace.
      await this.storage.deleteAlarm(); return
    }
    const now = Date.now()
    const due = this.storage.sql.exec<RunRow>('SELECT * FROM runs WHERE wake_at<=? AND host_error IS NULL LIMIT 5', now).toArray()
    this.storage.transactionSync(() => { for (const row of due) { this.advance(row, now); this.saveRun(row) } })
    const schedules = this.storage.sql.exec<WorkflowRow>('SELECT * FROM workflows WHERE next_at<=? LIMIT 3', now).toArray()
    this.storage.transactionSync(() => {
      for (const row of schedules) {
        const workflow: Workflow = JSON.parse(row.published!)
        const dueAt = row.next_at!
        const id = `cron-${workflow.id}-${workflow.revision}-${dueAt}`
        if (!this.storage.sql.exec('SELECT id FROM runs WHERE id=?', id).toArray().length) {
          const trigger = workflow.nodes.find(n => n.type === 'Schedule')!
          const input = { trigger: 'schedule', scheduledAt: new Date(dueAt).toISOString(), firedAt: new Date(now).toISOString(), cron: trigger.config.cron, timezone: trigger.config.timezone || 'UTC' }
          const active = this.storage.sql.exec<{ count: number }>("SELECT COUNT(*) AS count FROM runs WHERE json_extract(summary,'$.status') IN ('running','paused')").one().count
          const run = active < 5 ? this.newRun(workflow, 'schedule', input, now, id) : { id, checkpoint: this.engine.startRun(id, workflow, input, now), mode: 'schedule' as const, created_at: now, wake_at: null, host_error: 'concurrency_limit' }
          this.saveRun(run)
        }
        // Coalesce missed occurrences into one run, then compute the next future event.
        const next = scheduleFor(workflow, now)?.at
        this.storage.sql.exec('UPDATE workflows SET next_at=? WHERE id=?', next && next < this.expiresAt()! ? next : null, row.id)
      }
    })
    this.prune()
    await this.setNextAlarm()
  }
}
