import { useEffect, useId, useState } from 'react'
import type { Json } from '../../../../packages/contracts/src/index'
import { Button } from '../design/ui'
import { styles } from '../design/styles'
import { api, type WorkspaceSecret } from '../lib/api'
import { fieldChoices, fieldPointer, type InputHints } from '../lib/input-hints'
import {
  httpFieldMap, httpFieldNameError, httpHeaderValue, httpQueryValue, httpToken,
  workspaceHeaderValue, type HttpFieldKind
} from '../lib/http-fields'
import { FieldPicker } from './FieldPicker'

const headerNames = ['Accept', 'Authorization', 'Content-Type', 'Api-Key', 'X-Api-Key', 'X-Request-Id']

function HeaderValue({ value, name, secrets, onChange }: {
  value: Json; name: string; secrets: WorkspaceSecret[]; onChange: (value: Json) => void
}) {
  const header = httpHeaderValue(value)
  const aliasList = useId()
  return <>
    <label>Value source
      <select value={header.source} onChange={event => onChange(
        event.target.value === 'workspace' ? workspaceHeaderValue('', '')
          : event.target.value === 'installation' ? { $secret: '' } : ''
      )}>
        {header.source === 'unsupported' && <option value="unsupported" disabled>Unsupported saved value</option>}
        <option value="text">Fixed text</option>
        <option value="workspace">Workspace secret</option>
        <option value="installation">Installation secret</option>
      </select>
    </label>
    {header.source === 'text' && <label>Value
      <input aria-label={`${name} value`} value={header.text} spellCheck={false} onChange={event => onChange(event.target.value)} />
    </label>}
    {header.source === 'installation' && <label>Installation secret alias
      <input value={header.alias} spellCheck={false} onChange={event => onChange({ $secret: event.target.value })} />
    </label>}
    {header.source === 'workspace' && <>
      <label>Workspace secret alias
        <input list={aliasList} value={header.alias} spellCheck={false} placeholder="api_token" maxLength={128}
          onChange={event => {
            const alias = event.target.value
            if (/^[a-z0-9_.-]*$/i.test(alias)) onChange(workspaceHeaderValue(alias, header.scheme))
          }} />
        <datalist id={aliasList}>{secrets.map(secret => <option key={secret.alias} value={secret.alias} />)}</datalist>
      </label>
      <label>Authentication scheme (optional)
        <input value={header.scheme} placeholder="Bearer" spellCheck={false} onChange={event => {
          const scheme = event.target.value
          if (!scheme || httpToken.test(scheme)) onChange(workspaceHeaderValue(header.alias, scheme))
        }} />
      </label>
    </>}
  </>
}

function QueryValue({ value, name, hints, onChange }: {
  value: Json; name: string; hints: InputHints; onChange: (value: Json) => void
}) {
  const query = httpQueryValue(value)
  const [numberDraft, setNumberDraft] = useState<string | null>(null)
  return <>
    <label>Value source
      <select value={query.source} onChange={event => {
        setNumberDraft(null)
        switch (event.target.value) {
          case 'input': return onChange({ $input: '' })
          case 'number': return onChange(0)
          case 'boolean': return onChange(true)
          case 'null': return onChange(null)
          default: return onChange('')
        }
      }}>
        {query.source === 'unsupported' && <option value="unsupported" disabled>Unsupported saved value</option>}
        <option value="text">Fixed text</option>
        <option value="number">Number</option>
        <option value="boolean">Boolean</option>
        <option value="null">Null</option>
        <option value="input">Incoming field</option>
      </select>
    </label>
    {query.source === 'input' && <FieldPicker label="Incoming field" searchable value={query.pointer || '$'}
      choices={fieldChoices(hints)} onChange={field => onChange({ $input: fieldPointer(field) })} />}
    {query.source === 'text' && <label>Value
      <input aria-label={`${name} value`} value={query.value} onChange={event => onChange(event.target.value)} />
    </label>}
    {query.source === 'number' && <label>Value
      <input aria-label={`${name} value`} type="number" step="any" value={numberDraft ?? query.value} onBlur={() => setNumberDraft(null)}
        onChange={event => {
          const next = event.target.value
          setNumberDraft(next)
          if (next !== '' && Number.isFinite(Number(next))) onChange(Number(next))
        }} />
    </label>}
    {query.source === 'boolean' && <label>Value
      <select aria-label={`${name} value`} value={String(query.value)} onChange={event => onChange(event.target.value === 'true')}>
        <option value="true">True</option><option value="false">False</option>
      </select>
    </label>}
  </>
}

