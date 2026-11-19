import { useState } from 'react'
import type { Json } from '../../../../packages/contracts/src/index'
import { conditionValueText } from '../lib/conditions'
import type { Condition, ValueType } from '../lib/conditions'
type Props={row:Condition;index:number;choices:Json[];source:'schema'|'sample';onChange:(value:Json,valueType?:ValueType)=>void}
export function ConditionValue({row,index,choices,source,onChange}:Props) {
 const [custom,setCustom]=useState(false)
 if(row.valueType==='boolean')return <label>Value<select aria-label={`Condition ${index+1} value`} value={String(row.value)} onChange={e=>onChange(e.target.value==='true')}><option value="true">true</option><option value="false">false</option></select></label>
 if(row.valueType==='null')return <span>Matches JSON null</span>
 let parsed=row.value;try{if(row.valueType==='number'||row.valueType==='json')parsed=typeof row.value==='string'?JSON.parse(row.value):row.value}catch{/* Preserve incomplete typed input. */}
 const key=JSON.stringify(parsed),known=choices.some(v=>JSON.stringify(v)===key)
 const showCustom=custom||(!known&&row.value!=='')||choices.length===0
 const free=row.valueType==='json'?<textarea aria-label={`Condition ${index+1} custom value`} rows={3} value={conditionValueText(row)} onChange={e=>onChange(e.target.value)} />:<input aria-label={`Condition ${index+1} custom value`} inputMode={row.valueType==='number'?'decimal':undefined} value={conditionValueText(row)} onChange={e=>onChange(e.target.value)} />
 return <div>
  {choices.length>0&&<label>{source==='schema'?'Allowed value':'Sample value'}<select aria-label={`Condition ${index+1} value`} value={showCustom?'custom':known?'value:'+key:''} onChange={e=>{if(e.target.value==='custom'){setCustom(true);return}setCustom(false);const value=JSON.parse(e.target.value.slice(6)) as Json;const kind:ValueType=value===null?'null':typeof value==='string'?'string':typeof value==='number'?'number':typeof value==='boolean'?'boolean':'json';onChange(value,kind)}}>
    <option value="" disabled>Choose a value…</option>{choices.map(value=><option key={JSON.stringify(value)} value={'value:'+JSON.stringify(value)}>{typeof value==='string'?value:JSON.stringify(value)}</option>)}<option value="custom">Custom value…</option>
  </select></label>}
  {showCustom&&<label>{choices.length?'Custom value':'Value'}{free}</label>}
 </div>
}
