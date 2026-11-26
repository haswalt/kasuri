import { useState } from 'react'
import type { Json } from '../../../../packages/contracts/src/index'
import { Button } from '../design/ui'
import { styles } from '../design/styles'
import { decodeJson, fieldChoices, fieldPointer, type InputHints } from '../lib/input-hints'
import { FieldPicker } from './FieldPicker'
import { FixedValue } from './ExpressionBuilder'

type Props = {
  config: Record<string, Json>
  hints: InputHints
  target?: 'record'
  onChange: (config: Record<string, Json>) => void
}

export function TransformBuilder({ config, hints, target, onChange }: Props) {
  const [name, setName] = useState('')
  const decoded = decodeJson(config.mapping)
  const mapping = decoded && typeof decoded === 'object' && !Array.isArray(decoded) ? decoded : {}
  const entries = Object.entries(mapping)
  const update = (mapping: Record<string, Json>) => onChange({ ...config, mapping })
  const fieldName = name.trim()

  return <section className={styles.conditionBuilder}>
    {target !== 'record' && <p>Build a new object for the next node. Copy input fields to rename them, or add fixed values. Only the fields listed below are included.</p>}
    {config.mapping !== undefined && decoded !== mapping && <p role="alert">The saved mapping is not a JSON object. Add a field to replace it.</p>}
    {entries.map(([key, value]) => {
      const pointer = value && typeof value === 'object' && !Array.isArray(value) &&
        Object.keys(value).length === 1 && typeof value.$input === 'string' ? value.$input : undefined
      return <fieldset className={styles.conditionRow} key={key}>
        <legend>{key}</legend>
        <label>Value source
          <select value={pointer === undefined ? 'fixed' : 'input'}
            onChange={event => update({ ...mapping, [key]: event.target.value === 'input' ? { $input: '' } : '' })}>
            <option value="input">Incoming data</option>
            <option value="fixed">Fixed value</option>
          </select>
        </label>
        {pointer === undefined
          ? <FixedValue value={value} onChange={next => update({ ...mapping, [key]: next })} />
          : <FieldPicker label="Source field" searchable value={pointer || '$'} choices={fieldChoices(hints)}
              onChange={field => update({ ...mapping, [key]: { $input: fieldPointer(field) } })} />}
        <Button visual="ghost" onClick={() => update(Object.fromEntries(entries.filter(([field]) => field !== key)))}>Remove {key}</Button>
      </fieldset>
    })}
    <label>{target === 'record' ? 'Record field name' : 'Output field name'}<input value={name} placeholder="e.g. customerName" onChange={event => setName(event.target.value)} /></label>
    <Button disabled={!fieldName || Object.hasOwn(mapping, fieldName)} onClick={() => {
      update({ ...mapping, [fieldName]: { $input: '' } })
      setName('')
    }}>{target === 'record' ? 'Add record field' : 'Add output field'}</Button>
  </section>
}
