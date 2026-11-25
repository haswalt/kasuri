import { useState } from 'react'
import type { Json } from '../../../../packages/contracts/src/index'
import { styles } from '../design/styles'
import { Button } from '../design/ui'
import { fieldChoices, fieldPointer } from '../lib/input-hints'
import type { InputHints } from '../lib/input-hints'
import { FieldPicker } from './FieldPicker'

type Props = {
  config: Record<string, Json>
  hints: InputHints
  onChange: (config: Record<string, Json>) => void
}
const types = ['text', 'number', 'date', 'boolean', 'enum']
const text = (value: Json | undefined) => typeof value === 'string' ? value : ''
const fieldsFrom = (value: Json | undefined) => Array.isArray(value)
  ? value.filter((field): field is Record<string, Json> =>
    field !== null && typeof field === 'object' && !Array.isArray(field))
  : []

export function ClairicBuilder({ config, hints, onChange }: Props) {
  const fields = fieldsFrom(config.fields)
  const [fixture, setFixture] = useState(() => config.simulationResponse === undefined
    ? '' : JSON.stringify(config.simulationResponse, null, 2))
  const [invalidFixture, setInvalidFixture] = useState(false)

  const updateField = (index: number, update: Record<string, Json>) => {
    onChange({ ...config, fields: fields.map((field, i) => i === index ? update : field) })
  }

  return <section className={styles.conditionBuilder} aria-label="Clairic extraction settings">
    <FieldPicker label="Text to read" value={text(config.inputPointer) || '$'}
      choices={fieldChoices(hints)} rootLabel="Whole input text ($)"
      onChange={value => onChange({ ...config, inputPointer: fieldPointer(value) })} />
    <label>Reference date (optional)
      <input type="date" value={text(config.referenceDate)} onChange={event => {
        const next = { ...config }
        if (event.target.value) next.referenceDate = event.target.value
        else delete next.referenceDate
        onChange(next)
      }} />
    </label>
    {fields.map((field, index) => <fieldset key={index}>
      <legend>Field {index + 1}</legend>
      <label>Field name
        <input value={text(field.name)} placeholder="guest_count"
          onChange={event => updateField(index, { ...field, name: event.target.value })} />
      </label>
      <label>Field type
        <select value={text(field.type) || 'text'} onChange={event => {
          const next: Record<string, Json> = { ...field, type: event.target.value }
          if (event.target.value === 'enum') updateField(index, { ...next, enum: [] })
          else {
            delete next.enum
            updateField(index, next)
          }
        }}>
          {types.map(type => <option key={type} value={type}>{type === 'enum' ? 'Choose a label' : type}</option>)}
        </select>
      </label>
      {field.type === 'enum' && <label>Allowed labels, one per line
        <textarea rows={3} value={Array.isArray(field.enum) ? field.enum.join('\n') : ''}
          onChange={event => updateField(index, { ...field, enum: event.target.value.split('\n') })} />
      </label>}
      <label>Instructions (optional)
        <input value={text(field.instructions)} placeholder="Extract the number of expected guests"
          onChange={event => updateField(index, { ...field, instructions: event.target.value })} />
      </label>
      <Button onClick={() => onChange({ ...config, fields: fields.filter((_, i) => i !== index) })}>
        Remove field
      </Button>
    </fieldset>)}
    <Button onClick={() => onChange({ ...config, fields: [...fields, { name: '', type: 'text' }] })}>
      Add field
    </Button>
    <details>
      <summary>Offline simulation fixture</summary>
      <label>Clairic response JSON
        <textarea className={styles.mono} rows={5} value={fixture} aria-invalid={invalidFixture}
          placeholder={'{"answers":{"guest_count":{"value":80,"confidence":0.95}}}'}
          onChange={event => {
            const value = event.target.value
            setFixture(value)
            const next = { ...config }
            if (!value.trim()) {
              delete next.simulationResponse
            } else {
              try {
                const parsed: Json = JSON.parse(value)
                if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
                  throw new Error('Expected an object')
                }
                next.simulationResponse = parsed
              } catch {
                setInvalidFixture(true)
                return
              }
            }
            setInvalidFixture(false)
            onChange(next)
          }} />
      </label>
      {invalidFixture && <p role="alert">Enter a JSON object. The last valid fixture is still saved.</p>}
    </details>
    <p className={styles.muted}>Use the extracted values in IF or Switch. An undetermined field returns null.</p>
  </section>
}
