// Opt-in network check of the published, unmodified workflows against public APIs.
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { once } from 'node:events'
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { createServer } from 'node:net'
import { healthy, sleep } from './runtime-checks.mjs'

const storage = mkdtempSync(join(tmpdir(), 'kasuri-example-live-'))
const probe = createServer()
probe.listen(0, '127.0.0.1')
await once(probe, 'listening')
const port = probe.address().port
await new Promise(resolve => probe.close(resolve))
const origin = `http://127.0.0.1:${port}`
const child = spawn(resolve('node_modules/workerd/bin/workerd'), [
  'serve', 'workerd.capnp', '--socket-addr', `http=127.0.0.1:${port}`,
  '--directory-path', `storage=${storage}`
], { cwd: resolve('dist'), env: { ...process.env, BASIC_AUTH: '', PUBLIC_ORIGIN: '', HTTP_ALLOWED_HOSTS: '', HTTP_PRIVATE_HOSTS: '', HTTP_SECRETS: '{}' } })
let logs = '', cookie = ''
child.stdout.on('data', b => logs += b)
child.stderr.on('data', b => logs += b)
async function api(path, method = 'GET', body) {
  const response = await fetch(origin + '/api/' + path, {
    method, signal: AbortSignal.timeout(5000),
    headers: { cookie, ...(body ? { 'Content-Type': 'application/json' } : {}) },
    ...(body ? { body: JSON.stringify(body) } : {})
  })
  if (response.headers.has('set-cookie')) cookie = response.headers.get('set-cookie').split(';')[0]
  assert.ok(response.ok, `${path}: ${response.status} ${await response.clone().text()}`)
  return response.json()
}
try {
  await healthy(origin)
  const scenarios = JSON.parse(readFileSync('examples/scenarios.json', 'utf8'))
  for (const scenario of scenarios) {
    const workflow = JSON.parse(readFileSync(join('examples', scenario.file), 'utf8'))
    if (!workflow.nodes.some(n => n.type === 'HTTP Request')) continue
    await api('workflows/' + workflow.id, 'PUT', workflow)
    let run = await api('runs', 'POST', { workflowId: workflow.id, mode: 'run', input: {} })
    const deadline = Date.now() + 45000
    while (run.status === 'running' && Date.now() < deadline) {
      await sleep(200)
      run = await api('runs/' + run.id)
    }
    assert.equal(run.status, 'succeeded', `${scenario.file}: ${JSON.stringify(run)}`)
    const output = id => run.nodes.find(n => n.id === id).output
    assert.equal(output('fetch').status, 200)
    if (scenario.file.endsWith('06-public-http.json')) {
      assert.deepEqual(output('result'), scenario.outputs.result)
    } else {
      const records = output('decode')
      assert.ok(records.length > 0)
      assert.ok(records.every(n => n.completed === true && n.userId === 1))
      assert.deepEqual(records.map(n => n.id), records.map(n => n.id).sort((a,b) => a-b))
    }
    console.log(`Live public API passed: ${scenario.file}`)
  }
} catch (error) {
  console.error(logs)
  throw error
} finally {
  if (child.exitCode === null) {
    const done = once(child, 'exit')
    child.kill('SIGTERM')
    await done
  }
  rmSync(storage, { recursive: true, force: true })
}
