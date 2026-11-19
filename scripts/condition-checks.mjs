import assert from 'node:assert/strict'
import {readFileSync} from 'node:fs'
import {versionedTestClient} from './versioned-test-client.mjs'
export async function checkConditions(origin,credentials,restart) {
 let cookie='';const versions=versionedTestClient();const Authorization='Basic '+Buffer.from(credentials).toString('base64')
 async function api(path,method='GET',body,expected=200){body=versions.prepare(path,method,body);const r=await fetch(origin+'/api/'+path,{method,headers:{Authorization,cookie,...(body===undefined?{}:{'Content-Type':'application/json'})},...(body===undefined?{}:{body:JSON.stringify(body)})});if(r.headers.has('set-cookie'))cookie=r.headers.get('set-cookie').split(';')[0];assert.ok(r.status===expected || (expected===200&&r.status===201),`${path}: ${r.status} ${await r.clone().text()}`);return versions.remember(await r.json())}
 const example=JSON.parse(readFileSync('examples/workflows/14-structured-conditions.json','utf8'));await api('workflows/'+example.id,'PUT',example)
 const simulated=await api('runs','POST',{workflowId:example.id,mode:'simulation',input:{}});assert.equal(simulated.status,'succeeded');assert.deepEqual(simulated.nodes.find(n=>n.id==='accepted').output,{accepted:true})
 const collection=JSON.parse(readFileSync('examples/workflows/03-filter-sort.json','utf8'));await api('workflows/'+collection.id,'PUT',collection)
 const filtered=await api('runs','POST',{workflowId:collection.id,mode:'simulation',input:{}});assert.equal(filtered.status,'succeeded');assert.deepEqual(filtered.nodes.find(n=>n.id==='ordered').output.map(item=>item.name),['Alder','Cedar'])
 const invalid=structuredClone(example);invalid.id='invalid-condition';invalid.nodes.find(n=>n.id==='check').config.conditions[1].value='not a number';await api('workflows/'+invalid.id,'PUT',invalid);await api('workflows/'+invalid.id+'/publish','POST',undefined,400)
 await api('data','POST',{idempotencyKey:'create-workflow-claims',command:{operation:'create_table',definition:{id:'workflow-claims',name:'Workflow claims',kind:'context'}}})
 const graph={schemaVersion:1,id:'atomic-duplicate-gate',name:'Atomic duplicate gate',status:'draft',updatedAt:'',nodes:[
 {id:'start',type:'Webhook',label:'Receive request',config:{authentication:'none'}},
 {id:'claim',type:'Context Store',label:'Atomic event claim',config:{command:{operation:'insert_if_absent',tableId:'workflow-claims',key:{$input:'/eventId'},value:{handled:true},ttlMs:300000}}},
 {id:'check',type:'IF / Condition',label:'Was this claim inserted?',config:{match:'all',conditions:[{field:'inserted',operator:'equals',valueType:'boolean',value:true}]}},
 {id:'accepted',type:'Respond to Webhook',label:'New event',config:{status:201,body:{accepted:true}}},
 {id:'duplicate',type:'Respond to Webhook',label:'Duplicate event',config:{status:409,body:{duplicate:true}}}],edges:[{id:'a',source:'start',target:'claim'},{id:'b',source:'claim',target:'check'},{id:'c',source:'check',target:'accepted',sourcePort:'true'},{id:'d',source:'check',target:'duplicate',sourcePort:'false'}]}
 await api('workflows/'+graph.id,'PUT',graph);const endpoint=await api('workflows/'+graph.id+'/webhook/rotate','POST');await api('workflows/'+graph.id+'/publish','POST')
 const send=()=>fetch(origin+endpoint.path,{method:'POST',headers:{'Content-Type':'application/json'},body:'{"eventId":"same-business-event"}'})
 const requests=await Promise.all([send(),send()]);assert.deepEqual(requests.map(r=>r.status).sort(),[201,409]);const results=await Promise.all(requests.map(r=>r.json()));assert.ok(results.some(r=>r.accepted===true));assert.ok(results.some(r=>r.duplicate===true))
 const before=(await api('data','POST',{command:{operation:'get',tableId:'workflow-claims',key:'same-business-event'}})).result.row.version
 origin=await restart()||origin;assert.equal((await send()).status,409)
 assert.equal((await api('data','POST',{command:{operation:'get',tableId:'workflow-claims',key:'same-business-event'}})).result.row.version,before)
 console.log('Passed: structured typed IF execution, static validation, atomic concurrent duplicate gating and workflow-selected 201/409 responses across restart.')
}
