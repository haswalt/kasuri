import type { Json } from '../../../../packages/contracts/src/index'
import { ConditionBuilder } from './ConditionBuilder'
import { FieldPicker } from './FieldPicker'
import { styles } from '../design/styles'
import { arrayChoices, decodeJson, fieldChoices, fieldPointer, inputValue, schemaAt } from '../lib/input-hints'
import type { InputHints } from '../lib/input-hints'
type Props={kind:'Filter'|'Sort'|'Split / Batch'|'Aggregate';config:Record<string,Json>;hints:InputHints;onChange:(config:Record<string,Json>)=>void}
export function CollectionBuilder({kind,config,hints,onChange}:Props) {
 const pointer=typeof config.arrayPointer==='string'?config.arrayPointer:''
 const value=inputValue(hints.sample,pointer)
 const items=Array.isArray(value)?value.slice(0,20):[]
 const arraySchema=schemaAt(hints.schema,pointer)
 const schema=arraySchema&&typeof arraySchema==='object'&&!Array.isArray(arraySchema)?arraySchema.items:undefined
 const sample=items[0]??null
 const fields=[...new Set([...fieldChoices({sample,schema}),...items.flatMap(sample=>fieldChoices({sample}))])].slice(0,100)
 if(kind==='Split / Batch'||kind==='Aggregate')return <section className={styles.conditionBuilder}>
  <FieldPicker label="Input array" value={pointer||'$'} choices={arrayChoices(hints)} rootLabel="Whole input array ($)" onChange={field=>onChange({...config,arrayPointer:fieldPointer(field)})} />
  {kind==='Split / Batch'?<label>Items per batch<input type="number" min={1} max={100} step={1} value={typeof config.batchSize==='number'||typeof config.batchSize==='string'?config.batchSize:10} onChange={e=>onChange({...config,batchSize:e.target.value===''?'':Number(e.target.value)})} /></label>:<>
    <label>Operation<select value={String(config.operation??'collect')} onChange={e=>onChange({...config,operation:e.target.value})}><option value="collect">Collect selected values</option><option value="count">Count items</option><option value="sum">Sum numbers</option></select></label>
    {config.operation!=='count'&&<><FieldPicker label="Item value" value={typeof config.pointer==='string'&&config.pointer?config.pointer:'$'} choices={fields} rootLabel="Whole item ($)" onChange={field=>onChange({...config,pointer:fieldPointer(field)})} /><label>Missing item values<select value={String(config.missing??'fail')} onChange={e=>onChange({...config,missing:e.target.value})}><option value="fail">Fail with a missing-input error</option><option value="skip">Skip missing values</option></select></label></>}
  </>}
  <p className={styles.muted}>{kind==='Split / Batch'?'Batches remain explicit arrays. Use Each / Loop to process the batches.':'Values stay in input order. Sum preserves integer precision and rejects non-numeric values.'}</p>
 </section>
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
