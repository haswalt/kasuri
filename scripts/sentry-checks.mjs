import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { setTimeout as sleep } from 'node:timers/promises'
import { versionedTestClient } from './versioned-test-client.mjs'

export async function checkSentry(origin, restart) {
  const authorization = 'Basic ' + Buffer.from('test:runtime-check').toString('base64')
  function client() {
    let cookie = ''
    const versions = versionedTestClient()
    return {
      workspaceId: () => cookie.split('=')[1].split('.')[0],
      async api(path, method = 'GET', body) {
        const prepared = versions.prepare(path, method, body)
        const response = await fetch(origin + '/api/' + path, {
          method,
          headers: { Authorization: authorization, cookie, ...(prepared ? { 'Content-Type': 'application/json' } : {}) },
          body: JSON.stringify(prepared)
        })
        cookie = response.headers.get('set-cookie')?.split(';')[0] ?? cookie
        assert.ok(response.ok, `${path}: ${response.status} ${await response.clone().text()}`)
        return versions.remember(await response.json())
      }
    }
  }
  const allowed = client()
  const other = client()
  await allowed.api('workspace')
  await other.api('workspace')
  const example = JSON.parse(readFileSync('examples/workflows/39-sentry-error-report.json', 'utf8'))
  async function run(api, mode) {
    let record = await api('runs', 'POST', { workflowId: example.id, mode, input: {} })
    for (let i = 0; record.status === 'running' && i < 60; i++) {
      await sleep(50)
      record = await api('runs/' + record.id)
    }
    return record
  }
  const report = result => result.nodes.find(node => node.id === 'report')
  await allowed.api('workflows/' + example.id, 'PUT', example)
  assert.deepEqual(report(await run(allowed.api, 'simulation')).output, { status: 'preview', eventId: null })
  const profile = { dsn: 'https://fixture@sentry-fixture.example/100', workspaces: [allowed.workspaceId()] }
  await restart({ SENTRY_PROJECTS: JSON.stringify({ 'example-errors': profile }) })
  assert.deepEqual(await allowed.api('sentry/projects'), { projects: ['example-errors'] })
  assert.deepEqual(await other.api('sentry/projects'), { projects: [] })
  const delivered = await run(allowed.api, 'run')
  assert.equal(delivered.status, 'succeeded')
  assert.match(report(delivered).output.eventId, /^[a-f0-9]{32}$/)
  assert.equal(report(delivered).output.status, 'accepted')
  assert.ok(!JSON.stringify(delivered).includes(profile.dsn))
  assert.ok(!JSON.stringify(delivered).includes('privateFixtureResponse'))
  await other.api('workflows/' + example.id, 'PUT', example)
  const denied = await run(other.api, 'run')
  assert.equal(denied.status, 'failed')
  assert.equal(report(denied).error, 'egress_denied')
  console.log('Passed: simulated Sentry preview, workspace project discovery, real envelope delivery to a fixture and cross-workspace denial.')
}
