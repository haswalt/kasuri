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
 const each = node.type === 'Each / Loop'
 const target=typeof node.config.workflowId==='string'?node.config.workflowId:''
 useEffect(()=>{if(node.type!=='Execute Workflow'&&node.type!=='Each / Loop')return;let active=true;api.invocable().then(v=>{if(active)setWorkflows(v.workflows.filter(w=>w.id!==workflow.id))}).catch(e=>{if(active)setError(String(e))});return()=>{active=false}},[node.type,workflow.id])
 useEffect(()=>{setRevisions([]);if(!target)return;let active=true;api.revisions(target).then(v=>{if(active)setRevisions(v.revisions.filter(r=>r.graph.nodes.some(n=>n.type==='Execute Workflow Trigger')))}).catch(e=>{if(active)setError(String(e))});return()=>{active=false}},[target])
 const edit=(key:string,value:Json)=>onChange({...node.config,[key]:value})
 if(node.type==='Execute Workflow Trigger')return <section className={styles.conditionBuilder}>
  <p>This trigger lets other workflows call this one. Connect a final step, then select it below to return its output to the caller.</p>
  <label>Return output from<select value={String(node.config.outputNodeId??'')} onChange={e=>edit('outputNodeId',e.target.value)}><option value="" disabled>Choose a terminal node…</option>{workflow.nodes.filter(n=>n.id!==node.id&&!workflow.edges.some(e=>e.source===n.id)).map(n=><option key={n.id} value={n.id}>{n.label}</option>)}</select></label>
  <details open={node.config.inputSchema !== undefined || node.config.outputSchema !== undefined || undefined}>
  <summary>Optional input and output contracts</summary>
  {(['inputSchema','outputSchema'] as const).map(key=><label key={key}>{key==='inputSchema'?'Input schema (optional JSON)':'Output schema (optional JSON)'}<textarea rows={5} value={typeof node.config[key]==='string'?node.config[key]:node.config[key]===undefined?'':JSON.stringify(node.config[key],null,2)} onChange={e=>{if(!e.target.value){const config={...node.config};delete config[key];onChange(config)}else edit(key,e.target.value)}} /></label>)}
  </details>
  <p className={styles.muted}>Input enters unchanged. The selected terminal node supplies the result. Schemas use the offline Validate Schema profile.</p>
 </section>
 return <section className={styles.conditionBuilder}>
  <p>{each ? 'Run a reusable workflow once for each item. The child receives one item as its input; the results are returned in input order.' : 'Call a reusable workflow with data from this step.'}</p>
  {error&&<p role="alert">{error}</p>}
  {each && <FieldPicker label="Items array" searchable value={String(node.config.arrayPointer || '$')}
    choices={arrayChoices(hints)} rootLabel="Whole input array ($)" onChange={field => edit('arrayPointer', fieldPointer(field))} />}
  <label>{each ? 'Workflow to run for each item (required)' : 'Workflow to call (required)'}
    <select value={target} onChange={e=>{
      const config:Record<string,Json>={...node.config,workflowId:e.target.value}
      delete config.revision
      onChange(config)
    }}>
      <option value="" disabled>Choose a published workflow…</option>
      {target&&!workflows.some(w=>w.id===target)&&<option value={target}>Unavailable: {target}</option>}
      {workflows.map(w=><option key={w.id} value={w.id}>{w.name} · v{w.revision}</option>)}
    </select>
  </label>
  <p className={styles.muted}>In a separate workflow, add Execute Workflow Trigger, select the step that returns a result, then publish it. Active workflows in this workspace appear here.</p>
  {!each && <FieldPicker label="Child input" searchable value={String(node.config.inputPointer || '$')} choices={fieldChoices(hints)}
    onChange={field => edit('inputPointer', fieldPointer(field))} />}
  {!each && <label>Completion
    <select value={String(node.config.mode??'await')} onChange={e=>edit('mode',e.target.value)}>
      <option value="await">Wait for child result</option>
      <option value="background">Continue with child run ID</option>
    </select>
  </label>}
  <details open={node.config.revision !== undefined || node.config.concurrency !== undefined || node.config.failurePolicy !== undefined || undefined}>
    <summary>Advanced execution options</summary>
    <label>Child revision<select value={String(node.config.revision??'latest')} onChange={e=>{
      const config={...node.config}
      if(e.target.value==='latest')delete config.revision
      else config.revision=Number(e.target.value)
      onChange(config)
    }}>
      <option value="latest">Current published revision</option>
      {node.config.revision!==undefined&&!revisions.some(r=>r.revision===node.config.revision)&&<option value={String(node.config.revision)}>Unavailable revision {String(node.config.revision)}</option>}
      {revisions.map(r=><option key={r.revision} value={r.revision}>v{r.revision}</option>)}
    </select></label>
    {each && <>
      <label>Items at a time<select value={String(node.config.concurrency??1)} onChange={e=>edit('concurrency',Number(e.target.value))}>
        <option value="1">One item</option><option value="2">Two items</option><option value="4">Four items</option>
      </select></label>
      <label>When an item fails<select value={String(node.config.failurePolicy??'fail')} onChange={e=>edit('failurePolicy',e.target.value)}>
        <option value="fail">Stop and cancel remaining items</option><option value="collect">Collect errors and continue</option>
      </select></label>
    </>}
  </details>
  <a href={each ? 'https://github.com/haswalt/kasuri/blob/develop/docs/items.md#each--loop' : 'https://github.com/haswalt/kasuri/blob/develop/docs/workflow-invocation.md'}
    target="_blank" rel="noreferrer">{each ? 'Each / Loop guide and example' : 'Reusable workflow setup and examples'}</a>
 </section>
}
