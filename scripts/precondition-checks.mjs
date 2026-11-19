import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
export async function checkPreconditions(origin,credentials) {
  let cookie=''
  const Authorization='Basic '+Buffer.from(credentials).toString('base64')
  async function call(path,method='GET',body,expected=200) {
    const response=await fetch(origin+'/api/'+path,{method,headers:{Authorization,cookie,...(body===undefined?{}:{'Content-Type':'application/json'})},...(body===undefined?{}:{body:JSON.stringify(body)})})
    if(response.headers.has('set-cookie'))cookie=response.headers.get('set-cookie').split(';')[0]
    assert.equal(response.status,expected,`${path}: ${await response.clone().text()}`)
    return response.json()
  }
  let doc=await call('workflows/01-hello','PUT',JSON.parse(readFileSync('examples/workflows/01-hello.json','utf8')))
  const omitted={...doc,name:'An unversioned write must fail'}
  delete omitted.version
  await call('workflows/01-hello','PUT',omitted,428)
  await call('workflows/01-hello','PUT',{...omitted,version:'1'},400)
  await call('workflows/01-hello','PUT',{...omitted,version:0},409)
  let workspace=await call('workspace')
  assert.equal(workspace.apiVersion,2)
  assert.equal(workspace.editPreconditionsRequired,true)
  assert.equal(workspace.workflows[0].name,doc.name)
  doc=await call('workflows/01-hello','PUT',{...doc,name:'A versioned update'})
  assert.equal(doc.version,2)
  await call('workflows/01-hello/publish','POST',{},428)
  await call('workflows/01-hello/publish','POST',{expectedVersion:'2'},400)
  await call('workflows/01-hello/publish','POST',{expectedVersion:1},409)
  doc=await call('workflows/01-hello/publish','POST',{expectedVersion:doc.version})
  await call('runs','POST',{workflowId:doc.id,mode:'simulation',input:{}},428)
  assert.equal((await call('workspace')).runs.length,0)
  const run=await call('runs','POST',{workflowId:doc.id,mode:'simulation',input:{},expectedVersion:doc.version},201)
  assert.equal(run.status,'succeeded')
  console.log('Passed: HTTP API v2 creation, mandatory numeric edit versions, omitted/stale write rejection and execution preconditions.')
}
