import assert from 'node:assert/strict'
const origin = new URL(process.argv[2] || 'http://127.0.0.1:8787').origin
let cookie = ''
async function api(path, method = 'GET', body) {
  const response = await fetch(`${origin}/api/${path}`, { method, headers: { ...(cookie ? { cookie } : {}), ...(body === undefined ? {} : { 'Content-Type': 'application/json', Origin: origin }) }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) })
  if (response.headers.has('set-cookie')) cookie = response.headers.get('set-cookie').split(';')[0]
  const result = await response.json()
  assert.ok(response.ok, `${method} ${path}: ${response.status} ${result.error ?? ''}`)
  return result
}
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
assert.equal((await api('health')).engine, 'rust-wasm')
let workspace = await api('workspace')
assert.equal(workspace.connectors.length, 9)
const input = { name: 'Live demo', priority: 'urgent', title: 'End-to-end test' }
const simulation = await api('runs', 'POST', { workflowId: 'core-data-demo', mode: 'simulation', input })
assert.equal(simulation.status, 'succeeded')
assert.deepEqual(simulation.nodes.find(n => n.id === 'parse').output, { name: 'Live demo', source: 'kasuri' })
console.log('PASS: Rust simulation and node outputs')
let run = await api('runs', 'POST', { workflowId: 'core-data-demo', mode: 'run', input })
assert.equal(run.status, 'running')
assert.equal(run.nodes.find(n => n.id === 'wait').status, 'waiting')
for (let i = 0; i < 30 && run.status === 'running'; i++) { await sleep(500); run = await api(`runs/${run.id}`) }
assert.equal(run.status, 'succeeded')
assert.equal(run.nodes.find(n => n.id === 'wait').attempts, 1)
console.log('PASS: Real persisted wait resumed by alarm')
for (const priority of ['urgent', 'normal']) {
  const branch = await api('runs', 'POST', { workflowId: 'example-triage', mode: 'simulation', input: { ...input, priority } })
  assert.equal(branch.status, 'succeeded')
  assert.equal(branch.nodes.find(n => n.id === 'wait').status, priority === 'urgent' ? 'succeeded' : 'skipped')
}
console.log('PASS: Conditional routing and skipped branch')
const failed = await api('runs', 'POST', { workflowId: 'core-data-demo', mode: 'simulation', input: {} })
assert.equal(failed.status, 'failed'); assert.equal(failed.error, 'missing_input')
console.log('PASS: Core failure surfaced in run history')
try {
  const published = await api('workflows/scheduled-heartbeat/publish', 'POST')
  assert.ok(published.nextScheduledAt > Date.now())
  console.log(`Waiting for cron occurrence at ${new Date(published.nextScheduledAt).toISOString()}…`)
  let scheduled
  for (let i = 0; i < 75; i++) {
    await sleep(1000)
    workspace = await api('workspace')
    scheduled = workspace.runs.find(r => r.mode === 'schedule')
    if (scheduled) break
  }
  assert.ok(scheduled, 'Cron did not fire within 75 seconds')
  scheduled = await api(`runs/${scheduled.id}`)
  assert.equal(scheduled.status, 'succeeded')
  assert.equal(scheduled.nodes.find(n => n.id === 'map').output.source, 'cron')
  console.log('PASS: Published cron triggered a real backend run')
} finally {
  const paused = await api('workflows/scheduled-heartbeat/pause', 'POST')
  assert.equal(paused.nextScheduledAt, null)
}
console.log('PASS: Cron unregistered; smoke-test session is isolated and expires automatically')
