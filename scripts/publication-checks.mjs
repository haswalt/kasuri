import { versionedTestClient } from './versioned-test-client.mjs'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
export async function checkPublications(origin, credentials, restart) {
  let cookie = ''
  const Authorization = 'Basic ' + Buffer.from(credentials).toString('base64')
  const versionClient=versionedTestClient()
  async function api(path, method='GET', body, expected=200) {
    body=versionClient.prepare(path,method,body)
    const response = await fetch(origin + '/api/' + path,{method,headers:{Authorization,cookie,...(body===undefined?{}:{'Content-Type':'application/json'})},...(body===undefined?{}:{body:JSON.stringify(body)})})
    if(response.headers.has('set-cookie')) cookie=response.headers.get('set-cookie').split(';')[0]
    assert.equal(response.status,expected,`${path}: ${await response.clone().text()}`)
    return versionClient.remember(await response.json())
  }
  const fixture=JSON.parse(readFileSync('examples/workflows/01-hello.json','utf8'))
  fixture.id='publication-check'
  fixture.nodes[0].type='Webhook'
  fixture.nodes[0].config={authentication:'bearer'}
  let draft=await api('workflows/publication-check','PUT',fixture)
  const publish=()=>api('workflows/publication-check/publish','POST',{expectedVersion:draft.version})
  draft=await publish()
  const endpoint=await api('workflows/publication-check/webhook/rotate','POST')
  const deliver=event=>fetch(origin+endpoint.path,{method:'POST',headers:{Authorization:'Bearer '+endpoint.token,'Content-Type':'application/json','X-Kasuri-Event-Id':event},body:'{}'})
  draft.nodes[0].config={authentication:'hmac',signingSecret:'incoming_key'}
  draft.nodes[1].config.mapping.message='Unpublished draft output'
  draft=await api('workflows/publication-check','PUT',draft)
  assert.equal(draft.status,'active')
  assert.equal(draft.hasUnpublishedChanges,true)
  assert.equal((await api('workflows/publication-check/webhook')).authentication,'bearer')
  origin=await restart()||origin
  const delivered=await deliver('live-after-draft')
  assert.equal(delivered.status,202)
  const run=await api('runs/'+(await delivered.json()).runId)
  assert.equal(run.revision,1)
  assert.equal(run.nodes.find(n=>n.id==='hello').output.message,'Hello from Kasuri')
  draft=await api('workflows/publication-check/pause','POST',{expectedVersion:draft.version})
  assert.equal((await deliver('paused')).status,404)
  draft=await api('workflows/publication-check/revisions/1/restore','POST',{expectedVersion:draft.version})
  assert.equal(draft.status,'paused')
  assert.equal(draft.hasUnpublishedChanges,false)
  const unchanged=await publish()
  assert.equal(unchanged.revision,1)
  assert.equal(unchanged.version,draft.version)
  assert.equal(unchanged.status,'paused')
  draft=await api('workflows/publication-check/resume','POST',{expectedVersion:draft.version})
  draft.name='Second live name'
  draft=await api('workflows/publication-check','PUT',draft)
  draft=await publish()
  assert.equal(draft.revision,2)
  draft=await api('workflows/publication-check/revisions/1/restore','POST',{expectedVersion:draft.version})
  assert.equal(draft.revision,2)
  assert.equal(draft.lifecycle.publishedRevision,2)
  draft=await publish()
  assert.equal(draft.revision,3)
  origin=await restart()||origin
  const history=await api('workflows/publication-check/revisions')
  assert.deepEqual(history.revisions.map(r=>r.revision),[3,2,1])
  assert.equal(history.revisions[1].graph.name,'Second live name')
  assert.equal(history.revisions[0].publishedBy,'browser')
  await api('workflows/publication-check/pause','POST',{expectedVersion:draft.version})
  console.log('Passed: immutable live webhook auth/graphs, active draft saves, unchanged paused publication, retained history and monotonic rollback across restarts.')
}
