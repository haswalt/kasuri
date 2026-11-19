import { versionedTestClient } from './versioned-test-client.mjs'
import assert from 'node:assert/strict'
import { sleep } from './runtime-checks.mjs'
export async function checkHttp(origin){
 let cookie='';const authorization='Basic '+Buffer.from('test:runtime-check').toString('base64')
 const versionClient=versionedTestClient()
 async function api(path,method='GET',body){body=versionClient.prepare(path,method,body);const r=await fetch(`${origin}/api/${path}`,{method,headers:{cookie,Authorization:authorization,...(body?{'Content-Type':'application/json'}:{})},...(body?{body:JSON.stringify(body)}:{})});if(r.headers.has('set-cookie'))cookie=r.headers.get('set-cookie').split(';')[0];assert.ok(r.ok,`${path} ${r.status} ${await r.clone().text()}`);return versionClient.remember(await r.json())}
 async function workflow(config,extra=[],retry){const doc={schemaVersion:1,id:'http-check',name:'HTTP check',description:'',status:'draft',revision:0,updatedAt:'',nodes:[{id:'start',type:'Manual Trigger',label:'Start',config:{}},{id:'http',type:'HTTP Request',label:'HTTP',timeoutMs:config.timeoutMs||30000,config:{url:'https://fixture.example/items',...config},...(retry?{retry}:{})},...extra],edges:[]};doc.nodes.slice(1).forEach((n,i)=>doc.edges.push({id:'e'+i,source:doc.nodes[i].id,target:n.id}));await api('workflows/http-check','PUT',doc)}
 async function start(mode='run'){return api('runs','POST',{workflowId:'http-check',mode,input:{name:'Ada'}})}
 async function finish(run){for(let i=0;i<100&&run.status==='running';i++){await sleep(100);run=await api('runs/'+run.id)}return run}
 const workspace=await api('workspace');assert.equal(workspace.connectors.length,25)
 await workflow({},[{id:'filter',type:'Filter',label:'Active only',config:{arrayPointer:'/body/items',pointer:'/active',equals:'true'}},{id:'sort',type:'Sort',label:'Sort names',config:{pointer:'/name'}}]);let run=await finish(await start());assert.equal(run.status,'succeeded',JSON.stringify(run));assert.deepEqual(run.nodes.find(n=>n.id==='sort').output,[{name:'A',active:true},{name:'B',active:true}])
 await workflow({url:'https://fixture.example/echo',method:'POST',body:{record:{$input:'/name'}},query:{q:{$input:'/name'}}});run=await finish(await start());assert.equal(run.status,'succeeded');const output=run.nodes.find(n=>n.id==='http').output;assert.equal(output.body.method,'POST');assert.deepEqual(output.body.body,{record:'Ada'});assert.equal(output.body.query,'Ada');assert.ok(output.body.key);assert.equal('headers' in output,false)
 await workflow({url:'https://fixture.example/retry'},[],{maxAttempts:2,delayMs:50,maxDelayMs:50});run=await finish(await start());assert.equal(run.status,'succeeded');assert.equal(run.nodes.find(n=>n.id==='http').attempts,2)
 for(const [url,error] of [['https://private-dns.example/items','egress_denied'],['https://mixed-dns.example/items','egress_denied'],['https://private.internal/items','egress_denied'],['https://fixture.example/redirect','external'],['https://fixture.example/oversize','budget_exceeded'],['https://fixture.example/invalid','invalid_response']]){await workflow({url});run=await finish(await start());assert.equal(run.status,'failed');assert.equal(run.nodes.find(n=>n.id==='http').error,error)}
 await workflow({url:'https://unapproved.example/items'});run=await finish(await start());assert.equal(run.status,'succeeded')
 await workflow({url:'https://fixture.example/auth',authentication:'basic',username:'api-user',passwordSecret:'api_password',headers:{'X-Tenant':'team-a','X-Api-Key':{$secret:'api_key'}}});run=await finish(await start());assert.equal(run.status,'succeeded',JSON.stringify(run));let authOutput=run.nodes.find(n=>n.id==='http').output.body;assert.equal(authOutput.authenticated,true);assert.equal(authOutput.authorization,'[REDACTED]');assert.equal(authOutput.apiKey,'[REDACTED]');assert.equal(authOutput.password,'[REDACTED]');const visible=JSON.stringify({run,workspace:await api('workspace')});for(const secret of ['super-private-password','private-api-key',Buffer.from('api-user:super-private-password').toString('base64')])assert.equal(visible.includes(secret),false)
 await workflow({url:'https://fixture.example/bearer',headers:{Authorization:{$secret:'bearer'}}});run=await finish(await start());assert.equal(run.status,'succeeded');assert.equal(run.nodes.find(n=>n.id==='http').output.body.authenticated,true);assert.equal(run.nodes.find(n=>n.id==='http').output.body.echo,'[REDACTED]')
 await workflow({url:'https://fixture.example/auth',authentication:'basic',username:'api-user',passwordSecret:'not_configured'});run=await finish(await start());assert.equal(run.status,'failed');assert.equal(run.nodes.find(n=>n.id==='http').error,'missing_input')
 await workflow({url:'https://fixture.example/slow',timeoutMs:100});run=await finish(await start());assert.equal(run.status,'failed');assert.equal(run.nodes.find(n=>n.id==='http').error,'timeout')
 await workflow({url:'https://fixture.example/slow'});run=await start();await sleep(300);await api(`runs/${run.id}/cancel`,'POST');await sleep(1700);run=await api('runs/'+run.id);assert.equal(run.status,'cancelled')
 await workflow({url:'https://unapproved.example/never-called',simulationResponse:{status:200,body:{simulated:true}}});run=await start('simulation');assert.equal(run.status,'succeeded');assert.deepEqual(run.nodes.find(n=>n.id==='http').output.body,{simulated:true})
 console.log('HTTP checks passed: real dispatch, query/body mapping, filtering/sorting, retry, egress policy, redirects, response bounds, cancellation and offline simulation.')
}

