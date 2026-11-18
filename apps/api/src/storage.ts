export type SqlValue = string | number | null
export interface SqlRows<T> { toArray(): T[]; one(): T }
export interface WorkspaceStorage {
  sql: { exec<T extends Record<string, SqlValue> = Record<string, SqlValue>>(query: string, ...bindings: SqlValue[]): SqlRows<T> }
  transactionSync<T>(callback: () => T): T
  setAlarm(at: number): Promise<void>
  deleteAlarm(): Promise<void>
}
