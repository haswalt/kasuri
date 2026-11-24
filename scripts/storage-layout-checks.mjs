import assert from 'node:assert/strict'
import { versionedTestClient } from './versioned-test-client.mjs'
import { healthy, sleep } from './runtime-checks.mjs'

// A workspace moves between the generation and table layouts when
// WORKSPACE_STORAGE changes, in both directions, without losing workflows, runs,
// webhooks or pending waits. `restartWith(layout)` restarts workerd with that
// WORKSPACE_STORAGE value on the same storage directory.
export async function checkStorageLayouts(origin, credentials, restartWith) {
  let cookie = ''
  const versions = versionedTestClient()
  const Authorization = 'Basic ' + Buffer.from(credentials).toString('base64')
  async function call(path, method = 'GET', body) {
    body = versions.prepare(path, method, body)
    const r = await fetch(origin + '/api/' + path, { method, headers: { Authorization, cookie, ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) })
    if (r.headers.has('set-cookie')) cookie = r.headers.get('set-cookie').split(';')[0]
    const text = await r.text()
    let value
    try { value = JSON.parse(text) } catch { value = { error: text } }
    return { status: r.status, value: r.ok ? versions.remember(value) : value }
  }
  async function api(path, method, body) {
    const { status, value } = await call(path, method, body)
    assert.ok(status < 300, `${method || 'GET'} ${path}: ${status} ${JSON.stringify(value)}`)
    return value
  }
  async function restart(layout) {
    await restartWith(layout)
    await healthy(origin)
  }
  async function finished(id) {
    let run = await api('runs/' + id)
    for (let i = 0; i < 100 && run.status === 'running'; i++) {
      await sleep(100)
      run = await api('runs/' + id)
    }
    return run
  }
  const held = { schemaVersion: 1, id: 'layout-wait', name: 'Layout wait', description: '', status: 'draft', revision: 0, updatedAt: '', nodes: [{ id: 'start', type: 'Manual Trigger', label: 'Start', config: {} }, { id: 'wait', type: 'Wait / Delay', label: 'Hold', config: { delayMs: 2500 } }], edges: [{ id: 'e', source: 'start', target: 'wait' }] }
  const hook = { schemaVersion: 1, id: 'layout-hook', name: 'Layout hook', description: '', status: 'draft', revision: 0, updatedAt: '', nodes: [{ id: 'start', type: 'Webhook', label: 'Incoming', config: { authentication: 'bearer' } }], edges: [] }
  const deliver = (endpoint, token = endpoint.token) => fetch(origin + endpoint.path, { method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + token }, body: '{"layout":true}' })

  // Generations: a workflow, a waiting run and a published webhook.
  await api('workspace')
  await api('workflows/' + held.id, 'PUT', held)
  await api('workflows/' + hook.id, 'PUT', hook)
  const endpoint = await api('workflows/' + hook.id + '/webhook/rotate', 'POST')
  await api('workflows/' + hook.id + '/publish', 'POST')
  const waiting = (await api('runs', 'POST', { workflowId: held.id, mode: 'run', input: { at: 'generations' } })).id
  const before = await api('workspace')

  // Into tables: the workspace is unchanged and keeps working.
  await restart('tables')
  const migrated = await api('workspace')
  assert.deepEqual(migrated.workflows.map(w => w.id).sort(), before.workflows.map(w => w.id).sort(), 'workflows survive the move to tables')
  assert.deepEqual(migrated.runs.map(r => r.id), before.runs.map(r => r.id), 'runs keep their order')
  assert.equal((await deliver(endpoint, 'wrong')).status, 401, 'webhook credentials are still checked')
  const delivered = await deliver(endpoint)
  assert.equal(delivered.status, 202)
  const hookRun = (await delivered.json()).runId
  assert.equal((await finished(hookRun)).status, 'succeeded')
  assert.equal((await finished(waiting)).status, 'succeeded', 'a wait armed before the move completes after it')
  const second = (await api('runs', 'POST', { workflowId: held.id, mode: 'run', input: { at: 'tables' } })).id
  held.name = 'Layout wait (tables)'
  await api('workflows/' + held.id, 'PUT', held)
  const inTables = await api('workspace')

  // A restart in the same layout reads the same rows.
  await restart('tables')
  assert.deepEqual((await api('workspace')).runs.map(r => r.id), inTables.runs.map(r => r.id))

  // Back to generations: everything written in tables is still there.
  await restart('generations')
  const back = await api('workspace')
  assert.equal(back.workflows.find(w => w.id === held.id).name, 'Layout wait (tables)', 'edits made in tables survive the move back')
  assert.deepEqual(back.runs.map(r => r.id), inTables.runs.map(r => r.id))
  assert.equal((await finished(second)).status, 'succeeded', 'a wait armed in tables completes in generations')
  assert.equal((await deliver(endpoint)).status, 202)

  // And forward again, to show a second migration starts from a clean slate.
  await restart('tables')
  const again = await api('workspace')
  assert.ok(again.runs.length >= back.runs.length)
  assert.equal(again.workflows.find(w => w.id === held.id).name, 'Layout wait (tables)')

  // An unknown layout fails requests rather than silently picking one.
  await restart('table')
  const refused = await call('workspace')
  assert.ok(refused.status >= 500, `an invalid WORKSPACE_STORAGE is refused: ${refused.status}`)
  await restart('')
  assert.equal((await api('workspace')).workflows.find(w => w.id === held.id).name, 'Layout wait (tables)')
  console.log('Passed: workspaces move between generation and table storage in both directions, with runs, waits and webhooks intact.')
}
