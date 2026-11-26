import { useEffect, useState } from 'react'
import type { DataField, DataTableSummary, Json, Workflow } from '../../../../packages/contracts/src/index'
import { api } from '../lib/api'
import { decodeJson, fieldChoices, fieldPointer, type InputHints } from '../lib/input-hints'
import { upstreamNodes } from '../lib/expressions'
import { Button } from '../design/ui'
import { styles } from '../design/styles'
import { FieldPicker } from './FieldPicker'
import { FixedValue } from './ExpressionBuilder'
import { TransformBuilder } from './TransformBuilder'
import { DataFieldsEditor } from './DataFieldsEditor'

const operations = [
  ['get', 'Read a record'], ['insert_if_absent', 'Insert only if the key is new'],
  ['put', 'Create or replace a record'], ['delete', 'Delete a record'],
  ['query', 'List records'], ['get_table', 'Read dataset definition'], ['create_table', 'Create or ensure a dataset']
]
const object = (value: Json | undefined): Record<string, Json> =>
  value && typeof value === 'object' && !Array.isArray(value) ? value : {}
const text = (value: Json | undefined) => typeof value === 'string' ? value : ''

function BoundValue({ label, value, hints, onChange, keyOnly = false }: {
  label: string
  value: Json | undefined
  hints: InputHints
  onChange: (value: Json) => void
  keyOnly?: boolean
}) {
  const pointer = object(value).$input
  const bound = typeof pointer === 'string' && Object.keys(object(value)).length === 1
  return <fieldset className={styles.conditionRow}>
    <legend>{label}</legend>
    <label>{label} source<select value={bound ? 'incoming' : 'fixed'} onChange={event =>
      onChange(event.target.value === 'incoming' ? { $input: '' } : keyOnly ? '' : null)}>
      <option value="fixed">Fixed value</option><option value="incoming">Incoming field</option>
    </select></label>
    {bound
      ? <FieldPicker label={`${label} input field`} searchable value={pointer || '$'} choices={fieldChoices(hints)}
        onChange={field => onChange({ $input: fieldPointer(field) })} />
      : keyOnly ? <label>Record key<input value={text(value)} maxLength={512} onChange={event => onChange(event.target.value)} /></label>
      : <FixedValue value={value ?? null} onChange={onChange} />}
  </fieldset>
}

