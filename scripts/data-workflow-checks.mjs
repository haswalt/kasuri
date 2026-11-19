import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { versionedTestClient } from './versioned-test-client.mjs'
import { sleep } from './runtime-checks.mjs'
export async function checkDataWorkflows(origin,credentials,restart) {
  let cookie='';const versions=versionedTestClient()
  const Authorization='Basic '+Buffer.from(credentials).toString('base64')
  async function api(path,method='GET',body) {
    body=versions.prepare(path,method,body)
    const response=await fetch(origin+'/api/'+path,{method,headers:{Authorization,cookie,...(body===undefined?{}:{'Content-Type':'application/json'})},...(body===undefined?{}:{body:JSON.stringify(body)})})
    if(response.headers.has('set-cookie'))cookie=response.headers.get('set-cookie').split(';')[0]
    assert.ok([200,201].includes(response.status),`${path}: ${response.status} ${await response.clone().text()}`)
    const value=await response.json();versions.remember(value);return value
  }
  const read=(tableId,key)=>api('data','POST',{command:{operation:'get',tableId,key}})
  const create=async (id,kind)=>api('data','POST',{command:{operation:'create_table',definition:{id,name:id,kind}},idempotencyKey:'create-'+id})
  for (const file of ['10-context-cache','11-table-claim']) {
    const graph=JSON.parse(readFileSync(`examples/workflows/${file}.json`,'utf8'))
    await api(`workflows/${graph.id}`,'PUT',graph)
    const run=await api('runs','POST',{workflowId:graph.id,mode:'simulation',input:{}})
    assert.equal(run.status,'succeeded',JSON.stringify(run))
    assert.equal((await api('tables')).tables.some(t=>t.id.startsWith('example-')),false,'Simulated dataset creation must remain isolated')
  }
  // Snapshot existing live values, then mutate and expire only the simulation copy.
  await create('snapshot-cache','context')
  await api('data','POST',{command:{operation:'put',tableId:'snapshot-cache',key:'record',value:{message:'live'},ttlMs:60000},idempotencyKey:'snapshot-live-record'})
  const before=await read('snapshot-cache','record')
  const base={schemaVersion:1,id:'data-isolation',name:'Data isolation',description:'',status:'draft',revision:0,updatedAt:'',nodes:[
    {id:'start',type:'Manual Trigger',label:'Start',config:{}},
    {id:'get',type:'Context Store',label:'Snapshot read',config:{command:{operation:'get',tableId:'snapshot-cache',key:'record'}}},
    {id:'put',type:'Context Store',label:'Write',config:{command:{operation:'put',tableId:'snapshot-cache',key:'record',value:{message:'simulation'},ttlMs:100}}},
    {id:'wait',type:'Wait / Delay',label:'Wait',config:{delayMs:200}},
    {id:'expired',type:'Context Store',label:'Expired',config:{command:{operation:'get',tableId:'snapshot-cache',key:'record'}}}
  ],edges:[]}
  base.edges=base.nodes.slice(1).map((n,i)=>({id:'edge-'+i,source:base.nodes[i].id,target:n.id}))
  await api('workflows/'+base.id,'PUT',base)
  const simulated=await api('runs','POST',{workflowId:base.id,mode:'simulation',input:{}})
  assert.equal(simulated.status,'succeeded')
  assert.equal(simulated.nodes.find(n=>n.id==='get').output.row.value.message,'live')
  assert.equal(simulated.nodes.find(n=>n.id==='expired').output.status,'not_found')
  assert.deepEqual(await read('snapshot-cache','record'),before)
  const live=structuredClone(base);live.id='data-restart';live.name='Durable data restart';live.nodes=live.nodes.slice(0,4);live.nodes[3].config.delayMs=1200;live.edges=live.edges.slice(0,3)
  await api('workflows/'+live.id,'PUT',live)
  const run=await api('runs','POST',{workflowId:live.id,mode:'run',input:{}})
  assert.equal(run.status,'running')
  const committed=await api('data','POST',{command:{operation:'get_table',tableId:'snapshot-cache'}})
  origin=await restart()||origin
  let recovered
  for(let i=0;i<60;i++){recovered=await api('runs/'+run.id);if(recovered.status!=='running')break;await sleep(100)}
  assert.equal(recovered.status,'succeeded')
  assert.equal((await api('data','POST',{command:{operation:'get_table',tableId:'snapshot-cache'}})).result.table.dataRevision,committed.result.table.dataRevision,'Restart must not repeat a committed node write')
  const mismatch=structuredClone(base);mismatch.id='data-kind-check';mismatch.nodes=mismatch.nodes.slice(0,2);mismatch.nodes[1].type='Data Tables';mismatch.edges=mismatch.edges.slice(0,1)
  await api('workflows/'+mismatch.id,'PUT',mismatch)
  const invalid=await api('runs','POST',{workflowId:mismatch.id,mode:'run',input:{}})
  assert.equal(invalid.status,'failed','A table connector cannot access a context namespace')
  assert.equal((await api('audit?order=desc')).items.some(e=>e.actor==='engine'),false)
  console.log('Passed: importable data examples, live snapshot isolation, virtual TTL, durable writes across restart, dataset kinds and execution audit exclusion.')
}
