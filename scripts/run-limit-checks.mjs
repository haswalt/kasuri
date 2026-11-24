import assert from 'node:assert/strict'
import { versionedTestClient } from './versioned-test-client.mjs'

// Concurrent runs are unlimited unless MAX_ACTIVE_RUNS sets a cap; each call uses a
// fresh workspace so earlier checks' runs never count against it.
export async function checkRunLimits(origin, credentials, limit) {
  let cookie = ''
  const versions = versionedTestClient()
  const Authorization = 'Basic ' + Buffer.from(credentials).toString('base64')
  async function api(path, method = 'GET', body) {
    body = versions.prepare(path, method, body)
    const r = await fetch(origin + '/api/' + path, { method, headers: { Authorization, cookie, ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) })
    if (r.headers.has('set-cookie')) cookie = r.headers.get('set-cookie').split(';')[0]
    const value = await r.json()
    return { status: r.status, value: r.ok ? versions.remember(value) : value }
  }
  const workflow = { schemaVersion: 1, id: 'run-limits', name: 'Run limits', description: '', status: 'draft', revision: 0, updatedAt: '', nodes: [{ id: 'start', type: 'Manual Trigger', label: 'Start', config: {} }, { id: 'wait', type: 'Wait / Delay', label: 'Hold', config: { delayMs: 60000 } }], edges: [{ id: 'e', source: 'start', target: 'wait' }] }
  assert.ok((await api('workspace')).status < 300)
  assert.ok((await api('workflows/' + workflow.id, 'PUT', workflow)).status < 300)
  // Every run waits a minute, so all of them stay active for the whole check.
  const attempts = limit === null ? 8 : limit + 1
  const statuses = []
  for (let i = 0; i < attempts; i++) {
    const started = await api('runs', 'POST', { workflowId: workflow.id, mode: 'run', input: { i } })
    statuses.push(started.status)
    if (started.status >= 300) assert.match(started.value.error ?? started.value.message ?? JSON.stringify(started.value), /active run/)
  }
  if (limit === null) {
    assert.ok(statuses.every(s => s < 300), `default allows ${attempts} concurrent runs: ${statuses}`)
  } else {
    assert.ok(statuses.slice(0, limit).every(s => s < 300), `first ${limit} runs start: ${statuses}`)
    assert.equal(statuses[limit], 429, `run ${limit + 1} is refused at MAX_ACTIVE_RUNS=${limit}`)
  }
  console.log(limit === null ? `Passed: ${attempts} concurrent runs with no configured cap.` : `Passed: MAX_ACTIVE_RUNS=${limit} admits ${limit} concurrent runs and refuses the next.`)
}