export function DataCommandBuilder({ workflow, nodeId, config, kind, hints, onChange }: {
  workflow: Workflow
  nodeId: string
  config: Record<string, Json>
  kind: 'table' | 'context'
  hints: InputHints
  onChange: (config: Record<string, Json>) => void
}) {
  const [tables, setTables] = useState<DataTableSummary[]>([])
  const [cursor, setCursor] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [datasetSearch, setDatasetSearch] = useState('')
  const command = object(decodeJson(config.command))
  const operation = text(command.operation)
  const definition = object(command.definition)
  const fields = definition.fields
  const editableFields = Array.isArray(fields) && fields.every(field => typeof object(field).name === 'string' &&
    ['string', 'number', 'boolean', 'object', 'array', 'any'].includes(text(object(field).kind)))
  const tableId = text(command.tableId)
  const create = operation === 'create_table'
  const recordKey = ['get', 'insert_if_absent', 'put', 'delete'].includes(operation)
  const writeValue = operation === 'put' || operation === 'insert_if_absent'
  const query = object(command.query)
  const value = command.value
  const structuredRecord = kind === 'table' && (value === undefined ||
    Boolean(value && typeof value === 'object' && !Array.isArray(value) && !Object.hasOwn(value, '$input')))
  const setCommand = (next: Json) => onChange({ ...config, command: next })
  const edit = (key: string, next: Json) => setCommand({ ...command, [key]: next })
  const upstream = upstreamNodes(workflow, nodeId).flatMap(node => {
    if (node.type !== (kind === 'table' ? 'Data Tables' : 'Context Store')) return []
    const source = object(decodeJson(node.config.command))
    const created = object(source.definition)
    return source.operation === 'create_table' && created.kind === kind && text(created.id)
      ? [{ id: text(created.id), name: text(created.name) || text(created.id) }] : []
  })
  const choices = [...new Map([
    ...upstream, ...tables.filter(table => table.kind === kind)
  ].map(table => [table.id, table])).values()]
  const shown = choices.filter(table => `${table.name} ${table.id}`.toLowerCase().includes(datasetSearch.toLowerCase()))
  const knownOperation = operations.some(([value]) => value === operation)

  useEffect(() => {
    let active = true
    void api.dataTables().then(result => {
      if (active) { setTables(result.tables); setCursor(result.nextCursor) }
    }).catch(failure => {
      if (active) setError(failure instanceof Error ? failure.message : 'Could not load datasets.')
    })
    return () => { active = false }
  }, [])

  async function loadMore() {
    if (!cursor || loading) return
    setLoading(true)
    setError('')
    try {
      const result = await api.dataTables(cursor)
      setTables(current => [...current, ...result.tables])
      setCursor(result.nextCursor)
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : 'Could not load datasets.')
    } finally {
      setLoading(false)
    }
  }

  function changeOperation(next: string) {
    if (next === 'create_table') {
      setCommand({ operation: next, definition: { id: tableId, name: '', kind, fields: [] } })
      return
    }
    setCommand({
      operation: next, tableId: create ? text(definition.id) : tableId,
      ...(['get', 'insert_if_absent', 'put', 'delete'].includes(next) ? { key: command.key ?? '' } : {}),
      ...(['insert_if_absent', 'put'].includes(next) ? { value: command.value ?? (kind === 'table' ? {} : null) } : {})
    })
  }

  return <section className={styles.conditionBuilder}>
    <label>Operation<select value={operation} onChange={event => changeOperation(event.target.value)}>
      <option value="" disabled>Choose an operation…</option>
      {operation && !knownOperation && <option value={operation}>Advanced: {operation}</option>}
      {operations.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
    </select></label>
    {create && <>
      <p>The workflow creates this dataset if it does not exist. Choose the same ID on downstream nodes.</p>
      <label>Dataset ID<input value={text(definition.id)} maxLength={64} pattern="[A-Za-z0-9_\\-]+"
        onChange={event => edit('definition', { ...definition, id: event.target.value })} /></label>
      <label>Dataset name<input value={text(definition.name)} maxLength={160}
        onChange={event => edit('definition', { ...definition, name: event.target.value })} /></label>
      {kind === 'table' && (editableFields || fields === undefined
        ? <DataFieldsEditor fields={editableFields ? fields as DataField[] : []}
          onChange={fields => edit('definition', { ...definition, fields })} />
        : <p role="alert">Check the saved field definitions in Advanced command JSON.</p>)}
    </>}
    {knownOperation && !create && <>
      {error && <p role="alert">{error}</p>}
      {choices.length > 10 && <label>Find a dataset<input value={datasetSearch} onChange={event => setDatasetSearch(event.target.value)} /></label>}
      <label>Dataset<select value={tableId} onChange={event => edit('tableId', event.target.value)}>
        <option value="" disabled>Choose a dataset…</option>
        {tableId && !shown.some(table => table.id === tableId) && <option value={tableId}>{tableId} (saved ID)</option>}
        {shown.map(table => <option key={table.id} value={table.id}>{table.name} · {table.id}</option>)}
      </select></label>
      {cursor && <Button disabled={loading} onClick={() => void loadMore()}>{loading ? 'Loading…' : 'Load more datasets'}</Button>}
      {!choices.length && <p>Create a dataset in Data tables or add a Create dataset operation upstream.</p>}
    </>}
    {recordKey && <BoundValue label="Key" keyOnly value={command.key} hints={hints} onChange={value => edit('key', value)} />}
    {writeValue && <>
      {kind === 'table' && <label>Record source<select value={structuredRecord ? 'fields' : 'input'} onChange={event =>
        edit('value', event.target.value === 'fields' ? {} : { $input: '' })}>
        <option value="fields">Build a record</option><option value="input">Use an incoming object</option>
      </select></label>}
      {structuredRecord
        ? <TransformBuilder config={{ mapping: value ?? {} }} hints={hints} target="record" onChange={next => edit('value', next.mapping)} />
        : kind === 'table' && typeof object(value).$input === 'string'
          ? <FieldPicker label="Record input field" searchable value={text(object(value).$input) || '$'} choices={fieldChoices(hints)}
            onChange={field => edit('value', { $input: fieldPointer(field) })} />
          : <BoundValue label="Value" value={value} hints={hints} onChange={value => edit('value', value)} />}
      {operation === 'insert_if_absent' && <p>Use an IF on the Boolean <code>inserted</code> output to distinguish the first request from a duplicate.</p>}
    </>}
    {operation === 'query' && <label>Maximum records<input type="number" min={1} step={1}
      value={typeof query.limit === 'number' ? query.limit : ''}
      placeholder="25" onChange={event => {
        const next = { ...query }
        if (event.target.value) next.limit = event.target.valueAsNumber
        else delete next.limit
        edit('query', next)
      }} /></label>}
    <details open={Boolean(config.command && !knownOperation) || undefined}>
      <summary>Advanced command JSON</summary>
      <p>Use this for conditional writes, expiry, filters, and commands with additional options.</p>
      <label>Command JSON<textarea rows={8} spellCheck={false}
        value={typeof config.command === 'string' ? config.command : JSON.stringify(config.command ?? {}, null, 2)}
        onChange={event => setCommand(event.target.value)} /></label>
    </details>
  </section>
}
