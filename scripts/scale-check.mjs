// Measures the stage 2 scale targets of docs/workspace-storage.md on workerd:
//
// - concurrency: N runs active at once in one workspace (each waits, then makes
//   an HTTP call), with step latency compared to one run on its own;
// - large state: runs carrying 50 KB inputs, which stay cancellable.
//
// Usage: pnpm build:selfhost, then
//   RUN_EXECUTION=objects node scripts/scale-check.mjs [--runs 1000] [--size 51200] [--wait 3000]
// It prints a JSON summary and fails if any run doesn't finish as expected, or if
// the median lateness is more than twice the single run's (plus 250 ms for timer
// and clock granularity). With --no-latency, only completion and cancellation are
// checked (CI machines' timing varies).
//
// workerd runs every Durable Object of a worker on one isolate, so these numbers
// measure one machine's CPU, not Cloudflare, where each run object is separate.
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:net'
import { once } from 'node:events'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { versionedTestClient } from './versioned-test-client.mjs'

const option = (name, fallback) => {
  const at = process.argv.indexOf('--' + name)
  return at > 0 ? Number(process.argv[at + 1]) : fallback
}
const RUNS = option('runs', 1000)
const SIZE = option('size', 0)
const WAIT = option('wait', 3000)
const CANCEL = option('cancel', 10)
const checkLatency = !process.argv.includes('--no-latency')
const mode = process.env.RUN_EXECUTION ?? ''
const queues = mode === 'queues'
const sleep = ms => new Promise(r => setTimeout(r, ms))

const data = mkdtempSync(join(tmpdir(), 'kasuri-scale-'))
const probe = createServer().listen(0, '127.0.0.1')
await once(probe, 'listening')
const port = probe.address().port
await new Promise(r => probe.close(r))
cpSync('dist/worker', join(data, 'worker'), { recursive: true })
cpSync('scripts/fixtures/http-upstream.mjs', join(data, 'upstream.mjs'))
const config = readFileSync(queues ? 'dist/workerd-queues.capnp' : 'dist/workerd.capnp', 'utf8')
  .replace('services = [', `services = [(name = "upstream", worker = (modules = [(name = "upstream.mjs", esModule = embed "upstream.mjs")], compatibilityDate = "2026-11-18")),`)
  .replace('compatibilityDate = "2026-11-18",', 'globalOutbound = "upstream", compatibilityDate = "2026-11-18",')
writeFileSync(join(data, 'scale.capnp'), config)
const origin = `http://127.0.0.1:${port}`
const child = spawn(resolve('node_modules/workerd/bin/workerd'), ['serve', join(data, 'scale.capnp'), '--socket-addr', `http=127.0.0.1:${port}`, '--directory-path', `storage=${data}`, ...(queues ? ['--experimental'] : [])], {
  cwd: resolve('dist'),
  env: { ...process.env, BASIC_AUTH: 'scale:check', PUBLIC_ORIGIN: '', HTTP_ALLOWED_HOSTS: '', HTTP_PRIVATE_HOSTS: '', MAX_ACTIVE_RUNS: '', RUN_HISTORY: String(RUNS + 100), RUN_HISTORY_DAYS: '', FAILED_RUN_HISTORY_DAYS: '', RUN_EXECUTION: mode, HTTP_SECRETS: '{}' }
})
let logs = ''
child.stdout.on('data', b => { logs += b })
child.stderr.on('data', b => { logs += b })

let cookie = ''
const versions = versionedTestClient()
const Authorization = 'Basic ' + Buffer.from('scale:check').toString('base64')
async function api(path, method = 'GET', body, expected = method === 'POST' && path === 'runs' ? 201 : 200) {
  body = versions.prepare(path, method, body)
  const r = await fetch(origin + '/api/' + path, { method, headers: { Authorization, cookie, ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) })
  if (r.headers.has('set-cookie')) cookie = r.headers.get('set-cookie').split(';')[0]
  const text = await r.text()
  assert.equal(r.status, expected, `${method} ${path}: ${text.slice(0, 300)}`)
  return versions.remember(JSON.parse(text))
}
// Run `work` over `items`, at most `limit` at a time.
async function pool(items, limit, work) {
  const results = new Array(items.length)
  let next = 0
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (next < items.length) {
      const index = next++
      results[index] = await work(items[index], index)
    }
  }))
  return results
}
const percentile = (values, p) => {
  const sorted = [...values].sort((a, b) => a - b)
  return sorted[Math.min(sorted.length - 1, Math.floor((sorted.length - 1) * p))]
}

