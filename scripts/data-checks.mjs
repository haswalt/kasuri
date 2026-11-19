import assert from 'node:assert/strict'
import { sleep } from './runtime-checks.mjs'
export async function checkData(origin,credentials,restart) {
  let cookie=''
  const Authorization='Basic '+Buffer.from(credentials).toString('base64')
  async function api(path,method='GET',body,expected=200,workspace='personal',cookieOverride) {
    const response=await fetch(origin+'/api/'+path,{method,signal:AbortSignal.timeout(10000),headers:{Authorization,cookie:cookieOverride??cookie,'X-Kasuri-Workspace':workspace,...(body===undefined?{}:{'Content-Type':'application/json'})},...(body===undefined?{}:{body:JSON.stringify(body)})})
    if(cookieOverride===undefined&&response.headers.has('set-cookie'))cookie=response.headers.get('set-cookie').split(';')[0]
    assert.equal(response.status,expected,`${path}: ${await response.clone().text()}`)
    return response.json()
  }
  const execute=(command,idempotencyKey,expected=200,workspace='personal')=>api('data','POST',{command,...(idempotencyKey?{idempotencyKey}:{})},expected,workspace)
  assert.deepEqual((await api('tables')).tables,[])
  await api('tables?limit=bad','GET',undefined,400)
  await api('tables?limit=0','GET',undefined,400)
  const definition={id:'events',name:'Events',kind:'table',fields:[{name:'active',kind:'boolean',required:true}]}
  const create={operation:'create_table',definition}
  await execute(create,undefined,400)
  const created=await execute(create,'create-events')
  assert.equal(created.result.status,'created')
  assert.equal((await execute(create,'create-events')).replayed,true)
  const listing=await api('tables')
  assert.equal(listing.tables[0].id,'events')
  assert.equal(listing.tables[0].fieldCount,1)
  const claim={operation:'insert_if_absent',tableId:'events',key:'claim',value:{active:true,marker:'synthetic-private-row-value'}}
  const contenders=await Promise.all(Array.from({length:12},(_,i)=>execute(claim,'claim-key-'+i)))
  assert.equal(contenders.filter(r=>r.result.inserted).length,1)
  assert.equal((await execute({operation:'get_table',tableId:'events'})).result.table.dataRevision,1)
  assert.equal(contenders.filter(r=>r.result.status==='existing').length,11)
  const winner=contenders.findIndex(r=>r.result.inserted)
  origin=await restart()||origin
  const replay=await execute(claim,'claim-key-'+winner)
  assert.equal(replay.replayed,true)
  assert.deepEqual(replay.result,contenders[winner].result)
  await execute({...claim,key:'another'},'claim-key-'+winner,409)
  assert.equal((await execute({operation:'get',tableId:'events',key:'another'})).result.status,'not_found')
  assert.equal(JSON.stringify(await api('workspace')).includes('synthetic-private-row-value'),false,'Dataset values stay outside workflow/run checkpoints')
  assert.equal(JSON.stringify(await api('audit?order=desc')).includes('synthetic-private-row-value'),false)
  const before=(await execute({operation:'get_table',tableId:'events'})).result.table.dataRevision
  await api('data','POST',{commands:[{operation:'put',tableId:'events',key:'partial',value:{active:true}},{operation:'put',tableId:'events',key:'invalid',value:{active:'wrong type'}}],idempotencyKey:'failed-batch'},400)
  assert.equal((await execute({operation:'get',tableId:'events',key:'partial'})).result.status,'not_found','Rust callback errors roll back staged SQLite writes')
  assert.equal((await execute({operation:'get_table',tableId:'events'})).result.table.dataRevision,before)
  const goodBatch={commands:[{operation:'put',tableId:'events',key:'partial',value:{active:true}}],idempotencyKey:'failed-batch'}
  assert.equal((await api('data','POST',goodBatch)).result.results[0].status,'stored','Rolled-back batches leave no success receipt')
  assert.equal((await api('data','POST',goodBatch)).replayed,true)
  const ttl={operation:'insert_if_absent',tableId:'events',key:'ttl',value:{active:true},ttlMs:100}
  const original=await execute(ttl,'ttl-original')
  await sleep(150)
  assert.equal((await execute({operation:'get',tableId:'events',key:'ttl'})).result.status,'not_found')
  const renewed=await execute(ttl,'ttl-new-event')
  assert.equal(renewed.result.inserted,true)
  assert.ok(renewed.result.row.version>original.result.row.version)
  assert.equal((await execute(ttl,'ttl-original')).result.row.version,original.result.row.version)
  assert.equal((await execute({operation:'put',tableId:'events',key:'ttl',value:{active:false},expectedVersion:original.result.row.version},'ttl-stale-cas')).result.status,'conflict')
  const context=await execute({operation:'create_table',definition:{id:'context',name:'Context',kind:'context'}},'create-context')
  assert.equal(context.result.status,'created')
  await execute({operation:'put',tableId:'context',key:'state',value:[1,'two',null]},'context-write')
  assert.deepEqual((await execute({operation:'get',tableId:'context',key:'state'})).result.row.value,[1,'two',null])
  await api('data','POST',{command:{operation:'get',tableId:'events',key:'claim'},scope:'other'},400)
  await execute({operation:'get',tableId:"events'; DROP TABLE data_rows;--",key:'claim'},undefined,400)
  const page=await execute({operation:'query',tableId:'events',query:{limit:1,filters:[{pointer:'/active',operator:'eq',value:true}]}})
  assert.equal(page.result.rows.length,1)
  assert.ok(page.result.nextCursor)
  const next=await execute({operation:'query',tableId:'events',query:{limit:1,cursor:page.result.nextCursor,filters:[{pointer:'/active',operator:'eq',value:true}]}})
  assert.notEqual(next.result.rows[0].key,page.result.rows[0].key)
  const named=await api('workspaces','POST',{name:'Data isolation'},201)
  await execute({operation:'get_table',tableId:'events'},undefined,404,named.id)
  const isolated=await execute(create,'create-events',200,named.id)
  assert.equal(isolated.result.status,'created')
  assert.equal((await execute({operation:'get',tableId:'events',key:'claim'},undefined,200,named.id)).result.status,'not_found')
  const stranger=await fetch(origin+'/api/workspace',{headers:{Authorization}})
  const strangerCookie=stranger.headers.get('set-cookie').split(';')[0]
  await api('data','POST',{command:{operation:'get_table',tableId:'events'}},404,named.id,strangerCookie)
  origin=await restart()||origin
  assert.equal((await execute({operation:'get',tableId:'events',key:'claim'})).result.row.value.marker,'synthetic-private-row-value')
  console.log('Passed: SQLite data persistence, competing atomic claims, TTL/CAS, durable receipts, Rust transaction rollback, bounded query cursors, checkpoint/audit exclusion and workspace isolation.')
}
