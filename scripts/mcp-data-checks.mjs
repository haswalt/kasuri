import assert from 'node:assert/strict'
export async function checkMcpData(origin,credentials,restart) {
  let cookie='';let sequence=0
  const Authorization='Basic '+Buffer.from(credentials).toString('base64')
  async function owner(path,method='GET',body) {
    const response=await fetch(origin+'/api/'+path,{method,signal:AbortSignal.timeout(10000),headers:{Authorization,cookie,...(body===undefined?{}:{'Content-Type':'application/json'})},...(body===undefined?{}:{body:JSON.stringify(body)})})
    if(response.headers.has('set-cookie'))cookie=response.headers.get('set-cookie').split(';')[0]
    assert.equal(response.status,200,`${path}: ${await response.clone().text()}`)
    return response.json()
  }
  const status=await owner('mcp')
  const reader=await owner('mcp/credentials','POST',{name:'Existing reader',role:'reader',expiresDays:1})
  const legacy=await owner('mcp/credentials','POST',{name:'Existing manager permissions',scopes:['read','audit','write','simulate','publish','execute','control','delete'],expiresDays:1})
  const viewer=await owner('mcp/credentials','POST',{name:'Data viewer',role:'data_viewer',expiresDays:1})
  const editor=await owner('mcp/credentials','POST',{name:'Data editor',role:'data_editor',expiresDays:1})
  async function rpc(token,method,params={}) {
    const response=await fetch(status.endpoint,{method:'POST',signal:AbortSignal.timeout(10000),headers:{Authorization:'Bearer '+token,'Content-Type':'application/json',Accept:'application/json, text/event-stream'},body:JSON.stringify({jsonrpc:'2.0',id:++sequence,method,params})})
    assert.equal(response.status,200)
    return (await response.json()).result
  }
  const tool=(token,name,args)=>rpc(token,'tools/call',{name,arguments:args})
  assert.equal((await rpc(reader.token,'tools/list')).tools.some(t=>t.name.startsWith('data_')),false)
  assert.equal((await rpc(legacy.token,'tools/list')).tools.some(t=>t.name.startsWith('data_')),false)
  const viewerTools=(await rpc(viewer.token,'tools/list')).tools
  assert.ok(viewerTools.some(t=>t.name==='data_get'))
  assert.equal(viewerTools.some(t=>t.name==='data_apply'),false)
  const editorTools=(await rpc(editor.token,'tools/list')).tools
  const schema=editorTools.find(t=>t.name==='data_apply').inputSchema.properties.command
  assert.ok(schema.oneOf.some(v=>v.properties.operation.const==='patch'))
  assert.equal(schema.oneOf.some(v=>v.properties.operation.const==='get'),false)
  const create={command:{operation:'create_table',definition:{id:'agent-context',name:'Agent context',kind:'context'}},idempotencyKey:'agent-create-context'}
  assert.equal((await tool(viewer.token,'data_apply',create)).structuredContent.code,'forbidden')
  const created=await tool(editor.token,'data_apply',create)
  assert.equal(created.structuredContent.status,'created')
  assert.deepEqual(await tool(editor.token,'data_apply',create),created)
  assert.equal((await tool(reader.token,'data_table_get',{tableId:'agent-context'})).structuredContent.code,'forbidden')
  assert.equal((await tool(viewer.token,'data_table_get',{tableId:'agent-context'})).structuredContent.table.definition.kind,'context')
  const put={command:{operation:'put',tableId:'agent-context',key:'state',value:{count:1,token:'synthetic-agent-secret'}},idempotencyKey:'agent-context-write'}
  const stored=await tool(editor.token,'data_apply',put)
  assert.equal(stored.isError,false)
  assert.equal(JSON.stringify(stored).includes('synthetic-agent-secret'),false)
  const rowVersion=stored.structuredContent.rowVersion
  const read=await tool(viewer.token,'data_get',{tableId:'agent-context',key:'state'})
  assert.equal(read.structuredContent.row.value.token,'[REDACTED]')
  const placeholder=await tool(editor.token,'data_apply',{command:{operation:'put',tableId:'agent-context',key:'state',value:read.structuredContent.row.value},idempotencyKey:'agent-placeholder-reject'})
  assert.equal(placeholder.structuredContent.code,'invalid_arguments')
  const patch={command:{operation:'patch',tableId:'agent-context',key:'state',patch:{count:2},expectedVersion:rowVersion},idempotencyKey:'agent-known-field-patch'}
  const patched=await tool(editor.token,'data_apply',patch)
  assert.equal(patched.isError,false)
  const raw=await owner('data','POST',{command:{operation:'get',tableId:'agent-context',key:'state'}})
  assert.equal(raw.result.row.value.token,'synthetic-agent-secret')
  assert.equal(raw.result.row.value.count,2)
  const audit=(await owner('audit?order=desc')).items.filter(e=>e.action==='data.changed')
  assert.equal(audit.filter(e=>e.actor===editor.id).length,3)
  assert.equal(JSON.stringify(await owner('audit?order=desc')).includes('synthetic-agent-secret'),false)
  const failed=await tool(editor.token,'data_batch',{commands:[{operation:'put',tableId:'agent-context',key:'rollback',value:42},{operation:'patch',tableId:'agent-context',key:'rollback',patch:{count:1}}],idempotencyKey:'agent-rollback-batch'})
  assert.equal(failed.isError,true)
  assert.equal((await tool(viewer.token,'data_get',{tableId:'agent-context',key:'rollback'})).structuredContent.status,'not_found')
  const query=await tool(viewer.token,'data_query',{tableId:'agent-context',query:{limit:1,filters:[{pointer:'/count',operator:'eq',value:2}]}})
  assert.equal(query.structuredContent.rows.length,1)
  assert.equal(query.structuredContent.rows[0].value.token,'[REDACTED]')
  assert.equal((await tool(viewer.token,'data_tables_list',{limit:'bad'})).structuredContent.code,'invalid_arguments')
  assert.equal((await tool(viewer.token,'data_tables_list',{limit:4294967321})).structuredContent.code,'invalid_arguments')
  origin=await restart()||origin
  assert.deepEqual(await tool(editor.token,'data_apply',patch),patched)
  const after=(await owner('audit?order=desc')).items.filter(e=>e.action==='data.changed'&&e.actor===editor.id)
  assert.equal(after.length,3,'MCP receipt replay never duplicates data audit')
  assert.equal((await tool(viewer.token,'data_get',{tableId:'agent-context',key:'state'})).structuredContent.row.value.count,2)
  console.log('Passed: explicit MCP dataset scopes, unchanged existing grants, discoverable command schemas, redaction-safe patches, compact mutation receipts, transactional batch rollback and restart replay.')
}
