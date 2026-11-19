import type { Json } from '../../../../packages/contracts/src/index'
import { ConditionBuilder } from './ConditionBuilder'
import { FieldPicker } from './FieldPicker'
import { styles } from '../design/styles'
import { arrayChoices, decodeJson, fieldChoices, fieldPointer, inputValue, schemaAt } from '../lib/input-hints'
import type { InputHints } from '../lib/input-hints'
type Props={kind:'Filter'|'Sort';config:Record<string,Json>;hints:InputHints;onChange:(config:Record<string,Json>)=>void}
export function CollectionBuilder({kind,config,hints,onChange}:Props) {
 const pointer=typeof config.arrayPointer==='string'?config.arrayPointer:''
 const value=inputValue(hints.sample,pointer)
 const items=Array.isArray(value)?value.slice(0,20):[]
 const arraySchema=schemaAt(hints.schema,pointer)
 const schema=arraySchema&&typeof arraySchema==='object'&&!Array.isArray(arraySchema)?arraySchema.items:undefined
 const sample=items[0]??null
 const fields=[...new Set([...fieldChoices({sample,schema}),...items.flatMap(sample=>fieldChoices({sample}))])].slice(0,100)
 const decodedEquals=decodeJson(config.equals)
 const conditions:Record<string,Json>=config.conditions!==undefined?{conditions:config.conditions,...(config.match!==undefined?{match:config.match}:{})}:config.pointer!==undefined?{pointer:config.pointer,equals:decodedEquals===undefined?config.equals??null:decodedEquals}:{}
 return <section className={styles.conditionBuilder} aria-label={kind+' inputs'}>
  <FieldPicker label={kind==='Filter'?'Array to filter':'Array to sort'} value={pointer||'$'} choices={arrayChoices(hints)} rootLabel="Whole input array ($)" onChange={field=>onChange({...config,arrayPointer:fieldPointer(field)})} />
  {kind==='Filter'?<ConditionBuilder config={conditions} sample={sample} schema={schema} samples={items} onChange={conditions=>onChange({arrayPointer:pointer,...conditions})} />:<>
   <FieldPicker label="Sort field" value={typeof config.pointer==='string'&&config.pointer?config.pointer:'$'} choices={fields} rootLabel="Whole item ($)" onChange={field=>onChange({...config,pointer:fieldPointer(field)})} />
   <label>Direction<select value={typeof config.order==='string'?config.order:'asc'} onChange={e=>onChange({...config,order:e.target.value})}><option value="asc">Ascending</option><option value="desc">Descending</option></select></label>
  </>}
  <p className={styles.muted}>Fields come from the input schema or sample data. Custom paths remain available for dynamic inputs.</p>
 </section>
}
