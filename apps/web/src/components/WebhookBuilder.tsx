import { useState } from 'react'
import type { Json } from '../../../../packages/contracts/src/index'
import { Button } from '../design/ui'
import { styles } from '../design/styles'
import { decodeJson } from '../lib/input-hints'

const defaults = ['content-type', 'x-kasuri-event-id']
const validName = /^[a-z0-9_-]{1,64}$/i
const authenticationOptions = [
  ['none', 'Public · no authentication'], ['bearer', 'Bearer token'],
  ['query', 'Token in a query parameter'], ['hmac_sha256', 'HMAC signature'],
  ['hmac', 'HMAC signature with event ID (legacy)']
]

function CapturedNames({ label, singular, value, onChange, headers = false }: {
  label: string
  singular: string
  value: Json | undefined
  onChange: (names: string[]) => void
  headers?: boolean
}) {
  const [draft, setDraft] = useState('')
  const parsed = value === undefined ? (headers ? defaults : []) : decodeJson(value)
  const names = Array.isArray(parsed) ? parsed.filter((name): name is string => typeof name === 'string') : []
  const name = headers ? draft.trim().toLowerCase() : draft.trim()
  const duplicate = names.some(existing => (headers ? existing.toLowerCase() : existing) === name)
  const canAdd = validName.test(name) && !duplicate && names.length < 32
  function add() {
    if (!canAdd) return
    onChange([...names, name])
    setDraft('')
  }

  return <fieldset className={styles.conditionRow}>
    <legend>{label}</legend>
    {value !== undefined && (!Array.isArray(parsed) || names.length !== parsed.length) && (
      <p role="alert">The saved list is invalid. Add a name to replace it.</p>
    )}
    {!names.length && <p>None selected.</p>}
    {names.map((entry, index) => <div className={styles.metadataRow} key={index}>
      <code>{entry}</code>
      <Button type="button" visual="ghost" aria-label={`Remove ${entry}`}
        onClick={() => onChange(names.filter((_, position) => position !== index))}>Remove</Button>
    </div>)}
    <label>{singular} name
      <input maxLength={64} value={draft} placeholder={headers ? 'idempotency-key' : 'topic'}
        onChange={event => setDraft(event.target.value)} onKeyDown={event => {
          if (event.key === 'Enter') { event.preventDefault(); add() }
        }} />
    </label>
    {name && !validName.test(name) && <p>Use letters, numbers, hyphens or underscores.</p>}
    {duplicate && <p>Already selected.</p>}
    <Button type="button" disabled={!canAdd} onClick={add}>Add {singular.toLowerCase()}</Button>
  </fieldset>
}

export function WebhookBuilder({ config, onChange }: {
  config: Record<string, Json>
  onChange: (config: Record<string, Json>) => void
}) {
  const auth = typeof config.authentication === 'string' ? config.authentication : 'bearer'
  return <section className={styles.conditionBuilder}>
    <label>Authentication<select value={auth} onChange={event => onChange({ ...config, authentication: event.target.value })}>
      {!authenticationOptions.some(([mode]) => mode === auth) && <option value={auth}>Unsupported: {auth}</option>}
      {authenticationOptions.map(([mode, label]) => <option key={mode} value={mode}>{label}</option>)}
    </select></label>
    {auth === 'query' && <label>Token query parameter<input maxLength={64} placeholder="key"
      value={typeof config.queryParameter === 'string' ? config.queryParameter : ''}
      onChange={event => {
        const next = { ...config }
        if (event.target.value) next.queryParameter = event.target.value
        else delete next.queryParameter
        onChange(next)
      }} /></label>}
    {(auth === 'hmac' || auth === 'hmac_sha256') && <label>Signing secret name
      <input value={typeof config.signingSecret === 'string' ? config.signingSecret : ''}
        placeholder="incoming_webhook_key" onChange={event => onChange({ ...config, signingSecret: event.target.value })} />
      <small>Use the name configured in the Worker’s HTTP_SECRETS setting.</small>
    </label>}
    <details open={config.contextHeaders !== undefined || config.contextQuery !== undefined || undefined}>
      <summary>Request headers and query parameters</summary>
      <div className={styles.conditionBuilder}>
        <p>Use a Request Context node to read these values. Authentication credentials are excluded.</p>
        <CapturedNames label="Recorded headers" singular="Header" headers value={config.contextHeaders}
          onChange={contextHeaders => onChange({ ...config, contextHeaders })} />
        <CapturedNames label="Recorded query parameters" singular="Query parameter" value={config.contextQuery}
          onChange={contextQuery => onChange({ ...config, contextQuery })} />
      </div>
    </details>
  </section>
}
