import { versionedTestClient } from './versioned-test-client.mjs'
import assert from 'node:assert/strict'
export const fixture={schemaVersion:1,id:'check',name:'Runtime check',description:'',status:'draft',revision:0,updatedAt:'',nodes:[{id:'start',type:'Manual Trigger',label:'Start',config:{}},{id:'wait',type:'Wait / Delay',label:'Wait',config:{delayMs:2000}}],edges:[{id:'edge',source:'start',target:'wait'}]}
export const sleep=ms=>new Promise(r=>setTimeout(r,ms))
export async function healthy(origin){for(let i=0;i<80;i++){try{if((await fetch(`${origin}/api/health`)).ok)return}catch{}await sleep(250)}throw Error('Runtime did not become healthy')}
export async function checkRuntime(origin,credentials,restart){
 let cookie='';const authorization='Basic '+Buffer.from(credentials).toString('base64')
 const versionClient=versionedTestClient()
 async function api(path,method='GET',body){body=versionClient.prepare(path,method,body);const res=await fetch(`${origin}/api/${path}`,{method,headers:{cookie,Authorization:authorization,...(body===undefined?{}:{'Content-Type':'application/json'})},...(body===undefined?{}:{body:JSON.stringify(body)})});if(res.headers.has('set-cookie'))cookie=res.headers.get('set-cookie').split(';')[0];assert.ok(res.ok,`${path}: ${res.status} ${await res.clone().text()}`);return versionClient.remember(await res.json())}
 assert.equal((await fetch(origin)).status,401)
 const html=await fetch(origin,{headers:{Authorization:authorization}});assert.equal(html.status,200);assert.match(html.headers.get('content-type'),/text\/html/);const htmlText=await html.text();assert.match(htmlText,/root/);assert.match(html.headers.get('content-security-policy')??'',/frame-ancestors 'none'/);assert.equal(html.headers.get('x-frame-options'),'DENY');assert.equal(html.headers.get('referrer-policy'),'strict-origin-when-cross-origin')
 const stylesheet=htmlText.match(/href="([^"]+\.css)"/);assert.ok(stylesheet,'Built UI stylesheet')
 const cssUrl=new URL(stylesheet[1],origin);const cssResponse=await fetch(cssUrl,{headers:{Authorization:authorization}});assert.equal(cssResponse.status,200)
 const css=await cssResponse.text();const font=css.match(/url\(["']?([^"')]+\.woff2)/);assert.ok(font,'Bundled UI font')
 const fontResponse=await fetch(new URL(font[1],cssUrl),{headers:{Authorization:authorization}});assert.equal(fontResponse.status,200);assert.match(fontResponse.headers.get('content-type'),/font\/woff2/);assert.ok((await fontResponse.arrayBuffer()).byteLength>1000)
 const empty=await api('workspace');assert.equal(empty.workflows.length,0);assert.equal(empty.runs.length,0);assert.equal('expiresAt' in empty,false)
 await api('workflows/check','PUT',fixture)
 let run=await api('runs','POST',{workflowId:'check',mode:'simulation',input:{test:1}});assert.equal(run.status,'succeeded')
 run=await api('runs','POST',{workflowId:'check',mode:'run',input:{test:2}});assert.equal(run.status,'running')
 origin=await restart()||origin;await healthy(origin)
 for(let i=0;i<80;i++){run=await api(`runs/${run.id}`);if(run.status==='succeeded')break;await sleep(250)}
 assert.equal(run.status,'succeeded',JSON.stringify({now:Date.now(),run}));assert.equal(run.nodes.find(n=>n.id==='wait').attempts,1)
 assert.equal((await api('workspace')).workflows.length,1)
 const blocked=await fetch(`${origin}/api/workflows/check/publish`,{method:'POST',headers:{cookie,Authorization:authorization,Origin:'https://foreign.example'}});assert.equal(blocked.status,403)
 const schedule=structuredClone(fixture);schedule.id='scheduled';schedule.nodes[0].type='Schedule';schedule.nodes[0].config={cron:'* * * * *',timezone:'UTC'}
 await api('workflows/scheduled','PUT',schedule);const published=await api('workflows/scheduled/publish','POST');assert.ok(published.nextScheduledAt>Date.now());origin=await restart()||origin;await healthy(origin);assert.equal((await api('workspace')).workflows.find(w=>w.id==='scheduled').nextScheduledAt,published.nextScheduledAt);await api('workflows/scheduled/pause','POST')
 console.log('Passed: empty workspace, assets, authentication, simulations, persistent waits, cron registration and cross-site protection.')
}
