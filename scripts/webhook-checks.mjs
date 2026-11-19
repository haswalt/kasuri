import { versionedTestClient } from './versioned-test-client.mjs'
import assert from 'node:assert/strict'
import { createHmac } from 'node:crypto'
import { sleep } from './runtime-checks.mjs'
export async function checkWebhooks(origin,restart){
 let cookie='';const authorization='Basic '+Buffer.from('test:runtime-check').toString('base64')
 const versionClient=versionedTestClient()
 async function api(path,method='GET',body){body=versionClient.prepare(path,method,body);const r=await fetch(`${origin}/api/${path}`,{method,headers:{cookie,Authorization:authorization,...(body?{'Content-Type':'application/json'}:{})},...(body?{body:JSON.stringify(body)}:{})});if(r.headers.has('set-cookie'))cookie=r.headers.get('set-cookie').split(';')[0];assert.ok(r.ok,`${path}: ${r.status} ${await r.clone().text()}`);return versionClient.remember(await r.json())}
 const doc={schemaVersion:1,id:'incoming-check',name:'Incoming check',status:'draft',updatedAt:'',nodes:[{id:'start',type:'Webhook',label:'Incoming',config:{authentication:'bearer'}},{id:'http',type:'HTTP Request',label:'Fetch',config:{url:'https://fixture.example/items'}},{id:'filter',type:'Filter',label:'Active',config:{arrayPointer:'/body/items',pointer:'/active',equals:true}}],edges:[{id:'a',source:'start',target:'http'},{id:'b',source:'http',target:'filter'}]}
 await api('workflows/incoming-check','PUT',doc);let endpoint=await api('workflows/incoming-check/webhook/rotate','POST');assert.ok(endpoint.token);assert.ok(endpoint.path);assert.match(endpoint.path,/^\/hooks\/[a-f0-9]{64}\/[a-f0-9]{64}$/);assert.equal((await api('workflows/incoming-check/webhook')).token,undefined)
 async function deliver(endpoint,event,body='{"name":"Ada"}',extra={}){return fetch(origin+endpoint.path,{method:'POST',headers:{'Content-Type':'application/json','X-Kasuri-Event-Id':event,Authorization:'Bearer '+endpoint.token,...extra},body})}
 assert.equal((await deliver(endpoint,'event-1')).status,404);await api('workflows/incoming-check/publish','POST')
 assert.equal((await deliver(endpoint,'event-1',undefined,{Authorization:'Bearer wrong'})).status,401)
 const accepted=await deliver(endpoint,'event-1');assert.equal(accepted.status,202);const first=await accepted.json();assert.deepEqual(Object.keys(first),['runId']);const legacy={...endpoint,path:`/api/hooks/${endpoint.path.split('/')[2]}/incoming-check/${endpoint.endpointId}`};const alias=await deliver(legacy,'event-1');assert.equal(alias.status,202);assert.notEqual((await alias.json()).runId,first.runId)
 origin=await restart()||origin
 const repeated=await deliver(endpoint,'event-1');assert.equal(repeated.status,202);assert.notEqual((await repeated.json()).runId,first.runId)
 assert.equal((await deliver(endpoint,'event-1','{"changed":true}')).status,202)
 let run=await api('runs/'+first.runId);for(let i=0;i<80&&run.status==='running';i++){await sleep(100);run=await api('runs/'+first.runId)}assert.equal(run.status,'succeeded',JSON.stringify(run));assert.equal(run.mode,'webhook');assert.equal(run.nodes.find(n=>n.id==='filter').output.length,2);assert.deepEqual(run.input,{name:'Ada'});assert.equal(JSON.stringify(run).includes(endpoint.token),false)
 const exposedWorkspace=endpoint.path.split('/')[2];for(const forged of [`kasuri_session=${exposedWorkspace}`,`kasuri_session=${exposedWorkspace}.${'0'.repeat(64)}`]){const r=await fetch(origin+'/api/workspace',{headers:{Cookie:forged,Authorization:authorization,'X-Kasuri-Session':'spoof'}});assert.equal(r.status,403)}
 await api('workflows/incoming-check/pause','POST');assert.equal((await deliver(endpoint,'event-2')).status,404);await api('workflows/incoming-check/resume','POST')
 const old=endpoint;endpoint=await api('workflows/incoming-check/webhook/rotate','POST');assert.notEqual(endpoint.path,old.path);assert.equal((await deliver(old,'event-2')).status,404);assert.equal((await deliver(endpoint,'event-2')).status,202)
 assert.equal((await deliver(endpoint,'event-3','x'.repeat(32769))).status,413)
 doc.nodes[0].config={authentication:'hmac',signingSecret:'incoming_key'};await api('workflows/incoming-check','PUT',doc);await api('workflows/incoming-check/publish','POST');endpoint=await api('workflows/incoming-check/webhook');const body='{"signed":true}';const event='signed-event';const timestamp=String(Math.floor(Date.now()/1000));const signature=createHmac('sha256','incoming-test-key').update(`${timestamp}.${event}.${body}`).digest('hex')
 assert.equal((await deliver(endpoint,event,body,{'X-Kasuri-Timestamp':timestamp,'X-Kasuri-Signature':signature})).status,202)
 assert.equal((await deliver(endpoint,'tampered',body,{'X-Kasuri-Timestamp':timestamp,'X-Kasuri-Signature':signature})).status,401)
 const oldTime=String(Number(timestamp)-301);const expired=createHmac('sha256','incoming-test-key').update(`${oldTime}.expired.${body}`).digest('hex');assert.equal((await deliver(endpoint,'expired',body,{'X-Kasuri-Timestamp':oldTime,'X-Kasuri-Signature':expired})).status,401)
 // New HMAC format authenticates timestamp/body without a business event ID.
 doc.nodes[0].config={authentication:'hmac_sha256',signingSecret:'incoming_key'};await api('workflows/incoming-check','PUT',doc);await api('workflows/incoming-check/publish','POST');
 const modernTime=String(Math.floor(Date.now()/1000));const modernSignature=createHmac('sha256','incoming-test-key').update(`${modernTime}.${body}`).digest('hex')
 const signed=await fetch(origin+endpoint.path,{method:'POST',headers:{'Content-Type':'application/json','X-Kasuri-Timestamp':modernTime,'X-Kasuri-Signature':modernSignature},body});assert.equal(signed.status,202)
 assert.equal((await fetch(origin+endpoint.path,{method:'POST',headers:{'Content-Type':'application/json','X-Kasuri-Timestamp':modernTime,'X-Kasuri-Signature':modernSignature},body:'{}'})).status,401)
 await api('workflows/incoming-check/pause','POST')
 // An independent public JSON endpoint requires neither credentials nor event ID.
 const publicDoc={schemaVersion:1,id:'public-hook',name:'Public hook',status:'draft',updatedAt:'',nodes:[{id:'start',type:'Webhook',label:'Public incoming',config:{}}],edges:[]}
 const saved=await api('workflows/public-hook','PUT',publicDoc);assert.equal(saved.nodes[0].config.authentication,'none')
 let publicEndpoint=await api('workflows/public-hook/webhook/rotate','POST');assert.equal(publicEndpoint.token,undefined);await api('workflows/public-hook/publish','POST')
 const post=async(path=publicEndpoint.path,headers={})=>fetch(origin+path,{method:'POST',headers:{'Content-Type':'application/json',...headers},body:'{"event":"same"}'})
 const publicFirst=await post();assert.equal(publicFirst.status,202);const publicRun=(await publicFirst.json()).runId
 const publicRepeat=await post();assert.equal(publicRepeat.status,202);assert.notEqual((await publicRepeat.json()).runId,publicRun)
 origin=await restart()||origin;assert.equal((await post()).status,202)
 publicDoc.nodes[0].config={authentication:'query',queryParameter:'secret_key'};await api('workflows/public-hook','PUT',publicDoc)
 // Rotate while public live config remains active: key must be available for draft auth.
 publicEndpoint=await api('workflows/public-hook/webhook/rotate','POST');assert.ok(publicEndpoint.token);await api('workflows/public-hook/publish','POST')
 assert.equal((await post()).status,401);assert.equal((await post(publicEndpoint.path+'?secret_key=wrong')).status,401)
 const keyed=publicEndpoint.path+'?secret_key='+encodeURIComponent(publicEndpoint.token)
 const keyedResponse=await post(keyed);assert.equal(keyedResponse.status,202);const keyedRun=await api('runs/'+(await keyedResponse.json()).runId)
 assert.equal(JSON.stringify(keyedRun).includes(publicEndpoint.token),false)
 assert.equal((await post(keyed+'&secret_key=wrong')).status,401)
 assert.equal((await post(publicEndpoint.path,{Authorization:'Bearer '+publicEndpoint.token})).status,401)
 await api('workflows/public-hook/pause','POST')
 console.log('Webhook checks passed: optional authentication, independent deliveries, bearer/query keys, legacy and event-free HMAC, immutable live auth, rotation, bounds, restart persistence and session isolation.')
}
