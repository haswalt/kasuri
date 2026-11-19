import { versionedTestClient } from './versioned-test-client.mjs'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
export async function checkAudit(origin,credentials) {
  let cookie=''
  const Authorization='Basic '+Buffer.from(credentials).toString('base64')
  const versionClient=versionedTestClient()
  async function api(path,method='GET',body) {
    body=versionClient.prepare(path,method,body)
    const response=await fetch(origin+'/api/'+path,{method,headers:{cookie,Authorization,...(body===undefined?{}:{'Content-Type':'application/json'})},...(body===undefined?{}:{body:JSON.stringify(body)})})
    if(response.headers.has('set-cookie'))cookie=response.headers.get('set-cookie').split(';')[0]
    assert.ok(response.ok,`${path}: ${response.status} ${await response.clone().text()}`)
    return versionClient.remember(await response.json())
  }
  const audit=()=>api('audit?order=desc&limit=100')
  let doc=await api('workflows/01-hello','PUT',JSON.parse(readFileSync('examples/workflows/01-hello.json','utf8')))
  assert.equal((await audit()).items.length,1)
  await api('workflows/01-hello','PUT',doc)
  assert.equal((await audit()).items.length,1)
  doc=await api('workflows/01-hello/publish','POST',{expectedVersion:doc.version})
  await api('workflows/01-hello/publish','POST',{expectedVersion:doc.version})
  await api('runs','POST',{workflowId:doc.id,mode:'simulation',input:{marker:'synthetic-private-payload'}})
  await api('runs','POST',{workflowId:doc.id,mode:'run',input:{}})
  assert.equal((await audit()).items.length,2)
  doc.nodes[1].config.mapping.message='synthetic-private-config'
  doc=await api('workflows/01-hello','PUT',doc)
  let page=await audit()
  assert.equal(page.items.length,3)
  assert.equal(page.items[0].action,'workflow.updated')
  assert.equal(page.items[0].changes[0].nodeId,'hello')
  assert.equal(JSON.stringify(page).includes('synthetic-private-config'),false)
  assert.equal(JSON.stringify(page).includes('synthetic-private-payload'),false)
  const status=await api('mcp')
  const builder=await api('mcp/credentials','POST',{name:'Audit builder',role:'builder',expiresDays:1})
  const call=async(name,args)=>{
    const response=await fetch(status.endpoint,{method:'POST',headers:{Authorization:'Bearer '+builder.token,'Content-Type':'application/json',Accept:'application/json, text/event-stream','MCP-Protocol-Version':'2025-11-25'},body:JSON.stringify({jsonrpc:'2.0',id:1,method:'tools/call',params:{name,arguments:args}})})
    assert.equal(response.status,200)
    return (await response.json()).result
  }
  const count=(await audit()).items.length
  await call('workflows_get',{workflowId:doc.id})
  await call('connectors_list',{})
  assert.equal((await audit()).items.length,count)
  doc.name='Audit renamed workflow'
  const updated=await call('workflows_update',{workflow:doc,expectedVersion:doc.version,idempotencyKey:'audit-single-write'})
  assert.equal(updated.isError,false)
  page=await audit()
  assert.equal(page.items.length,count+1,'MCP mutation has one semantic event, without a transport duplicate')
  assert.equal(page.items[0].actor,builder.id)
  assert.equal(page.items[0].entity.name,doc.name)
  assert.equal(page.items[0].action,'workflow.updated')
  assert.equal(JSON.stringify(page).includes(builder.token),false)
  assert.equal((await api('audit?order=desc&limit=1&before='+page.items[0].id)).items[0].id,page.items[1].id)
  console.log('Passed: contextual management-only audit, no-op/read/run exclusion, MCP single-event writes, secret exclusion and newest-first detail pagination.')
}
