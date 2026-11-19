import { useCallback, useEffect, useRef, useState } from 'react'
import type { DataDefinition, DataField, DataFilter, DataKind, DataRow, DataTable, DataTableSummary, Json } from '../../../../packages/contracts/src/index'
import { api } from '../lib/api'
import { uid } from '../lib/workflows'
import { go, path, readRoute } from '../lib/navigation'
import { Button, Dialog, Empty, Icon, PageHeader } from '../design/ui'
import { styles } from '../design/styles'
export function DataTables({ tableId }: { tableId?: string }) {
  const [tables, setTables] = useState<DataTableSummary[]>([])
  const [tableCursor, setTableCursor] = useState<string | null>(null)
  const [table, setTable] = useState<DataTable | null>(null)
  const [rows, setRows] = useState<DataRow[]>([])
  const [cursor, setCursor] = useState<string | null>(null)
  const [filters, setFilters] = useState<DataFilter[]>([])
  const [loaded, setLoaded] = useState(false)
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [formError, setFormError] = useState('')
  const [dialog, setDialog] = useState<'create' | 'row' | 'delete' | 'rename' | 'filter' | null>(null)
  const [name, setName] = useState('')
  const [id, setId] = useState('')
  const [idEdited, setIdEdited] = useState(false)
  const [kind, setKind] = useState<DataKind>('table')
  const [fields, setFields] = useState('[]')
  const [selectedRow, setSelectedRow] = useState<DataRow | null>(null)
  const [rowKey, setRowKey] = useState('')
  const [rowValue, setRowValue] = useState('{}')
  const [ttl, setTtl] = useState('')
  const [ttlChanged, setTtlChanged] = useState(false)
  const [filterText, setFilterText] = useState('[]')
  const mounted = useRef(true)
  const workspace = useRef(readRoute().workspaceKey)
  const epoch = useRef(0)
  const catalogueEpoch = useRef(0)
  const pending = useRef(false)
  const active = useCallback(() => mounted.current && readRoute().workspaceKey === workspace.current, [])
  const catalogue = useCallback(async (after?: string) => {
    const at = ++catalogueEpoch.current
    const result = await api.dataTables(after)
    if (!active() || at !== catalogueEpoch.current) return
    setTables((old) => after ? [...new Map([...old, ...result.tables].map((entry) => [entry.id, entry])).values()] : result.tables)
    setTableCursor(result.nextCursor)
  }, [active])
  const records = useCallback(async (after?: string) => {
    if (!tableId) return
    const at = ++epoch.current
    setLoading(true)
    try {
      const [definition, result] = await Promise.all([
        api.data({ operation: 'get_table', tableId }),
        api.data({ operation: 'query', tableId, query: { limit: 25, filters, ...(after ? { cursor: after } : {}) } })
      ])
      if (!active() || at !== epoch.current) return
      setTable(definition.result.table || null)
      setRows((old) => after ? [...old, ...(result.result.rows || [])] : result.result.rows || [])
      setCursor(result.result.nextCursor || null)
    } finally { if (active() && at === epoch.current) setLoading(false) }
  }, [tableId, filters, active])
  const refresh = useCallback(async () => {
    setError('')
    try { await Promise.all([catalogue(), records()]) }
    catch (e) { if (active()) setError(e instanceof Error ? e.message : 'Could not load data') }
    finally { if (active()) setLoaded(true) }
  }, [catalogue, records, active])
  useEffect(() => {
    mounted.current = true
    void refresh()
    return () => { mounted.current = false }
  }, [refresh])
  function open(next: NonNullable<typeof dialog>) { setFormError(''); setDialog(next) }
  function create() { setName(''); setId('data-' + uid().slice(0, 8)); setIdEdited(false); setKind('table'); setFields('[]'); open('create') }
  function edit(row?: DataRow) {
    setSelectedRow(row || null); setRowKey(row?.key || '')
    const defaults: Record<string, Json> = {}
    for (const field of table?.definition.fields || []) if (field.required) defaults[field.name] = field.kind === 'number' ? 0 : field.kind === 'boolean' ? false : field.kind === 'array' ? [] : field.kind === 'object' ? {} : field.kind === 'any' ? null : ''
    setRowValue(JSON.stringify(row ? row.value : table?.definition.kind === 'context' ? null : defaults, null, 2))
    setTtl(row?.expiresAt ? String(Math.max(1, Math.ceil((row.expiresAt - Date.now()) / 1000))) : '')
    setTtlChanged(false); open('row')
  }
  async function perform(action: () => Promise<void>) {
    if (pending.current) return
    pending.current = true
    setBusy(true); setFormError(''); setNotice('')
    try { await action() }
    catch (e) { if (active()) setFormError(e instanceof Error ? e.message : 'Data operation failed') }
    finally { pending.current = false; if (active()) setBusy(false) }
  }
  function close() { if (!busy) setDialog(null) }
  async function submit() {
    if (dialog === 'create') {
      const parsed: unknown = kind === 'context' ? [] : JSON.parse(fields)
      if (!Array.isArray(parsed)) throw Error('Fields must be a JSON array.')
      const definition: DataDefinition = { id, name: name.trim(), kind, fields: parsed as DataField[] }
      const result = await api.data({ operation: 'create_table', definition }, uid())
      if (!active()) return
      if (result.result.status === 'conflict') throw Error('That ID belongs to a different data set. Choose another ID.')
      setDialog(null); go(path('data/' + id)); return
    }
    if (!tableId || !table) return
    if (dialog === 'row') {
      const value: Json = JSON.parse(rowValue)
      let ttlMs: number | undefined
      if (selectedRow?.expiresAt && !ttlChanged) ttlMs = Math.max(1, selectedRow.expiresAt - Date.now())
      else if (ttl) { ttlMs = Math.round(Number(ttl) * 1000); if (!Number.isSafeInteger(ttlMs) || ttlMs <= 0) throw Error('Use a positive time to live, or leave it blank.') }
      const result = await api.data(selectedRow ? { operation: 'put', tableId, key: selectedRow.key, value, expectedVersion: selectedRow.version, ttlMs } : { operation: 'insert_if_absent', tableId, key: rowKey, value, ttlMs }, uid())
      if (!active()) return
      if (result.result.status === 'conflict') throw Error('This record changed or expired. Reopen it to review the latest version.')
      if (result.result.status === 'existing') throw Error('That key already exists. Open the record to edit it.')
      setDialog(null); setNotice('Record saved.'); await refresh()
    } else if (dialog === 'delete' && selectedRow) {
      const result = await api.data({ operation: 'delete', tableId, key: selectedRow.key, expectedVersion: selectedRow.version }, uid())
      if (!active()) return
      if (result.result.status === 'conflict') throw Error('The record changed. Reopen it before deleting.')
      setDialog(null); setNotice('Record deleted.'); await refresh()
    } else if (dialog === 'rename') {
      const result = await api.data({ operation: 'rename_table', tableId, name: name.trim(), expectedVersion: table.version }, uid())
      if (!active()) return
      if (result.result.status === 'conflict') throw Error('The table changed. Refresh before renaming it.')
      setDialog(null); setNotice('Data set renamed.'); await refresh()
    } else if (dialog === 'filter') {
      const parsed: unknown = JSON.parse(filterText)
      if (!Array.isArray(parsed)) throw Error('Filters must be a JSON array.')
      // Validate with the server before applying the new view.
      const result = await api.data({ operation: 'query', tableId, query: { limit: 25, filters: parsed as DataFilter[] } })
      if (!active()) return
      setRows(result.result.rows || []); setCursor(result.result.nextCursor || null)
      setFilters(parsed as DataFilter[]); setDialog(null)
    }
  }
  return <>
    <PageHeader eyebrow="WORKSPACE DATA" title="Data tables" description="Store structured records and key/value context in this workspace." actions={<><Button disabled={busy || loading} onClick={() => void refresh()}>Refresh</Button><Button visual="primary" disabled={busy} onClick={create}><Icon name="plus" />New data set</Button></>} />
    {error && <div className={styles.errorPanel} role="alert">{error}</div>}
    {notice && <div className={styles.dataNotice} role="status">{notice}</div>}
    <div className={styles.dataLayout}>
      <aside className={styles.dataSidebar} aria-label="Data sets">
        <span className={styles.eyebrow}>DATA SETS</span>
        {tables.map((entry) => <Button key={entry.id} disabled={busy} aria-current={tableId === entry.id ? 'page' : undefined} onClick={() => go(path('data/' + entry.id))}><Icon name={entry.kind === 'context' ? 'hash' : 'database'} /><span>{entry.name}<small>{entry.id} · {entry.kind === 'context' ? 'Context' : 'Table'}</small></span></Button>)}
        {!loaded && <small>Loading…</small>}
        {loaded && !tables.length && <small>No data sets yet.</small>}
        {tableCursor && <Button disabled={busy} onClick={() => void catalogue(tableCursor).catch((e) => setError(String(e)))}>More data sets</Button>}
      </aside>
      <section className={styles.dataContent}>
        {!tableId ? <Empty icon="database" title="Workspace data, ready for your logic" action={<Button visual="primary" onClick={create}>Create a data set</Button>}>Choose a table for structured records or a context namespace for arbitrary JSON values.</Empty> : table ? <>
          <div className={styles.dataHeading}><div><h2>{table.definition.name}</h2><small className={styles.mono}>{table.definition.id} · {table.definition.kind === 'context' ? 'Context namespace' : 'Data table'}</small></div><div className={styles.actions}><Button disabled={busy} onClick={() => { setName(table.definition.name); open('rename') }}>Rename</Button><Button disabled={busy} onClick={() => { setFilterText(JSON.stringify(filters, null, 2)); open('filter') }}>Filters{filters.length ? ` (${filters.length})` : ''}</Button><Button visual="primary" disabled={busy} onClick={() => edit()}><Icon name="plus" />Add record</Button></div></div>
          <div className={styles.dataFields}>{table.definition.fields.length ? table.definition.fields.map((field) => <span className={styles.chip} key={field.name}>{field.name} · {field.kind}{field.required ? ' · required' : ''}</span>) : <span>{table.definition.kind === 'context' ? 'Values may be any JSON type.' : 'Records may contain any JSON object fields.'}</span>}</div>
          <div className={styles.panel}>
            <div className={styles.tableScroll}><table><thead><tr><th>Key</th><th>Value</th><th>Version</th><th>Expiry</th><th>Actions</th></tr></thead><tbody>{rows.map((row) => <tr key={row.key}><td className={styles.mono}>{row.key}</td><td><code className={styles.dataValue}>{JSON.stringify(row.value).slice(0, 160)}</code></td><td>{row.version}</td><td>{row.expiresAt ? new Date(row.expiresAt).toLocaleString() : 'No expiry'}</td><td><div className={styles.actions}><Button disabled={busy} aria-label={'Edit record ' + row.key} onClick={() => edit(row)}>Edit</Button><Button tone="danger" disabled={busy} aria-label={'Delete record ' + row.key} onClick={() => { setSelectedRow(row); open('delete') }}>Delete</Button></div></td></tr>)}</tbody></table></div>
            {!rows.length && !loading && <Empty icon="database" title={filters.length ? 'No matching records on this page' : 'No records on this page'}>Add a record or continue to the next page if more results are available.</Empty>}
            <div className={styles.tableFooter}><small>{rows.length} loaded records{loading ? ' · Loading…' : ''}</small><Button disabled={busy || loading || !cursor} onClick={() => void records(cursor || undefined).catch((e) => setError(String(e)))}>Load more records</Button></div>
          </div>
        </> : loaded ? <Empty icon="warning" title="Data set unavailable">Choose another data set or create one.</Empty> : <p>Loading data set…</p>}
      </section>
    </div>
    {dialog && <Dialog title={dialog === 'create' ? 'Create data set' : dialog === 'row' ? selectedRow ? 'Edit record' : 'Add record' : dialog === 'delete' ? 'Delete record' : dialog === 'filter' ? 'Filter records' : 'Rename data set'} onClose={close}><form className={styles.settingsFields} onSubmit={(event) => { event.preventDefault(); void perform(submit) }}>
      {dialog === 'create' ? <>
        <label>Name<input required maxLength={160} value={name} onChange={(e) => { setName(e.target.value); if (!idEdited) setId(e.target.value.toLowerCase().replace(/[^a-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 64) || id) }} /></label>
        <label>Stable ID<input required pattern="[A-Za-z0-9_-]+" maxLength={64} value={id} onChange={(e) => { setIdEdited(true); setId(e.target.value) }} /></label>
        <small>Use letters, numbers, hyphens or underscores. Renaming keeps this ID.</small>
        <label>Kind<select value={kind} onChange={(e) => setKind(e.target.value as DataKind)}><option value="table">Table · JSON objects</option><option value="context">Context · any JSON value</option></select></label>
        {kind === 'table' && <label>Fields · JSON array<textarea className={styles.mono} rows={7} value={fields} onChange={(e) => setFields(e.target.value)} /><small>Example: {`[{"name":"active","kind":"boolean","required":true}]`}. Leave [] for flexible object records.</small></label>}
      </> : dialog === 'row' ? <>
        <label>Key<input required maxLength={512} disabled={!!selectedRow} value={rowKey} onChange={(e) => setRowKey(e.target.value)} /></label>
        <label>Value · JSON<textarea className={styles.mono} rows={12} value={rowValue} onChange={(e) => setRowValue(e.target.value)} /></label>
        <label>Time to live · seconds<input type="number" min="0.001" step="any" value={ttl} onChange={(e) => { setTtl(e.target.value); setTtlChanged(true) }} /><small>Leave blank for no expiry.</small></label>
        {selectedRow && <small>Editing version {selectedRow.version}. A conflicting update will preserve the newer record.</small>}
      </> : dialog === 'delete' ? <p>Permanently delete record “{selectedRow?.key}”? This cannot be undone.</p> : dialog === 'rename' ? <label>Name<input required maxLength={160} value={name} onChange={(e) => setName(e.target.value)} /></label> : <label>Filters · JSON array<textarea className={styles.mono} rows={10} value={filterText} onChange={(e) => setFilterText(e.target.value)} /><small>Example: {`[{"pointer":"/active","operator":"eq","value":true}]`}. Operators: eq, ne, gt, lt, exists.</small></label>}
      {formError && <p className={styles.errorPanel} role="alert">{formError}</p>}
      <div className={styles.actions}><Button type="button" disabled={busy} onClick={close}>Cancel</Button><Button type="submit" visual={dialog === 'delete' ? undefined : 'primary'} tone={dialog === 'delete' ? 'danger' : undefined} disabled={busy}>{busy ? 'Saving…' : dialog === 'delete' ? 'Delete record' : dialog === 'filter' ? 'Apply filters' : dialog === 'create' ? 'Create data set' : 'Save'}</Button></div>
    </form></Dialog>}
  </>
}
