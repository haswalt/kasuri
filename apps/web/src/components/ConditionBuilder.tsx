import type { Json } from '../../../../packages/contracts/src/index'
import { Button } from '../design/ui'
import { styles } from '../design/styles'
import { FieldPicker } from './FieldPicker'
import { ConditionValue } from './ConditionValue'
import { fieldChoices, inputValue, schemaAt, valueChoices } from '../lib/input-hints'
import type { InputHints } from '../lib/input-hints'
import { conditionOperators, conditionProblem, emptyCondition, numericCondition, readConditions, unaryCondition } from '../lib/conditions'
import type { Condition, ValueType } from '../lib/conditions'
type Props = { config: Record<string, Json>; sample: Json; schema?: Json; samples?: Json[]; operators?: typeof conditionOperators[number][]; hideMatch?:boolean;resultHint?:string; maxRows?:number; onChange: (config: Record<string, Json>) => void }
export function ConditionBuilder({ config, sample, schema, samples = [sample], operators = [...conditionOperators], hideMatch=false, maxRows=32, resultHint, onChange }: Props) {
  const { rows, match, error } = readConditions(config)
  const hints: InputHints = {sample,schema}
  const fields = [...new Set([...fieldChoices(hints),...samples.flatMap(sample=>fieldChoices({sample}))])].slice(0,100)
  function fieldChange(index:number, field:string) {
    const row=rows[index];const actual=inputValue(sample,field);const definition=schemaAt(schema,field)
    const type=definition && typeof definition==='object' && !Array.isArray(definition) ? definition.type : undefined
    const kind:ValueType|undefined = type==='boolean'||typeof actual==='boolean'?'boolean':type==='number'||type==='integer'||typeof actual==='number'?'number':type==='string'||typeof actual==='string'?'string':undefined
    const value=kind==='boolean' && row.valueType!=='boolean'?true:row.value
    change(index,{field,...(kind?{valueType:kind,value}:{})})
  }
  function choices(row:Condition) {
    if(['equals','not_equals'].includes(row.operator)) return valueChoices(hints,row.field,samples)
    if(['contains','not_contains'].includes(row.operator)) {
      const values=samples.flatMap(sample=>{const value=inputValue(sample,row.field);return Array.isArray(value)?value:[]})
      const definition=schemaAt(schema,row.field)
      const items=definition && typeof definition==='object' && !Array.isArray(definition)?definition.items:undefined
      return valueChoices({sample:values[0]??null,schema:items},'$',values)
    }
    return {values:[],source:'sample' as const}
  }
  function commit(next: Condition[], mode = match) { onChange({ match: mode, conditions: next }) }
  function change(index: number, patch: Partial<Condition>) { commit(rows.map((row, i) => i === index ? { ...row, ...patch } : row)) }
  return <section className={styles.conditionBuilder} aria-label="Conditions">
    {!hideMatch&&<label>Match<select value={match} onChange={e => commit(rows, e.target.value as 'all' | 'any')}><option value="all">All conditions (AND)</option><option value="any">Any condition (OR)</option></select></label>}
    {error && <p role="alert">{error}</p>}
    {rows.map((row, index) => <fieldset key={index} className={styles.conditionRow}>
      <legend>Condition {index + 1}</legend>
      <FieldPicker label="Field" ariaLabel={`Condition ${index+1} field`} value={row.field} choices={fields} onChange={field=>fieldChange(index,field)} />
      <label>Operator<select aria-label={`Condition ${index + 1} operator`} value={row.operator} onChange={e => { const operator = e.target.value; change(index, { operator, ...(numericCondition(operator) ? { valueType: 'number' } : {}) }) }}>
        {!operators.some(([operator]) => operator === row.operator) && <option value={row.operator}>Unsupported: {row.operator}</option>}
        {operators.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
      </select></label>
      {!unaryCondition(row.operator) && <div className={styles.conditionValue}>
        <label>Value type<select aria-label={`Condition ${index + 1} value type`} value={row.valueType} onChange={e => { const valueType = e.target.value as ValueType; change(index, { valueType, value: valueType === 'boolean' ? true : valueType === 'null' ? null : valueType === 'json' ? '{}' : valueType === 'number' ? 0 : '' }) }}>
          {numericCondition(row.operator) ? <>{row.valueType!=='number'&&<option value={row.valueType}>Choose Number</option>}<option value="number">Number</option></> : <><option value="string">Text</option><option value="number">Number</option><option value="boolean">Boolean</option><option value="null">Null</option><option value="json">JSON</option></>}
        </select></label>
        <ConditionValue row={row} index={index} choices={choices(row).values} source={choices(row).source} onChange={(value,valueType)=>change(index,{value,...(valueType?{valueType}:{})})} />
      </div>}
      {conditionProblem(row) && <p className={styles.muted} role="status">{conditionProblem(row)}</p>}
      <Button visual="ghost" disabled={rows.length === 1} onClick={() => commit(rows.filter((_, i) => i !== index))} aria-label={`Remove condition ${index + 1}`}>Remove condition</Button>
    </fieldset>)}
    <Button disabled={rows.length >= maxRows} onClick={() => commit([...rows, emptyCondition()])}>Add condition</Button>
    <p className={styles.muted}>{resultHint??(hideMatch?"All filters must match. Choose fields from the schema or loaded records; custom paths remain available.":"Matching input follows true; other input follows false. Use a field path, a JSON Pointer, or $ for the whole input. Values keep their selected type.")}</p>
  </section>
}
