import type { Json } from '../../../../packages/contracts/src/index'
export const conditionOperators = [
  ['equals', 'Equals'], ['not_equals', 'Does not equal'], ['exists', 'Exists'], ['not_exists', 'Does not exist'],
  ['is_empty', 'Is empty'], ['is_not_empty', 'Is not empty'], ['contains', 'Contains'], ['not_contains', 'Does not contain'],
  ['starts_with', 'Starts with'], ['ends_with', 'Ends with'], ['greater_than', 'Greater than'], ['greater_or_equal', 'Greater than or equal'],
  ['less_than', 'Less than'], ['less_or_equal', 'Less than or equal']
] as const
export type ValueType = 'string' | 'number' | 'boolean' | 'null' | 'json'
export type Condition = { field: string; operator: string; valueType: ValueType; value: Json }
export const unaryCondition = (operator: string) => ['exists','not_exists','is_empty','is_not_empty'].includes(operator)
export const numericCondition = (operator: string) => ['greater_than','greater_or_equal','less_than','less_or_equal'].includes(operator)
export const emptyCondition = (): Condition => ({ field: '', operator: 'equals', valueType: 'string', value: '' })
function infer(value: Json): ValueType { return value === null ? 'null' : typeof value === 'string' ? 'string' : typeof value === 'number' ? 'number' : typeof value === 'boolean' ? 'boolean' : 'json' }
export function readConditions(config: Record<string, Json>): { rows: Condition[]; match: 'all' | 'any'; error?: string } {
  try {
    if (config.conditions === undefined) return { rows: config.pointer !== undefined ? [{ field: String(config.pointer) || '$', operator: 'equals', value: config.equals ?? null, valueType: infer(config.equals ?? null) }] : [emptyCondition()], match: 'all' }
    const rows = typeof config.conditions === 'string' ? JSON.parse(config.conditions) : config.conditions
    if (!Array.isArray(rows) || !rows.length || rows.length > 32 || rows.some(row => !row || typeof row.field !== 'string' || typeof row.operator !== 'string')) throw Error()
    return { rows: rows.map(row => ({ field: row.field, operator: row.operator, value: row.value ?? null, valueType: row.valueType || infer(row.value ?? null) })), match: config.match === 'any' ? 'any' : 'all' }
  } catch { return { rows: [emptyCondition()], match: 'all', error: 'This configuration is invalid. Editing these rows will replace it with structured conditions.' } }
}
export function conditionValueText(row: Condition): string { return typeof row.value === 'string' ? row.value : JSON.stringify(row.value, null, row.valueType === 'json' ? 2 : undefined) }
export function conditionProblem(row: Condition): string | undefined {
  if (!row.field.trim()) return 'Choose a field.'
  if (unaryCondition(row.operator)) return
  if (numericCondition(row.operator) && row.valueType !== 'number') return 'This comparison needs a number.'
  if (row.valueType === 'number' || row.valueType === 'json') {
    try { const value = typeof row.value === 'string' ? JSON.parse(row.value) : row.value; if (row.valueType === 'number' && (typeof value !== 'number' || !Number.isFinite(value))) throw Error() }
    catch { return row.valueType === 'number' ? 'Enter a valid number.' : 'Enter valid JSON.' }
  }
}
export function conditionFields(input: Json, pointer = '', depth = 0): string[] {
  if (!input || typeof input !== 'object' || depth > 3) return []
  return Object.entries(input).slice(0, 30).flatMap(([key, value]) => { const path = `${pointer}/${key.replaceAll('~','~0').replaceAll('/','~1')}`; return [path,...conditionFields(value, path, depth + 1)] }).slice(0,100)
}
