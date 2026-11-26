import { useState } from 'react'
import type { Json } from '../../../../packages/contracts/src/index'
import { Button } from '../design/ui'
import { styles } from '../design/styles'
import { decodeJson, fieldChoices, fieldPointer, type InputHints } from '../lib/input-hints'
import { FieldPicker } from './FieldPicker'

const types = ['string', 'number', 'integer', 'boolean', 'object', 'array', 'null']
const object = (value: Json | undefined): Record<string, Json> =>
  value !== null && typeof value === 'object' && !Array.isArray(value) ? value : {}
const isObject = (value: Json | undefined) => value !== null && typeof value === 'object' && !Array.isArray(value)

type Props = {
  config: Record<string, Json>
  hints: InputHints
  onChange: (config: Record<string, Json>) => void
}

export function ValidateSchemaBuilder({ config, hints, onChange }: Props) {
  const [name, setName] = useState('')
  const parsed = decodeJson(config.schema)
  const schema = object(parsed)
  const properties = object(schema.properties)
  const required = Array.isArray(schema.required) ? schema.required : []
  const raw = typeof config.schema === 'string' ? config.schema : JSON.stringify(config.schema, null, 2) ?? ''
  const update = (schema: Json) => onChange({ ...config, schema })
  const fieldName = name.trim()
  const validShape = typeof parsed === 'boolean' || isObject(parsed)

  return <section className={styles.conditionBuilder}>
    <p>Check incoming data against a schema. Use IF on <code>valid</code> to handle invalid data. The result also includes <code>errors</code> and the checked <code>value</code>.</p>
    <FieldPicker label="Data to validate" searchable value={String(config.pointer || '$')} choices={fieldChoices(hints)}
      onChange={field => onChange({ ...config, pointer: fieldPointer(field) })} />
    {schema.type === 'object' || config.schema === undefined ? <>
      <h3>Object fields</h3>
      {Object.entries(properties).map(([key, definition]) => {
        const field = object(definition)
        const type = typeof field.type === 'string' && types.includes(field.type) ? field.type : ''
        return <fieldset className={styles.conditionRow} key={key}>
          <legend>{key}</legend>
          <label>Type
            <select value={type} onChange={event => update({
              ...schema, properties: { ...properties, [key]: { ...field, type: event.target.value } }
            })}>
              <option value="" disabled>Defined in JSON</option>
              {types.map(kind => <option key={kind}>{kind}</option>)}
            </select>
          </label>
          <label><input type="checkbox" checked={required.includes(key)} onChange={event => update({
            ...schema, required: event.target.checked ? [...required, key] : required.filter(field => field !== key)
          })} /> Required</label>
          <Button visual="ghost" onClick={() => update({
            ...schema,
            properties: Object.fromEntries(Object.entries(properties).filter(([field]) => field !== key)),
            required: required.filter(field => field !== key)
          })}>Remove {key}</Button>
        </fieldset>
      })}
      <label>Field name<input value={name} placeholder="e.g. orderId" onChange={event => setName(event.target.value)} /></label>
      <Button disabled={!fieldName || Object.hasOwn(properties, fieldName)} onClick={() => {
        update({ ...schema, type: 'object', properties: { ...properties, [fieldName]: { type: 'string' } } })
        setName('')
      }}>Add schema field</Button>
    </> : <p>Use the schema JSON editor for this schema type.</p>}
    <details>
      <summary>Advanced schema JSON</summary>
      <label>JSON Schema
        <textarea value={raw} rows={8} spellCheck={false} onChange={event => update(event.target.value)} />
      </label>
      {raw && !validShape && <p role="alert">Enter a valid JSON object or Boolean schema.</p>}
    </details>
    <a href="https://github.com/haswalt/kasuri/blob/develop/docs/validation.md" target="_blank" rel="noreferrer">Supported schema rules and examples</a>
  </section>
}
