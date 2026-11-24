import assert from 'node:assert/strict'
import { DatabaseSync } from 'node:sqlite'
import { readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { versionedTestClient } from './versioned-test-client.mjs'
import { sleep } from './runtime-checks.mjs'

// With RUN_EXECUTION=objects or queues, a run that has to wait is executed by its
// own run object, stepped by its alarm or by queue messages: the workspace keeps
// only an index row, kept current by reports.
// `data` is workerd's storage directory, inspected directly to prove it.
export async function checkRunObjects(origin, credentials, data, mode, restart) {
  let cookie = ''
  const versions = versionedTestClient()
  const Authorization = 'Basic ' + Buffer.from(credentials).toString('base64')
  async function api(path, method = 'GET', body, expected = method === 'POST' && path === 'runs' ? 201 : 200) {
    body = versions.prepare(path, method, body)
    const r = await fetch(origin + '/api/' + path, { method, headers: { Authorization, cookie, ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) })
    if (r.headers.has('set-cookie')) cookie = r.headers.get('set-cookie').split(';')[0]
    const text = await r.text()
    assert.equal(r.status, expected, `${method} ${path}: ${text}`)
    return versions.remember(JSON.parse(text))
  }
  async function settled(id) {
    let run = await api('runs/' + id)
    for (let i = 0; i < 150 && run.status === 'running'; i++) { await sleep(100); run = await api('runs/' + id) }
    return run
  }
  // Every SQLite database workerd keeps, with the tables it has.
  function databases() {
    const found = []
    const walk = dir => {
      for (const name of readdirSync(dir)) {
        const path = join(dir, name)
        if (statSync(path).isDirectory()) walk(path)
        else if (name.endsWith('.sqlite')) found.push(path)
      }
    }
    walk(data)
    return found
  }
  function query(sql, ...params) {
    const rows = []
    for (const path of databases()) {
      const db = new DatabaseSync(path, { readOnly: true })
      try { rows.push(...db.prepare(sql).all(...params)) } catch { /* not this table */ } finally { db.close() }
    }
    return rows
  }
  const indexed = id => query('SELECT remote, status FROM ws_runs WHERE id=?', id)[0]
  const inObject = id => query('SELECT id FROM run WHERE id=?', id).length === 1

  const wait = { schemaVersion: 1, id: 'object-wait', name: 'Object wait', description: '', status: 'draft', revision: 0, updatedAt: '', nodes: [{ id: 'start', type: 'Manual Trigger', label: 'Start', config: {} }, { id: 'wait', type: 'Wait / Delay', label: 'Wait', config: { delayMs: 1500 } }, { id: 'fetch', type: 'HTTP Request', label: 'Fetch', config: { url: 'https://fixture.example/items' } }], edges: [{ id: 'a', source: 'start', target: 'wait' }, { id: 'b', source: 'wait', target: 'fetch' }] }
  const instant = { ...wait, id: 'object-instant', name: 'Object instant', nodes: [wait.nodes[0]], edges: [] }
  const hold = { ...wait, id: 'object-hold', name: 'Object hold', nodes: [wait.nodes[0], { ...wait.nodes[1], config: { delayMs: 60000 } }], edges: [wait.edges[0]] }
  await api('workspace')
  await api('workflows/' + wait.id, 'PUT', wait)
  await api('workflows/' + instant.id, 'PUT', instant)
  await api('workflows/' + hold.id, 'PUT', hold)

  // A run that finishes at once still answers with its result, from the workspace.
  const quick = await api('runs', 'POST', { workflowId: instant.id, mode: 'run', input: {} })
  assert.equal(quick.status, 'succeeded')
  assert.equal(indexed(quick.id)?.remote, 0, 'finished in the workspace')

  // A waiting run is handed to its run object.
  const started = await api('runs', 'POST', { workflowId: wait.id, mode: 'run', input: { n: 1 } })
  assert.equal(started.status, 'running')
  assert.equal(indexed(started.id)?.remote, 1, 'indexed as remote')
  assert.ok(inObject(started.id), 'stored in its run object')
  assert.equal((await api('workspace')).runs.find(r => r.id === started.id)?.status, 'running', 'listed')
  if (mode === 'queues') {
    // Its next step is a queued message, not a run object alarm.
    assert.ok(query('SELECT id FROM messages').length >= 1, 'a message is queued')
    const alarms = databases().filter(path => path.includes(join(data, 'runs')))
      .flatMap(path => { const db = new DatabaseSync(path, { readOnly: true }); try { return db.prepare('SELECT * FROM _cf_ALARM').all() } catch { return [] } finally { db.close() } })
    assert.equal(alarms.length, 0, 'no run object alarm')
  }

  // It survives a restart mid-wait, completes, and its run object is deleted.
  await restart()
  const done = await settled(started.id)
  assert.equal(done.status, 'succeeded', JSON.stringify(done))
  assert.equal(done.nodes.find(n => n.id === 'fetch').status, 'succeeded')
  for (let i = 0; i < 50 && inObject(started.id); i++) await sleep(100)
  assert.ok(!inObject(started.id), 'run object storage deleted once finished')
  assert.equal(indexed(started.id).status, 'succeeded', 'index has the final state')

  // Control requests reach the run object.
  const held = await api('runs', 'POST', { workflowId: hold.id, mode: 'run', input: { n: 2 } })
  const paused = await api(`runs/${held.id}/pause`, 'POST')
  assert.equal(paused.status, 'paused')
  for (let i = 0; i < 50 && (await api('runs/' + held.id)).status !== 'paused'; i++) await sleep(100)
  await sleep(500)
  assert.equal((await api('runs/' + held.id)).status, 'paused', 'stays paused')
  assert.equal((await api(`runs/${held.id}/resume`, 'POST')).status, 'running')
  assert.equal((await api(`runs/${held.id}/cancel`, 'POST')).status, 'cancelled')
  const cancelled = await settled(held.id)
  assert.equal(cancelled.status, 'cancelled')
  await api(`runs/${held.id}/cancel`, 'POST', undefined, 409)
  console.log(`Passed (${mode}): waiting runs execute in run objects, survive restarts, accept control requests and are deleted once finished; instant runs answer from the workspace.`)
}
