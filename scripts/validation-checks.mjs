import assert from 'node:assert/strict'
import {readFileSync} from 'node:fs'
import {versionedTestClient} from './versioned-test-client.mjs'
export async function checkValidation(origin,credentials,restart) {
 let cookie='';const versions=versionedTestClient();const Authorization='Basic '+Buffer.from(credentials).toString('base64')
 async function api(path,method='GET',body,expected=200){body=versions.prepare(path,method,body);const r=await fetch(origin+'/api/'+path,{method,headers:{Authorization,cookie,...(body===undefined?{}:{'Content-Type':'application/json'})},...(body===undefined?{}:{body:JSON.stringify(body)})});if(r.headers.has('set-cookie'))cookie=r.headers.get('set-cookie').split(';')[0];assert.ok(r.status===expected||(expected===200&&r.status===201),`${path}: ${r.status} ${await r.clone().text()}`);return versions.remember(await r.json())}
 const example=JSON.parse(readFileSync('examples/workflows/15-schema-validation.json','utf8'));await api('workflows/'+example.id,'PUT',example)
 const preview=await api('runs','POST',{workflowId:example.id,mode:'simulation',input:{}});assert.equal(preview.status,'succeeded');assert.equal(preview.nodes.find(n=>n.id==='validate').output.valid,true)
 for(const schema of [{$ref:'https://private.internal/schema.json'},{pattern:'(a+)+$'},{format:'typo-format'}]) {
  const invalid=structuredClone(example);invalid.id='bad-schema-'+Object.keys(schema)[0].replace('$','');invalid.nodes.find(n=>n.id==='validate').config.schema=schema;await api('workflows/'+invalid.id,'PUT',invalid);await api('workflows/'+invalid.id+'/publish','POST',undefined,400)
 }
 const graph=JSON.parse(readFileSync('examples/workflows/16-request-validation-gate.json','utf8'));await api('workflows/'+graph.id,'PUT',graph);const endpoint=await api('workflows/'+graph.id+'/webhook/rotate','POST');await api('workflows/'+graph.id+'/publish','POST')
 const post=(body,eventType='order.created')=>fetch(origin+endpoint.path,{method:'POST',headers:{'Content-Type':'application/json','X-Event-Type':eventType},body:JSON.stringify(body)})
 let response=await post({eventId:'wrong-header',amount:10},'other.event');assert.equal(response.status,400);assert.deepEqual(await response.json(),{error:'Expected x-event-type: order.created'})
 response=await post({email:'private-echo-value',amount:-1});assert.equal(response.status,400);let invalid=await response.json();assert.equal(invalid.error,'Invalid request body');assert.ok(invalid.errors.some(e=>e.keyword==='required'&&e.missingProperty==='eventId'));assert.ok(invalid.errors.some(e=>e.keyword==='minimum'));assert.equal(JSON.stringify(invalid.errors).includes('private-echo-value'),false)
 const good=await post({eventId:'validated-event',amount:10});assert.equal(good.status,201);assert.deepEqual(await good.json(),{accepted:true})
 origin=await restart()||origin;const repeated=await post({eventId:'validated-event',amount:10});assert.equal(repeated.status,409);assert.deepEqual(await repeated.json(),{duplicate:true})
 const row=(await api('data','POST',{command:{operation:'get',tableId:'example-request-events',key:'validated-event'}})).result.row;assert.equal(row.value.accepted,true)
 console.log('Passed: offline schema validation in Wasm, rejected external/unsupported schemas, header/body validation branches, safe diagnostics and validated atomic 201/409 event handling across restart.')
}
