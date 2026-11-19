import { useId } from 'react'
import type { Json } from '../../../../packages/contracts/src/index'
import { Button } from '../design/ui'
import { styles } from '../design/styles'
import { conditionFields, conditionOperators, conditionProblem, conditionValueText, emptyCondition, numericCondition, readConditions, unaryCondition } from '../lib/conditions'
import type { Condition, ValueType } from '../lib/conditions'
type Props = { config: Record<string, Json>; sample: Json; onChange: (config: Record<string, Json>) => void }
export function ConditionBuilder({ config, sample, onChange }: Props) {
  const { rows, match, error } = readConditions(config)
  const list = useId()
  function commit(next: Condition[], mode = match) { onChange({ match: mode, conditions: next }) }
  function change(index: number, patch: Partial<Condition>) { commit(rows.map((row, i) => i === index ? { ...row, ...patch } : row)) }
  return <section className={styles.conditionBuilder} aria-label="Conditions">
    <label>Match<select value={match} onChange={e => commit(rows, e.target.value as 'all' | 'any')}><option value="all">All conditions (AND)</option><option value="any">Any condition (OR)</option></select></label>
    {error && <p role="alert">{error}</p>}
    <datalist id={list}>{conditionFields(sample).map(field => <option key={field} value={field} />)}</datalist>
    {rows.map((row, index) => <fieldset key={index} className={styles.conditionRow}>
      <legend>Condition {index + 1}</legend>
      <label>Field<input aria-label={`Condition ${index + 1} field`} list={list} value={row.field} placeholder="customer.email or /headers/x-event-id" onChange={e => change(index, { field: e.target.value })} /></label>
      <label>Operator<select aria-label={`Condition ${index + 1} operator`} value={row.operator} onChange={e => { const operator = e.target.value; change(index, { operator, ...(numericCondition(operator) ? { valueType: 'number' } : {}) }) }}>
        {!conditionOperators.some(([operator]) => operator === row.operator) && <option value={row.operator}>Unsupported: {row.operator}</option>}
        {conditionOperators.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
      </select></label>
      {!unaryCondition(row.operator) && <div className={styles.conditionValue}>
        <label>Value type<select aria-label={`Condition ${index + 1} value type`} value={row.valueType} onChange={e => { const valueType = e.target.value as ValueType; change(index, { valueType, value: valueType === 'boolean' ? true : valueType === 'null' ? null : valueType === 'json' ? '{}' : valueType === 'number' ? 0 : '' }) }}>
          <option value="string">Text</option><option value="number">Number</option><option value="boolean">Boolean</option><option value="null">Null</option><option value="json">JSON</option>
        </select></label>
        {row.valueType === 'boolean' ? <label>Value<select aria-label={`Condition ${index + 1} value`} value={String(row.value)} onChange={e => change(index, { value: e.target.value === 'true' })}><option value="true">true</option><option value="false">false</option></select></label> : row.valueType === 'null' ? <span>Matches JSON null</span> : <label>Value{row.valueType === 'json' ? <textarea aria-label={`Condition ${index + 1} value`} rows={3} value={conditionValueText(row)} onChange={e => change(index, { value: e.target.value })} /> : <input aria-label={`Condition ${index + 1} value`} inputMode={row.valueType === 'number' ? 'decimal' : undefined} value={conditionValueText(row)} onChange={e => change(index, { value: e.target.value })} />}</label>}
      </div>}
      {conditionProblem(row) && <p className={styles.muted} role="status">{conditionProblem(row)}</p>}
      <Button visual="ghost" disabled={rows.length === 1} onClick={() => commit(rows.filter((_, i) => i !== index))} aria-label={`Remove condition ${index + 1}`}>Remove condition</Button>
    </fieldset>)}
    <Button disabled={rows.length >= 32} onClick={() => commit([...rows, emptyCondition()])}>Add condition</Button>
    <p className={styles.muted}>Matching input follows true; other input follows false. Use a field path, a JSON Pointer, or $ for the whole input. Values keep their selected type.</p>
  </section>
}
