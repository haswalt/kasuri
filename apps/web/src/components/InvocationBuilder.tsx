import {useEffect,useState} from 'react'
import type {Json,Workflow,WorkflowNode} from '../../../../packages/contracts/src/index'
import {api} from '../lib/api'
import type {WorkflowRevision} from '../lib/api'
import {styles} from '../design/styles'
import {FieldPicker} from './FieldPicker'
import {fieldChoices,arrayChoices,fieldPointer} from '../lib/input-hints'
import type {InputHints} from '../lib/input-hints'
type Props={node:WorkflowNode;workflow:Workflow;hints:InputHints;onChange:(config:Record<string,Json>)=>void}
export function InvocationBuilder({node,workflow,hints,onChange}:Props){
 const [workflows,setWorkflows]=useState<{id:string;name:string;revision:number}[]>([]),[revisions,setRevisions]=useState<WorkflowRevision[]>([]),[error,setError]=useState('')
 const target=typeof node.config.workflowId==='string'?node.config.workflowId:''
 useEffect(()=>{if(node.type!=='Execute Workflow'&&node.type!=='Each / Loop')return;let active=true;api.invocable().then(v=>{if(active)setWorkflows(v.workflows.filter(w=>w.id!==workflow.id))}).catch(e=>{if(active)setError(String(e))});return()=>{active=false}},[node.type,workflow.id])
 useEffect(()=>{setRevisions([]);if(!target)return;let active=true;api.revisions(target).then(v=>{if(active)setRevisions(v.revisions.filter(r=>r.graph.nodes.some(n=>n.type==='Execute Workflow Trigger')))}).catch(e=>{if(active)setError(String(e))});return()=>{active=false}},[target])
 const edit=(key:string,value:Json)=>onChange({...node.config,[key]:value})
 if(node.type==='Execute Workflow Trigger')return <section className={styles.conditionBuilder}>
  <label>Workflow output<select value={String(node.config.outputNodeId??'')} onChange={e=>edit('outputNodeId',e.target.value)}><option value="" disabled>Choose a terminal node…</option>{workflow.nodes.filter(n=>n.id!==node.id&&!workflow.edges.some(e=>e.source===n.id)).map(n=><option key={n.id} value={n.id}>{n.label}</option>)}</select></label>
  {(['inputSchema','outputSchema'] as const).map(key=><label key={key}>{key==='inputSchema'?'Input schema (optional JSON)':'Output schema (optional JSON)'}<textarea rows={5} value={typeof node.config[key]==='string'?node.config[key]:node.config[key]===undefined?'':JSON.stringify(node.config[key],null,2)} onChange={e=>{if(!e.target.value){const config={...node.config};delete config[key];onChange(config)}else edit(key,e.target.value)}} /></label>)}
  <p className={styles.muted}>Input enters unchanged. The selected terminal node supplies the result. Schemas use the offline Validate Schema profile.</p>
 </section>
 return <section className={styles.conditionBuilder}>
  {error&&<p role="alert">{error}</p>}
  <label>Workflow<select value={target} onChange={e=>{const config:Record<string,Json>={...node.config,workflowId:e.target.value};delete config.revision;onChange(config)}}><option value="" disabled>Choose a published workflow…</option>{target&&!workflows.some(w=>w.id===target)&&<option value={target}>Unavailable: {target}</option>}{workflows.map(w=><option key={w.id} value={w.id}>{w.name} · v{w.revision}</option>)}</select></label>
  <label>Revision<select value={String(node.config.revision??'latest')} onChange={e=>{const config={...node.config};if(e.target.value==='latest')delete config.revision;else config.revision=Number(e.target.value);onChange(config)}}><option value="latest">Current published revision (pinned when run starts)</option>{node.config.revision!==undefined&&!revisions.some(r=>r.revision===node.config.revision)&&<option value={String(node.config.revision)}>Unavailable revision {String(node.config.revision)}</option>}{revisions.map(r=><option key={r.revision} value={r.revision}>v{r.revision}</option>)}</select></label>
  {node.type!=='Each / Loop'&&<label>Completion<select value={String(node.config.mode??'await')} onChange={e=>edit('mode',e.target.value)}><option value="await">Wait for child result</option><option value="background">Continue with child run ID</option></select></label>}
  {node.type==='Each / Loop'?<>
   <FieldPicker label="Items array" value={typeof node.config.arrayPointer==='string'&&node.config.arrayPointer?node.config.arrayPointer:'$'} choices={arrayChoices(hints)} rootLabel="Whole input array ($)" onChange={field=>edit('arrayPointer',fieldPointer(field))} />
   <label>Item concurrency<select value={String(node.config.concurrency??1)} onChange={e=>edit('concurrency',Number(e.target.value))}><option value="1">One at a time</option><option value="2">Two items</option><option value="4">Four items</option></select></label>
   <label>Item failures<select value={String(node.config.failurePolicy??'fail')} onChange={e=>edit('failurePolicy',e.target.value)}><option value="fail">Stop and cancel remaining items</option><option value="collect">Collect errors and continue</option></select></label>
   <p className={styles.muted}>Each child receives one item unchanged. Configure retries on child workflow nodes; completed items are never repeated. Item identity/index is available in Request Context metadata.</p>
  </>:<FieldPicker label="Child input" value={typeof node.config.inputPointer==='string'&&node.config.inputPointer?node.config.inputPointer:'$'} choices={fieldChoices(hints)} onChange={field=>edit('inputPointer',field==='$'?'':field.startsWith('/')?field:'/'+field.split('.').map(s=>s.replaceAll('~','~0').replaceAll('/','~1')).join('/'))} />}
  <p className={styles.muted}>{node.type==='Each / Loop'?'Targets must be active published workflows with Execute Workflow Trigger. Simulations always process simulated children.':'Targets must have an Execute Workflow Trigger and be active. Simulations always invoke simulated children. Awaited failures follow the error output; background children continue independently.'}</p>
 </section>
}
