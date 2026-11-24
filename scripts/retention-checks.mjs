import assert from 'node:assert/strict'
import { versionedTestClient } from './versioned-test-client.mjs'

// With RUN_HISTORY=3, each workflow keeps its three newest finished runs; an
// invalid setting fails requests instead of keeping a different amount.
export async function checkRetention(origin, credentials, restartWith) {
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
  await restartWith({ RUN_HISTORY: '3' })
  const flow = id => ({ schemaVersion: 1, id, name: id, description: '', status: 'draft', revision: 0, updatedAt: '', nodes: [{ id: 'start', type: 'Manual Trigger', label: 'Start', config: {} }], edges: [] })
  await api('workspace')
  for (const id of ['keep-a', 'keep-b']) await api('workflows/' + id, 'PUT', flow(id))
  const started = []
  for (let i = 0; i < 5; i++) {
    for (const id of ['keep-a', 'keep-b']) started.push((await api('runs', 'POST', { workflowId: id, mode: 'run', input: { i } })).id)
  }
  const listed = (await api('workspace')).runs.map(r => r.id)
  assert.deepEqual(listed, started.slice(-6).reverse(), 'the newest three runs of each workflow')
  assert.equal((await call('runs/' + started[0])).status, 404, 'older runs are deleted')
  assert.equal((await api('runs/' + started.at(-1))).status, 'succeeded')

  await restartWith({ RUN_HISTORY: 'three' })
  assert.ok((await call('workspace')).status >= 500, 'an invalid RUN_HISTORY is refused')
  await restartWith({})
  console.log('Passed: RUN_HISTORY keeps the newest finished runs per workflow, older ones are deleted, and invalid settings are refused.')
}