try {
  for (let i = 0; i < 80; i++) { try { if ((await fetch(origin + '/api/health')).ok) break } catch {} await sleep(250) }
  await api('workspace')
  const workflow = { schemaVersion: 1, id: 'scale', name: 'Scale', description: '', status: 'draft', revision: 0, updatedAt: '', nodes: [{ id: 'start', type: 'Manual Trigger', label: 'Start', config: {} }, { id: 'wait', type: 'Wait / Delay', label: 'Wait', config: { delayMs: WAIT } }, { id: 'fetch', type: 'HTTP Request', label: 'Fetch', config: { url: 'https://fixture.example/clock' } }], edges: [{ id: 'a', source: 'start', target: 'wait' }, { id: 'b', source: 'wait', target: 'fetch' }] }
  await api('workflows/scale', 'PUT', workflow)
  const input = SIZE > 0 ? { payload: 'x'.repeat(SIZE) } : {}

  // Start runs, then poll each until it settles. A run's lateness is when its
  // HTTP step ran (the upstream's clock) beyond its wait, counted from when its
  // start was acknowledged: the time it took to step, not to be admitted.
  async function measure(count, cancel) {
    const started = await pool(Array.from({ length: count }), 25, async (_, index) => {
      const at = Date.now()
      const run = await api('runs', 'POST', { workflowId: 'scale', mode: 'run', input })
      assert.equal(run.status, 'running')
      const accepted = Date.now() - at
      // Some are cancelled while they wait.
      if (index < cancel) assert.equal((await api(`runs/${run.id}/cancel`, 'POST')).status, 'cancelled')
      return { id: run.id, at, accepted }
    })
    const cancelled = new Set(started.slice(0, cancel).map(r => r.id))
    const done = new Map()
    const deadline = Date.now() + WAIT + 120_000
    await sleep(WAIT)
    // Polling reads through the workspace too: keep it light.
    while (done.size < count && Date.now() < deadline) {
      const pending = started.filter(r => !done.has(r.id))
      await pool(pending, 10, async r => {
        const run = await api('runs/' + r.id)
        if (run.status !== 'running') done.set(r.id, { status: run.status, at: run.nodes.find(n => n.id === 'fetch')?.output?.body?.now })
      })
      if (done.size < count) await sleep(1000)
    }
    assert.equal(done.size, count, `${count - done.size} runs never settled`)
    for (const r of started) {
      assert.equal(done.get(r.id).status, cancelled.has(r.id) ? 'cancelled' : 'succeeded', r.id)
    }
    const late = started.filter(r => !cancelled.has(r.id)).map(r => done.get(r.id).at - (r.at + r.accepted) - WAIT)
    return { runs: count, cancelled: cancelled.size, acceptMedianMs: percentile(started.map(r => r.accepted), 0.5), acceptP95Ms: percentile(started.map(r => r.accepted), 0.95), latenessMedianMs: percentile(late, 0.5), latenessP95Ms: percentile(late, 0.95), latenessMaxMs: Math.max(...late) }
  }
  const single = await measure(1, 0)
  const many = await measure(RUNS, CANCEL)
  const summary = { mode: mode || 'workspace', inputBytes: SIZE, waitMs: WAIT, single, concurrent: many }
  console.log(JSON.stringify(summary, null, 2))
  const limit = 2 * Math.max(single.latenessMedianMs, 0) + 250
  if (checkLatency) {
    assert.ok(many.latenessMedianMs <= limit, `median lateness ${many.latenessMedianMs} ms exceeds ${limit} ms`)
  }
  console.log(`Passed (${summary.mode}): ${RUNS} concurrent runs (${SIZE} byte inputs) finished and ${CANCEL} were cancelled${checkLatency ? ', with median lateness within 2x of one run' : ''}.`)
} catch (error) {
  console.error(logs.split('\n').filter(l => !l.startsWith('Listening')).slice(-20).join('\n'))
  throw error
} finally {
  child.kill('SIGTERM')
  await once(child, 'exit').catch(() => {})
  rmSync(data, { recursive: true, force: true })
}
