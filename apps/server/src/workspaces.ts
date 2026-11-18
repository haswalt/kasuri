import { DatabaseSync } from 'node:sqlite'
import { mkdirSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { WorkspaceService } from '../../api/src/workspace-service'
import type { Engine } from '../../api/src/engine-bridge'
import type { WorkspaceStub } from '../../api/src/router'
import { SqliteStorage } from './storage'

class LocalWorkspace implements WorkspaceStub {
  private queue: Promise<unknown> = Promise.resolve()
  private service: WorkspaceService
  private storage: SqliteStorage
  constructor(path: string, engine: Engine, sessionTtlMs: number) {
    this.storage = new SqliteStorage(path)
    this.service = new WorkspaceService(this.storage, engine, sessionTtlMs)
    this.storage.recover(() => this.enqueue(() => this.service.alarm()))
  }
  private enqueue<T>(task: () => Promise<T>): Promise<T> {
    const next = this.queue.then(task)
    this.queue = next.catch(() => {})
    return next
  }
  api(operation: string, payloadJson: string) { return this.enqueue(() => this.service.api(operation, payloadJson)) }
  async close() { await this.queue; this.storage.close() }
}

export class WorkspaceRegistry {
  private workspaces = new Map<string, LocalWorkspace>()
  private lock: DatabaseSync
  constructor(private directory: string, private engine: Engine, private sessionTtlMs: number) {
    mkdirSync(directory, { recursive: true })
    // Hold an OS-managed SQLite lock: a second host cannot schedule the same data volume.
    // The OS releases it on crash, so recovery never relies on deleting stale PID files.
    this.lock = new DatabaseSync(join(directory, '.host.sqlite'))
    try { this.lock.exec('PRAGMA busy_timeout=0; CREATE TABLE IF NOT EXISTS host_lock (id INTEGER); BEGIN IMMEDIATE') }
    catch (error) { this.lock.close(); throw new Error('DATA_DIR is already owned by another server.', { cause: error }) }
    for (const file of readdirSync(directory)) if (/^[a-f0-9-]{36}\.sqlite$/.test(file)) this.getByName(file.slice(0, -7))
  }
  getByName(id: string): WorkspaceStub {
    if (!/^[a-f0-9-]{36}$/.test(id)) throw new Error('Invalid workspace ID.')
    let workspace = this.workspaces.get(id)
    if (!workspace) { workspace = new LocalWorkspace(join(this.directory, `${id}.sqlite`), this.engine, this.sessionTtlMs); this.workspaces.set(id, workspace) }
    return workspace
  }
  async close() { await Promise.all([...this.workspaces.values()].map(w => w.close())); this.lock.close() }
}
