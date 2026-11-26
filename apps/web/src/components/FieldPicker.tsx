import { useId, useState } from 'react'
import { fieldPointer } from '../lib/input-hints'
type Props={label:string;ariaLabel?:string;value:string;choices:string[];onChange:(value:string)=>void;rootLabel?:string;searchable?:boolean}
export function FieldPicker({label,ariaLabel=label,value,choices,onChange,rootLabel='Whole input ($)',searchable=false}:Props) {
 const [custom,setCustom]=useState(false)
 const [draft,setDraft]=useState<string|null>(null)
 const listId=useId()
 if (searchable) return <label>{label}
   <input aria-label={ariaLabel} list={listId} value={draft??value} placeholder="Type a field path or choose a suggestion"
     onChange={event=>{setDraft(event.target.value);onChange(event.target.value)}} onBlur={()=>setDraft(null)} />
   <datalist id={listId}><option value="$" label={rootLabel} />
     {choices.map(path=><option key={path} value={path} />)}
   </datalist>
 </label>
 const known=choices.find(path=>fieldPointer(path)===fieldPointer(value))
 const isRoot=value==='$'
 const showCustom=custom || (!known&&!isRoot&&value!=='') || choices.length===0&&!isRoot
 const selected=showCustom?'custom':isRoot?'root':known?'field:'+known:''
 return <div>
  <label>{label}<select aria-label={ariaLabel} value={selected} onChange={e=>{const choice=e.target.value;if(choice==='custom'){setCustom(true)}else{setCustom(false);onChange(choice==='root'?'$':choice.slice(6))}}}>
   <option value="" disabled>Choose a field…</option><option value="root">{rootLabel}</option>
   {choices.map(path=><option key={path} value={'field:'+path}>{path}</option>)}<option value="custom">Custom path…</option>
  </select></label>
  {showCustom&&<label>Custom field path<input aria-label={ariaLabel+' custom path'} value={value} placeholder="customer.email or /headers/x-event-id" onChange={e=>onChange(e.target.value)} /></label>}
 </div>
}
