import { DatabaseSync } from 'node:sqlite'
import type { SqlValue, WorkspaceStorage } from '../../api/src/storage'

export class SqliteStorage implements WorkspaceStorage {
  private db: DatabaseSync
  private timer?: ReturnType<typeof setTimeout>
  private closed = false
  private onAlarm: () => Promise<void> = async () => {}
  readonly sql: WorkspaceStorage['sql']
  constructor(path: string) {
    this.db = new DatabaseSync(path)
    this.db.exec('PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; CREATE TABLE IF NOT EXISTS runtime_alarm (id INTEGER PRIMARY KEY CHECK(id=1), at INTEGER NOT NULL)')
    this.sql = { exec: <T extends Record<string, SqlValue>>(query: string, ...bindings: SqlValue[]) => {
      const rows = this.db.prepare(query).all(...bindings) as T[]
      return { toArray: () => rows, one: () => { if (rows.length !== 1) throw new Error('Expected one SQL row.'); return rows[0] } }
    } }
  }
  transactionSync<T>(callback: () => T): T {
    this.db.exec('BEGIN IMMEDIATE')
    try { const result = callback(); this.db.exec('COMMIT'); return result }
    catch (error) { this.db.exec('ROLLBACK'); throw error }
  }
  async setAlarm(at: number) {
    this.db.prepare('INSERT INTO runtime_alarm(id,at) VALUES(1,?) ON CONFLICT(id) DO UPDATE SET at=excluded.at').run(at)
    this.arm(at)
  }
  async deleteAlarm() { this.db.exec('DELETE FROM runtime_alarm'); clearTimeout(this.timer) }
  recover(onAlarm: () => Promise<void>) {
    this.onAlarm = onAlarm
    // Reconstruct timers from durable state, including a crash between run save and setAlarm.
    const times = [
      this.db.prepare('SELECT MIN(at) AS at FROM runtime_alarm').get()?.at,
      this.db.prepare('SELECT MIN(wake_at) AS at FROM runs WHERE host_error IS NULL').get()?.at,
      this.db.prepare('SELECT MIN(next_at) AS at FROM workflows').get()?.at,
      this.db.prepare("SELECT value AS at FROM meta WHERE key='expires_at'").get()?.at,
    ].filter((v): v is number => typeof v === 'number' && v > 0)
    if (times.length) this.arm(Math.min(...times))
  }
  private arm(at: number) {
    clearTimeout(this.timer)
    if (this.closed) return
    this.timer = setTimeout(() => {
      if (at > Date.now()) { this.arm(at); return }
      void this.onAlarm().catch(() => {
        console.error('Workspace alarm failed; scheduling recovery.')
        if (!this.closed) this.arm(Date.now() + 5000)
      })
    }, Math.min(86_400_000, Math.max(1, at - Date.now())))
    this.timer.unref()
  }
  close() { this.closed = true; clearTimeout(this.timer); this.db.close() }
}
