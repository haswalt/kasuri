import { execFileSync } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import assert from 'node:assert/strict'
const image = process.argv[2] || 'kasuri:local'
const name = `kasuri-check-${randomUUID().slice(0,8)}`
const docker = (...args) => execFileSync('docker',args,{encoding:'utf8'}).trim()
const sleep = ms => new Promise(r=>setTimeout(r,ms))
let cookie=''; let origin
async function api(path,method='GET',body) {
  const response = await fetch(`${origin}/api/${path}`,{method,headers:{cookie,...(body===undefined?{}:{'Content-Type':'application/json'})},...(body===undefined?{}:{body:JSON.stringify(body)})})
  if(response.headers.has('set-cookie')) cookie=response.headers.get('set-cookie').split(';')[0]
  assert.ok(response.ok, `${path}: ${response.status}`)
  return response.json()
}
async function healthy() {
  for(let i=0;i<60;i++) { try { if((await fetch(`${origin}/api/health`)).ok) return } catch {} await sleep(250) }
  throw new Error('Container did not become healthy.')
}
try {
  docker('volume','create',name)
  docker('run','--detach','--name',name,'--publish','127.0.0.1::8080','--volume',`${name}:/data`,'--read-only','--tmpfs','/tmp','--cap-drop','ALL','--security-opt','no-new-privileges',image)
  origin=`http://${docker('port',name,'8080/tcp').split('\n')[0]}`
  await healthy()
  assert.equal(docker('exec',name,'id','-u'),'1000')
  await api('workspace')
  let run=await api('runs','POST',{workflowId:'core-data-demo',mode:'simulation',input:{name:'Container'}})
  assert.equal(run.status,'succeeded')
  assert.deepEqual(run.nodes.find(n=>n.id==='parse').output,{name:'Container',source:'kasuri'})
  run=await api('runs','POST',{workflowId:'core-data-demo',mode:'run',input:{name:'Restarted'}})
  assert.equal(run.status,'running')
  docker('restart',name)
  origin=`http://${docker('port',name,'8080/tcp').split('\n')[0]}`
  await healthy()
  for(let i=0;i<60;i++) { run=await api(`runs/${run.id}`); if(run.status==='succeeded') break; await sleep(250) }
  assert.equal(run.status,'succeeded')
  assert.equal(run.nodes.find(n=>n.id==='wait').attempts,1)
  assert.deepEqual(run.nodes.find(n=>n.id==='parse').output,{name:'Restarted',source:'kasuri'})
  console.log('Container checks passed: non-root runtime, simulations, persistent data and wait recovery after restart.')
} catch(error) {
  try { console.error(docker('logs',name)) } catch {}
  throw error
} finally {
  try { docker('rm','--force',name) } catch {}
  try { docker('volume','rm',name) } catch {}
}
