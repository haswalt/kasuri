import assert from 'node:assert/strict'
import {readFileSync} from 'node:fs'
import {request as httpRequest} from 'node:http'
import {versionedTestClient} from './versioned-test-client.mjs'
import {sleep} from './runtime-checks.mjs'
export async function checkWebhookResponses(origin,credentials,restart) {
 let cookie='';const versions=versionedTestClient();const Authorization='Basic '+Buffer.from(credentials).toString('base64')
 async function api(path,method='GET',body,expected=200){body=versions.prepare(path,method,body);const r=await fetch(origin+'/api/'+path,{method,headers:{Authorization,cookie,...(body===undefined?{}:{'Content-Type':'application/json'})},...(body===undefined?{}:{body:JSON.stringify(body)})});if(r.headers.has('set-cookie'))cookie=r.headers.get('set-cookie').split(';')[0];assert.ok(r.status===expected||(expected===200&&r.status===201),`${path}: ${r.status} ${await r.clone().text()}`);return versions.remember(await r.json())}
 const base=JSON.parse(readFileSync('examples/workflows/13-webhook-response.json','utf8'))
 function flow(id,{waitBefore=0,waitAfter=0,duplicate=false}={}) {
  const doc=structuredClone(base);doc.id=id;doc.nodes=doc.nodes.filter(n=>n.id!=='background')
  if(waitBefore)doc.nodes.splice(2,0,{id:'wait-before',type:'Wait / Delay',label:'Wait before response',config:{delayMs:waitBefore}})
  if(waitAfter)doc.nodes.push({id:'wait-after',type:'Wait / Delay',label:'Background wait',config:{delayMs:waitAfter}})
  doc.nodes.push(duplicate?{id:'duplicate',type:'Respond to Webhook',label:'Incorrect second response',config:{body:{duplicate:true}}}:structuredClone(base.nodes.at(-1)))
  doc.edges=doc.nodes.slice(1).map((n,i)=>({id:'edge-'+i,source:doc.nodes[i].id,target:n.id}));return doc
 }
 async function deploy(doc){await api('workflows/'+doc.id,'PUT',doc);const endpoint=await api('workflows/'+doc.id+'/webhook/rotate','POST');await api('workflows/'+doc.id+'/publish','POST');return endpoint}
 const post=(endpoint,signal)=>fetch(origin+endpoint.path,{method:'POST',headers:{'Content-Type':'application/json'},body:'{"request":"synthetic"}',...(signal?{signal}:{})})
 async function finish(id){let run;for(let i=0;i<100;i++){run=await api('runs/'+id);if(!['running','paused'].includes(run.status))return run;await sleep(100)}throw Error('Run did not finish: '+id)}
 const endpoint=await deploy(flow('early-response',{waitAfter:1200}))
 const response=await post(endpoint);assert.equal(response.status,201);assert.equal(response.headers.get('x-workflow-result'),'accepted');assert.equal(response.headers.get('set-cookie'),null);assert.equal(response.headers.get('cache-control'),'no-store');assert.equal(response.headers.get('x-content-type-options'),'nosniff');assert.deepEqual(await response.json(),{accepted:true,message:'Hello from Kasuri'})
 const id=response.headers.get('x-kasuri-run-id');assert.ok(id);assert.equal((await api('runs/'+id)).status,'running','Background work continues after response')
 origin=await restart()||origin;let run=await finish(id);assert.equal(run.status,'succeeded');assert.equal(run.webhookResponse.delivery,'submitted');assert.deepEqual(run.nodes.find(n=>n.id==='respond').output,{accepted:true,message:'Hello from Kasuri'})
 const delayed=await post(await deploy(flow('delayed-response',{waitBefore:200})));assert.equal(delayed.status,201);await finish(delayed.headers.get('x-kasuri-run-id'))
 const duplicate=await post(await deploy(flow('duplicate-response',{duplicate:true})));assert.equal(duplicate.status,201);run=await finish(duplicate.headers.get('x-kasuri-run-id'));assert.equal(run.status,'failed');assert.equal(run.nodes.find(n=>n.id==='duplicate').error,'duplicate_response');assert.equal(run.webhookResponse.decision.nodeId,'respond')
 const textDoc=flow('text-response');textDoc.nodes.find(n=>n.id==='respond').config={responseType:'text',body:{$input:'/message'}}
 const text=await post(await deploy(textDoc));assert.equal(text.status,200);assert.match(text.headers.get('content-type'),/^text\/plain/);assert.equal(await text.text(),'Hello from Kasuri')
 const emptyDoc=flow('empty-response');emptyDoc.nodes.find(n=>n.id==='respond').config={status:204,body:null}
 const empty=await post(await deploy(emptyDoc));assert.equal(empty.status,204);assert.equal(await empty.text(),'')
 const unsafe=flow('unsafe-response');unsafe.nodes.find(n=>n.id==='respond').config.headers={'Set-Cookie':'session=bad'};await api('workflows/'+unsafe.id,'PUT',unsafe);await api('workflows/'+unsafe.id+'/publish','POST',undefined,400)
 const missing=flow('missing-response');missing.nodes=[missing.nodes[0],{id:'if',type:'IF / Condition',label:'Never take response branch',config:{pointer:'/request',equals:'different'}},missing.nodes.find(n=>n.id==='respond'),{id:'skip',type:'Map / Transform',label:'Finish without responding',config:{mapping:{done:true}}}];missing.edges=[{id:'a',source:'start',target:'if'},{id:'b',source:'if',target:'respond',sourcePort:'true'},{id:'c',source:'if',target:'skip',sourcePort:'false'}]
 const noResponse=await post(await deploy(missing));assert.equal(noResponse.status,502);assert.equal((await noResponse.json()).error,'workflow_completed_without_response')
 const failed=flow('failed-response');failed.nodes[1]={id:'prepare',type:'JSON Utilities',label:'Fail parsing',config:{operation:'parse'}}
 const failure=await post(await deploy(failed));assert.equal(failure.status,500);assert.equal((await failure.json()).error,'workflow_failed_before_response')
 const timeout=await post(await deploy(flow('late-response',{waitBefore:5500})));assert.equal(timeout.status,504);const timeoutId=timeout.headers.get('x-kasuri-run-id');assert.equal((await timeout.json()).error,'webhook_response_timeout');run=await finish(timeoutId);assert.equal(run.status,'succeeded');assert.equal(run.webhookResponse.delivery,'timed_out');assert.equal(run.webhookResponse.decision.late,true)
 const disconnectEndpoint=await deploy(flow('disconnected-response',{waitBefore:2000}));let connection;const pending=new Promise(resolve=>{connection=httpRequest(origin+disconnectEndpoint.path,{method:'POST',headers:{'Content-Type':'application/json','Connection':'close'}},res=>{res.resume();resolve(res.statusCode)});connection.on('error',resolve);connection.end('{"request":"synthetic"}')})
 let disconnectId;for(let i=0;i<30;i++){disconnectId=(await api('workspace')).runs.find(r=>r.workflowId==='disconnected-response')?.id;if(disconnectId)break;await sleep(30)}assert.ok(disconnectId);const beforeAbort=await api('runs/'+disconnectId);assert.equal(beforeAbort.webhookResponse.delivery,'waiting',JSON.stringify(beforeAbort));connection.destroy();assert.ok(await pending instanceof Error)
 // A transport may not notify the DO before preparation. Submitted records an
 // unconfirmed attempt, never successful client receipt; observed abort is distinct.
 run=await finish(disconnectId);assert.equal(run.status,'succeeded');assert.ok(['disconnected','submitted'].includes(run.webhookResponse.delivery));assert.equal(run.webhookResponse.decision.late,run.webhookResponse.delivery==='disconnected')
 console.log('Passed: positioned webhook responses, pass-through input, asynchronous continuation/restart, JSON/text/empty bodies, unsafe header rejection, missing/failed/duplicate responses, timeout and disconnected-client durability.')
}