export async function checkRestrictedHttp(origin){
 let cookie='';const authorization='Basic '+Buffer.from('test:runtime-check').toString('base64')
 const versionClient=versionedTestClient()
 async function api(path,method='GET',body){body=versionClient.prepare(path,method,body);const r=await fetch(`${origin}/api/${path}`,{method,headers:{cookie,Authorization:authorization,...(body?{'Content-Type':'application/json'}:{})},...(body?{body:JSON.stringify(body)}:{})});if(r.headers.has('set-cookie'))cookie=r.headers.get('set-cookie').split(';')[0];assert.ok(r.ok);return versionClient.remember(await r.json())}
 for(const [host,status]of [['unapproved.example','failed'],['fixture.example','succeeded'],['private.internal','succeeded'],['other.internal','failed']]){
  const doc={schemaVersion:1,id:'policy-check',name:'Policy check',description:'',status:'draft',revision:0,updatedAt:'',nodes:[{id:'start',type:'Manual Trigger',label:'Start',config:{}},{id:'http',type:'HTTP Request',label:'HTTP',config:{url:`https://${host}/items`}}],edges:[{id:'edge',source:'start',target:'http'}]}
  await api('workflows/policy-check','PUT',doc);let run=await api('runs','POST',{workflowId:doc.id,mode:'run',input:{}});for(let i=0;i<100&&run.status==='running';i++){await sleep(100);run=await api('runs/'+run.id)}assert.equal(run.status,status,host);if(status==='failed')assert.equal(run.nodes.find(n=>n.id==='http').error,'egress_denied')
 }
 console.log('HTTP policy checks passed: optional host restriction and explicit private-host opt-in.')
}

export async function checkAnonymousSecrets(origin){
 let cookie=''
 const versionClient=versionedTestClient()
 async function api(path,method='GET',body){body=versionClient.prepare(path,method,body);const r=await fetch(`${origin}/api/${path}`,{method,headers:{cookie,...(body?{'Content-Type':'application/json'}:{})},...(body?{body:JSON.stringify(body)}:{})});if(r.headers.has('set-cookie'))cookie=r.headers.get('set-cookie').split(';')[0];assert.ok(r.ok);return versionClient.remember(await r.json())}
 const doc={schemaVersion:1,id:'anonymous-check',name:'Anonymous credential check',status:'draft',updatedAt:'',nodes:[{id:'start',type:'Manual Trigger',label:'Start',config:{}},{id:'http',type:'HTTP Request',label:'HTTP',config:{url:'https://fixture.example/auth',authentication:'basic',username:'api-user',passwordSecret:'api_password'}}],edges:[{id:'edge',source:'start',target:'http'}]}
 await api('workflows/anonymous-check','PUT',doc);let run=await api('runs','POST',{workflowId:doc.id,mode:'run',input:{}});for(let i=0;i<100&&run.status==='running';i++){await sleep(100);run=await api('runs/'+run.id)}assert.equal(run.status,'failed');assert.equal(run.nodes.find(n=>n.id==='http').error,'egress_denied')
 console.log('Anonymous use of installation HTTP secrets blocked.')
}
