import type { DataField } from '../../../../packages/contracts/src/index'
import { Button } from '../design/ui'
import { styles } from '../design/styles'

const kinds: [DataField['kind'], string][] = [
  ['string', 'Text'], ['number', 'Number'], ['boolean', 'Boolean'],
  ['object', 'Object'], ['array', 'Array'], ['any', 'Any JSON value']
]

export function DataFieldsEditor({ fields, onChange }: {
  fields: DataField[]
  onChange: (fields: DataField[]) => void
}) {
  function edit(index: number, patch: Partial<DataField>) {
    onChange(fields.map((field, position) => position === index ? { ...field, ...patch } : field))
  }

  return <section className={styles.conditionBuilder} aria-label="Table fields">
    <h3>Table fields</h3>
    {!fields.length && <p>Add fields to validate records. An empty list allows any object fields.</p>}
    {fields.map((field, index) => <fieldset className={styles.conditionRow} key={index}>
      <legend>Field {index + 1}</legend>
      <label>Name<input aria-label={`Field ${index + 1} name`} value={field.name} maxLength={64}
        pattern="[A-Za-z0-9_\\-]+" onChange={event => edit(index, { name: event.target.value })} /></label>
      <label>Type<select aria-label={`Field ${index + 1} type`} value={field.kind}
        onChange={event => edit(index, { kind: event.target.value as DataField['kind'] })}>
        {kinds.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
      </select></label>
      <label><input type="checkbox" checked={Boolean(field.required)}
        onChange={event => edit(index, { required: event.target.checked })} /> Required</label>
      <Button type="button" visual="ghost" onClick={() => onChange(fields.filter((_, position) => position !== index))}>
        Remove field {index + 1}
      </Button>
    </fieldset>)}
    <Button type="button" disabled={fields.length >= 100}
      onClick={() => onChange([...fields, { name: '', kind: 'string', required: false }])}>Add table field</Button>
  </section>
}
