import type { Json,Workflow } from '../../../../packages/contracts/src/index'
import type { InputHints } from '../lib/input-hints'
import { ExpressionBuilder } from './ExpressionBuilder'
import { styles } from '../design/styles'
type Props={workflow:Workflow;nodeId:string;bindings:Record<string,Json>;hints:InputHints;nodeHints:(id:string)=>InputHints;triggerHints:InputHints;onChange:(bindings:Record<string,Json>)=>void}
export function InputBindingsBuilder({workflow,nodeId,bindings,hints,nodeHints,triggerHints,onChange}:Props){const enabled=Object.keys(bindings).length>0
 return <details className={styles.conditionBuilder} open={enabled||undefined}><summary>Input assembly</summary><label>Input mode<select value={enabled?'assemble':'incoming'} onChange={e=>onChange(e.target.value==='assemble'?{value:{op:'input',path:''}}:{})}><option value="incoming">Use normal incoming data</option><option value="assemble">Assemble fields from expressions</option></select></label>{enabled&&<><p>The assembled object replaces this node’s input. Configure the node to read these fields; use a whole-input reference to send the entire object. Format v2 is required.</p><ExpressionBuilder config={{fields:bindings}} workflow={workflow} nodeId={nodeId} hints={hints} nodeHints={nodeHints} triggerHints={triggerHints} onChange={config=>onChange(config.fields as Record<string,Json>)}/></>}</details>
}