export function HttpFieldsEditor({ kind, value, hints, url, onChange }: {
  kind: HttpFieldKind; value: Json | undefined; hints: InputHints; url: Json | undefined
  onChange: (value: Json) => void
}) {
  const [newName, setNewName] = useState('')
  const [rename, setRename] = useState<{ name: string; draft: string } | null>(null)
  const [secrets, setSecrets] = useState<WorkspaceSecret[]>([])
  const headerList = useId()
  const fields = httpFieldMap(value)
  const entries = fields === null ? [] : Object.entries(fields)
  const usesWorkspaceSecret = kind === 'headers' && entries.some(([, entry]) => httpHeaderValue(entry).source === 'workspace')
  useEffect(() => {
    if (!usesWorkspaceSecret) return
    let active = true
    void api.secrets().then(result => { if (active) setSecrets(result.secrets) }).catch(() => {})
    return () => { active = false }
  }, [usesWorkspaceSecret])

  const noun = kind === 'headers' ? 'header' : 'query parameter'
  const title = kind === 'headers' ? 'Custom headers' : 'Query parameters'
  if (fields === null) return <details className={styles.conditionBuilder}>
    <summary>Repair {title.toLowerCase()}</summary>
    <label>Saved {title.toLowerCase()}
      <textarea className={styles.mono} rows={4} value={typeof value === 'string' ? value : JSON.stringify(value)}
        onChange={event => onChange(event.target.value)} />
    </label>
    <p role="alert">Repair the saved object to edit it as a list.</p>
  </details>

  let origin: string | undefined
  try { if (typeof url === 'string') origin = new URL(url).origin } catch { /* The URL may still be a draft. */ }
  const suggestions = origin ? secrets.filter(secret => secret.origin === origin) : []
  const problem = httpFieldNameError(kind, fields, newName.trim())
  function add() {
    if (problem) return
    onChange({ ...fields, [newName.trim()]: '' })
    setNewName('')
  }

  return <section aria-label={title} className={styles.conditionBuilder}>
    <h3>{title}</h3>
    {entries.map(([name, entry]) => {
      const draft = rename?.name === name ? rename.draft : name
      const nameError = httpFieldNameError(kind, fields, draft.trim(), name)
      const editValue = (next: Json) => onChange({ ...fields, [name]: next })
      function commitName() {
        if (nameError) return
        setRename(null)
        if (draft.trim() !== name) onChange(Object.fromEntries(entries.map(([key, entry]) => [key === name ? draft.trim() : key, entry])))
      }
      return <fieldset key={name} className={styles.conditionRow}>
        <legend>{name}</legend>
        <label>Name
          <input value={draft} aria-invalid={Boolean(nameError)} onChange={event => setRename({ name, draft: event.target.value })}
            onBlur={commitName} onKeyDown={event => {
              if (event.key === 'Enter') { event.preventDefault(); commitName() }
              if (event.key === 'Escape') setRename(null)
            }} />
        </label>
        {nameError && <p role="alert">{nameError}</p>}
        {kind === 'headers'
          ? <HeaderValue name={name} value={entry} secrets={suggestions} onChange={editValue} />
          : <QueryValue name={name} value={entry} hints={hints} onChange={editValue} />}
        <Button visual="ghost" aria-label={`Remove ${name}`} onClick={() => onChange(Object.fromEntries(entries.filter(([key]) => key !== name)))}>Remove</Button>
      </fieldset>
    })}
    <label>New {noun} name
      <input value={newName} list={kind === 'headers' ? headerList : undefined} placeholder={kind === 'headers' ? 'Authorization' : 'page'}
        aria-invalid={Boolean(newName && problem)} onChange={event => setNewName(event.target.value)}
        onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); add() } }} />
    </label>
    {kind === 'headers' && <datalist id={headerList}>{headerNames.map(name => <option key={name} value={name} />)}</datalist>}
    {newName && problem && <p role="alert">{problem}</p>}
    <Button disabled={Boolean(problem)} onClick={add}>Add {noun}</Button>
  </section>
}
