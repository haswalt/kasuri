import { env, runDurableObjectAlarm, runInDurableObject } from 'cloudflare:test'
import { expect, it } from 'vitest'
import { demoWorkflows } from '../src/demos'
import { scheduleFor } from '../src/schedule'
import type { Run, Workflow, Workspace } from '../../../packages/contracts/src/index'
import worker from '../src/index'
it('validates five-field cron and timezone and finds a timezone-aware next occurrence', () => {
  const w = demoWorkflows(0)[2]
  w.nodes[0].config = { cron: '0 9 * * *', timezone: 'America/New_York' }
  expect(scheduleFor(w, Date.parse('2026-11-18T00:00:00Z'))?.at).toBe(Date.parse('2026-11-18T14:00:00Z'))
  w.nodes[0].config.cron = '* * * * * *'
  expect(() => scheduleFor(w, Date.now())).toThrow('five-field')
  w.nodes[0].config = { cron: '* * * * *', timezone: 'Invalid/Zone' }
  expect(() => scheduleFor(w, Date.now())).toThrow('Invalid cron')
})
it('registers, fires once, advances and unregisters a persisted schedule', async () => {
  const response = await worker.fetch(new Request('https://example.test/api/workspace'), env)
  const cookie = response.headers.get('set-cookie')!.split(';')[0]
  const stub = env.WORKSPACES.getByName(cookie.split('=')[1])
  const published = await worker.fetch(new Request('https://example.test/api/workflows/scheduled-heartbeat/publish', { method: 'POST', headers: { cookie } }), env)
  const workflow = await published.json() as Workflow
  expect(workflow.status).toBe('active'); expect(workflow.nextScheduledAt).toBeGreaterThan(Date.now())
  await runInDurableObject(stub, async (_instance, state) => {
    state.storage.sql.exec('UPDATE workflows SET next_at=? WHERE id=?', Date.now() - 1000, workflow.id)
  })
  await runDurableObjectAlarm(stub)
  const first = await (await worker.fetch(new Request('https://example.test/api/workspace', { headers: { cookie } }), env)).json() as Workspace
  const runs = first.runs.filter((r: Run) => r.mode === 'schedule')
  expect(runs).toHaveLength(1); expect(runs[0].status).toBe('succeeded')
  expect(first.workflows.find(w => w.id === workflow.id)?.nextScheduledAt).toBeGreaterThan(Date.now())
  await runDurableObjectAlarm(stub)
  const repeated = await (await worker.fetch(new Request('https://example.test/api/workspace', { headers: { cookie } }), env)).json() as Workspace
  expect(repeated.runs.filter(r => r.mode === 'schedule')).toHaveLength(1)
  const pause = await worker.fetch(new Request(`https://example.test/api/workflows/${workflow.id}/pause`, { method: 'POST', headers: { cookie } }), env)
  const paused = await pause.json() as Workflow
  expect(paused.status).toBe('paused'); expect(paused.nextScheduledAt).toBeNull()
})
