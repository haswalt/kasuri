import { useEffect, useState } from 'react'
import type { Json } from '../../../../packages/contracts/src/index'
import { styles } from '../design/styles'
import { api } from '../lib/api'
import type { McpClientConnection } from '../lib/api'
import { fieldChoices, fieldPointer, inputValue, schemaAt } from '../lib/input-hints'
import type { InputHints } from '../lib/input-hints'
import { FieldPicker } from './FieldPicker'

type Props = {
  config: Record<string, Json>
  hints: InputHints
  onChange: (config: Record<string, Json>) => void
}
const string = (value: Json | undefined) => typeof value === 'string' ? value : ''
const object = (value: Json | undefined) => value !== null && typeof value === 'object' && !Array.isArray(value)

export function McpClientBuilder({config, hints, onChange}: Props) {
  const [connections, setConnections] = useState<McpClientConnection[] | null>(null)
  const [error, setError] = useState('')
  const [fixture, setFixture] = useState(() => config.simulationResponse === undefined
    ? '' : JSON.stringify(config.simulationResponse, null, 2))
  const [invalidFixture, setInvalidFixture] = useState(false)
  const operation = config.operation === 'list' ? 'list' : 'call'
  const alias = string(config.connection)
  const connection = connections?.find(connection => connection.alias === alias)
  const tool = string(config.tool)

  useEffect(() => {
    let active = true
    api.mcpClientConnections().then(result => {
      if (active) setConnections(result.connections)
    }).catch(() => {
      if (active) setError('Could not load MCP connections.')
    })
    return () => { active = false }
  }, [])

  const argumentPaths = fieldChoices(hints).filter(path => {
    const schema = schemaAt(hints.schema, path)
    return object(inputValue(hints.sample, path))
      || (schema && typeof schema === 'object' && !Array.isArray(schema) && schema.type === 'object')
  })

  return <section className={styles.conditionBuilder} aria-label="MCP Client settings">
    <label>Connection
      <select value={alias} onChange={event => {
        const next: Record<string, Json> = {...config, connection: event.target.value}
        delete next.tool
        onChange(next)
      }}>
        <option value="" disabled>Select a connection…</option>
        {alias && !connection && <option value={alias}>{alias} (offline or unavailable)</option>}
        {connections?.map(item => <option key={item.alias} value={item.alias}>{item.alias}</option>)}
        {alias !== 'offline' && !connections?.some(item => item.alias === 'offline')
          && <option value="offline">Offline simulation</option>}
      </select>
    </label>
    {error && <p role="alert">{error}</p>}
    {connections?.length === 0 && <p className={styles.muted}>Your operator can enable a live MCP connection for this workspace.</p>}
    <label>Operation
      <select value={operation} onChange={event => {
        const next: Record<string, Json> = {...config, operation: event.target.value}
        delete next.simulationResponse
        if (event.target.value === 'list') {
          delete next.tool
          delete next.argumentsPointer
        }
        setFixture('')
        setInvalidFixture(false)
        onChange(next)
      }}>
        <option value="call">Call a tool</option>
        <option value="list">List approved tools</option>
      </select>
    </label>
    {operation === 'call' && <>
      <label>Tool
        {connection ? <select value={tool} onChange={event => onChange({...config,tool:event.target.value})}>
          <option value="" disabled>Select an approved tool…</option>
          {tool && !connection.tools.includes(tool) && <option value={tool}>{tool} (unavailable)</option>}
          {connection.tools.map(name => <option key={name} value={name}>{name}</option>)}
        </select> : <input value={tool} placeholder="example_lookup"
          onChange={event => onChange({...config,tool:event.target.value})}/>}
      </label>
      <FieldPicker label="Tool arguments" value={string(config.argumentsPointer) || '$'} choices={argumentPaths}
        rootLabel="Whole input object ($)"
        onChange={value => onChange({...config,argumentsPointer:fieldPointer(value)})}/>
    </>}
    <details>
      <summary>Offline simulation fixture</summary>
      <label>MCP result JSON
        <textarea className={styles.mono} rows={5} value={fixture} aria-invalid={invalidFixture}
          placeholder={operation === 'list' ? '{"tools":[]}' : '{"content":[],"structuredContent":{"found":true}}'}
          onChange={event => {
            const value = event.target.value
            setFixture(value)
            const next = {...config}
            if (!value.trim()) {
              delete next.simulationResponse
            } else {
              try {
                const parsed: Json = JSON.parse(value)
                if (!object(parsed)) throw new Error('Expected an object')
                next.simulationResponse = parsed
              } catch {
                setInvalidFixture(true)
                return
              }
            }
            setInvalidFixture(false)
            onChange(next)
          }}/>
      </label>
      {invalidFixture && <p role="alert">Enter a JSON object. The last valid fixture is still configured.</p>}
    </details>
  </section>
}
