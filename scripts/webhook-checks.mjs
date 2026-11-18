import assert from 'node:assert/strict'
import { createHmac } from 'node:crypto'
import { sleep } from './runtime-checks.mjs'
export async function checkWebhooks(origin,restart){
 let cookie='';const authorization='Basic '+Buffer.from('test:runtime-check').toString('base64')
 async function api(path,method='GET',body){const r=await fetch(`${origin}/api/${path}`,{method,headers:{cookie,Authorization:authorization,...(body?{'Content-Type':'application/json'}:{})},...(body?{body:JSON.stringify(body)}:{})});if(r.headers.has('set-cookie'))cookie=r.headers.get('set-cookie').split(';')[0];assert.ok(r.ok,`${path}: ${r.status} ${await r.clone().text()}`);return r.json()}
 const doc={schemaVersion:1,id:'incoming-check',name:'Incoming check',status:'draft',updatedAt:'',nodes:[{id:'start',type:'Webhook',label:'Incoming',config:{}},{id:'http',type:'HTTP Request',label:'Fetch',config:{url:'https://fixture.example/items'}},{id:'filter',type:'Filter',label:'Active',config:{arrayPointer:'/body/items',pointer:'/active',equals:true}}],edges:[{id:'a',source:'start',target:'http'},{id:'b',source:'http',target:'filter'}]}
 await api('workflows/incoming-check','PUT',doc);let endpoint=await api('workflows/incoming-check/webhook/rotate','POST');assert.ok(endpoint.token);assert.ok(endpoint.path);assert.equal((await api('workflows/incoming-check/webhook')).token,undefined)
 async function deliver(endpoint,event,body='{"name":"Ada"}',extra={}){return fetch(origin+endpoint.path,{method:'POST',headers:{'Content-Type':'application/json','X-Kasuri-Event-Id':event,Authorization:'Bearer '+endpoint.token,...extra},body})}
 assert.equal((await deliver(endpoint,'event-1')).status,404);await api('workflows/incoming-check/publish','POST')
 assert.equal((await deliver(endpoint,'event-1',undefined,{Authorization:'Bearer wrong'})).status,401)
 const accepted=await deliver(endpoint,'event-1');assert.equal(accepted.status,202);const first=await accepted.json();assert.equal(first.duplicate,false)
 origin=await restart()||origin
 const repeated=await deliver(endpoint,'event-1');assert.equal(repeated.status,202);assert.deepEqual(await repeated.json(),{runId:first.runId,eventId:'event-1',duplicate:true})
 assert.equal((await deliver(endpoint,'event-1','{"changed":true}')).status,409)
 let run=await api('runs/'+first.runId);for(let i=0;i<80&&run.status==='running';i++){await sleep(100);run=await api('runs/'+first.runId)}assert.equal(run.status,'succeeded',JSON.stringify(run));assert.equal(run.mode,'webhook');assert.equal(run.nodes.find(n=>n.id==='filter').output.length,2);assert.deepEqual(run.input,{name:'Ada'});assert.equal(JSON.stringify(run).includes(endpoint.token),false)
 const exposedWorkspace=endpoint.path.split('/')[3];for(const forged of [`kasuri_session=${exposedWorkspace}`,`kasuri_session=${exposedWorkspace}.${'0'.repeat(64)}`]){const r=await fetch(origin+'/api/workspace',{headers:{Cookie:forged,Authorization:authorization,'X-Kasuri-Session':'spoof'}});assert.equal(r.status,403)}
 await api('workflows/incoming-check/pause','POST');assert.equal((await deliver(endpoint,'event-2')).status,404);await api('workflows/incoming-check/resume','POST')
 const old=endpoint;endpoint=await api('workflows/incoming-check/webhook/rotate','POST');assert.notEqual(endpoint.path,old.path);assert.equal((await deliver(old,'event-2')).status,404);assert.equal((await deliver(endpoint,'event-2')).status,202)
 assert.equal((await deliver(endpoint,'event-3','x'.repeat(32769))).status,413)
 doc.nodes[0].config={authentication:'hmac',signingSecret:'incoming_key'};await api('workflows/incoming-check','PUT',doc);await api('workflows/incoming-check/publish','POST');endpoint=await api('workflows/incoming-check/webhook');const body='{"signed":true}';const event='signed-event';const timestamp=String(Math.floor(Date.now()/1000));const signature=createHmac('sha256','incoming-test-key').update(`${timestamp}.${event}.${body}`).digest('hex')
 assert.equal((await deliver(endpoint,event,body,{'X-Kasuri-Timestamp':timestamp,'X-Kasuri-Signature':signature})).status,202)
 assert.equal((await deliver(endpoint,'tampered',body,{'X-Kasuri-Timestamp':timestamp,'X-Kasuri-Signature':signature})).status,401)
 const oldTime=String(Number(timestamp)-301);const expired=createHmac('sha256','incoming-test-key').update(`${oldTime}.expired.${body}`).digest('hex');assert.equal((await deliver(endpoint,'expired',body,{'X-Kasuri-Timestamp':oldTime,'X-Kasuri-Signature':expired})).status,401)
 await api('workflows/incoming-check/pause','POST')
 console.log('Webhook checks passed: publish/pause, bearer and HMAC auth, replay rejection, restart-safe deduplication, rotation, payload bounds, session isolation and Webhook → HTTP → Filter execution.')
}
