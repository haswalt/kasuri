import { env, runDurableObjectAlarm, runInDurableObject } from 'cloudflare:test'
import { describe, expect, it } from 'vitest'
import worker from '../src/index'
import type { Run, Workspace } from '../../../packages/contracts/src/index'
const context = { waitUntil() {}, passThroughOnException() {}, props: {} }
async function session() {
  const response = await worker.fetch(new Request('https://example.test/api/workspace'), env, context)
  expect(response.status).toBe(200)
  return { cookie: response.headers.get('set-cookie')!.split(';')[0], data: await response.json() as Workspace }
}
async function request(cookie: string, path: string, method = 'GET', body?: unknown) {
  return worker.fetch(new Request(`https://example.test/api/${path}`, { method, headers: { cookie, 'Content-Type': 'application/json' }, ...(body !== undefined ? { body: JSON.stringify(body) } : {}) }), env, context)
}
describe('Rust-backed demo API', () => {
  it('provides isolated browser workspaces and nine implemented connectors', async () => {
    const first = await session(); const second = await session()
    expect(first.cookie).not.toBe(second.cookie)
    expect(first.data.connectors).toHaveLength(9)
    const changed = { ...first.data.workflows[0], name: 'Only in workspace one' }
    expect((await request(first.cookie, `workflows/${changed.id}`, 'PUT', changed)).status).toBe(200)
    const other = await (await request(second.cookie, 'workspace')).json() as Workspace
    expect(other.workflows[0].name).not.toBe(changed.name)
  })
  it('simulates actual mapping and JSON nodes with virtual waits', async () => {
    const { cookie } = await session()
    const response = await request(cookie, 'runs', 'POST', { workflowId: 'core-data-demo', mode: 'simulation', input: { name: 'Ada' } })
    expect(response.status).toBe(201)
    const run = await response.json() as Run
    expect(run.status).toBe('succeeded')
    expect(run.nodes.find(n => n.id === 'parse')?.output).toEqual({ name: 'Ada', source: 'kasuri' })
    const stored = await (await request(cookie, `runs/${run.id}`)).json() as Run
    expect(stored).toEqual(run)
  })
  it('persists a real wait and resumes from an alarm without re-running completed nodes', async () => {
    const { cookie } = await session()
    const response = await request(cookie, 'runs', 'POST', { workflowId: 'core-data-demo', mode: 'run', input: { name: 'Grace' } })
    const run = await response.json() as Run
    expect(run.status).toBe('running'); expect(run.nodes.find(n => n.id === 'wait')?.status).toBe('waiting')
    const stub = env.WORKSPACES.getByName(cookie.split('=')[1])
    // Drive the persisted wake timestamp without sleeping or relying on a process-local timer.
    await runInDurableObject(stub, async (_instance, state) => {
      const row = state.storage.sql.exec<{ id: string; checkpoint: string }>('SELECT id,checkpoint FROM runs WHERE id=?', run.id).one()
      const checkpoint = JSON.parse(row.checkpoint)
      checkpoint.nodes.wait.until_ms = Date.now() - 1
      state.storage.sql.exec('UPDATE runs SET checkpoint=?,wake_at=? WHERE id=?', JSON.stringify(checkpoint), Date.now() - 1, row.id)
    })
    await runDurableObjectAlarm(stub)
    const complete = await (await request(cookie, `runs/${run.id}`)).json() as Run
    expect(complete.status).toBe('succeeded')
    expect(complete.nodes.find(n => n.id === 'map')?.attempts).toBe(1)
    expect(complete.nodes.find(n => n.id === 'wait')?.attempts).toBe(1)
  })
  it('reports missing-input failures and rejects unimplemented connectors', async () => {
    const { cookie, data } = await session()
    const failure = await (await request(cookie, 'runs', 'POST', { workflowId: 'core-data-demo', mode: 'simulation', input: {} })).json() as Run
    expect(failure.status).toBe('failed'); expect(failure.error).toBe('missing_input')
    const unsupported = { ...data.workflows[0], id: 'unsupported', nodes: [{ id: 'start', type: 'Manual Trigger', label: 'Start', config: {} }, { id: 'http', type: 'HTTP Request', label: 'HTTP', config: {} }], edges: [{ id: 'e', source: 'start', target: 'http' }] }
    await request(cookie, 'workflows/unsupported', 'PUT', unsupported)
    const rejected = await request(cookie, 'runs', 'POST', { workflowId: 'unsupported', mode: 'run', input: {} })
    expect(rejected.status).toBe(400)
    expect(await rejected.text()).toContain('not implemented')
  })
  it('rejects cross-origin changes and requires a session', async () => {
    expect((await worker.fetch(new Request('https://example.test/api/runs', { method: 'POST', headers: { origin: 'https://evil.test' } }), env, context)).status).toBe(403)
    expect((await request('', 'runs', 'POST', {})).status).toBe(401)
  })
  it('publishes validated revisions and supports run cancellation', async () => {
    const { cookie } = await session()
    const published = await request(cookie, 'workflows/core-data-demo/publish', 'POST')
    expect(published.status).toBe(200)
    expect((await published.json() as { revision: number }).revision).toBe(1)
    const run = await (await request(cookie, 'runs', 'POST', { workflowId: 'core-data-demo', mode: 'run', input: { name: 'Ada' } })).json() as Run
    const cancelled = await (await request(cookie, `runs/${run.id}/cancel`, 'POST')).json() as Run
    expect(cancelled.status).toBe('cancelled')
  })
})
