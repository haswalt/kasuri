import assert from 'node:assert/strict'
import { createHash, randomBytes } from 'node:crypto'
import { readFileSync } from 'node:fs'

const boundedFetch = (url, init = {}) =>
  fetch(url, { ...init, signal: AbortSignal.timeout(10000) })

export async function checkMcp(origin, credentials, restart) {
  let cookie = ''
  const Authorization = 'Basic ' + Buffer.from(credentials).toString('base64')
  async function owner(path, method = 'GET', body, status = 200) {
    const response = await boundedFetch(origin + '/api/' + path, {
      method,
      headers: {
        Authorization,
        cookie,
        ...(body ? { 'Content-Type': 'application/json' } : {})
      },
      ...(body ? { body: JSON.stringify(body) } : {})
    })
    if (response.headers.has('set-cookie'))
      cookie = response.headers.get('set-cookie').split(';')[0]
    assert.equal(
      response.status,
      status,
      `${path}: ${await response.clone().text()}`
    )
    return response.json()
  }
  const status = await owner('mcp')
  const endpoint = status.endpoint
  assert.ok(endpoint.startsWith(origin + '/mcp/'))
  assert.ok(endpoint.endsWith('/mcp'))
  const anonymous = await boundedFetch(endpoint, { headers: { cookie } })
  assert.equal(anonymous.status, 401)
  const metadata = anonymous.headers
    .get('www-authenticate')
    .match(/resource_metadata="([^"]+)"/)[1]
  const resource = await (await boundedFetch(metadata)).json()
  assert.equal(resource.resource, endpoint)
  const discovery = await (
    await boundedFetch(
      origin +
        '/.well-known/oauth-authorization-server' +
        new URL(resource.authorization_servers[0]).pathname
    )
  ).json()
  assert.equal(discovery.issuer, status.issuer)
  const builder = await owner('mcp/credentials', 'POST', {
    name: 'Runtime builder',
    role: 'builder',
    expiresDays: 1
  })
  const reader = await owner('mcp/credentials', 'POST', {
    name: 'Runtime reader',
    role: 'reader',
    expiresDays: 1
  })
  assert.equal('token' in (await owner('mcp')).credentials[0], false)
  let sequence = 0
  async function rpc(token, method, params = {}, expected = 200, headers = {}) {
    const response = await boundedFetch(endpoint, {
      method: 'POST',
      headers: {
        Authorization: 'Bearer ' + token,
        'Content-Type': 'application/json',
        Accept: 'application/json, text/event-stream',
        ...headers
      },
      body: JSON.stringify({ jsonrpc: '2.0', id: ++sequence, method, params })
    })
    assert.equal(response.status, expected)
    return response.json()
  }
  const init = await rpc(builder.token, 'initialize', {
    protocolVersion: '2025-11-25',
    capabilities: {},
    clientInfo: { name: 'Runtime tests', version: '1' }
  })
  assert.equal(init.result.protocolVersion, '2025-11-25')
  const listed = (await rpc(reader.token, 'tools/list')).result.tools
  assert.ok(listed.some((t) => t.name === 'audit_list'))
  assert.ok(!listed.some((t) => t.name === 'workflows_create'))
  assert.equal(
    (await rpc(builder.token, 'tools/list')).result.tools.some(
      (t) => t.name === 'runs_start'
    ),
    false
  )
  await rpc(builder.token, 'ping', {}, 403, {
    Origin: 'https://foreign.example'
  })
  const unsupported = await boundedFetch(endpoint, {
    headers: { Authorization: 'Bearer ' + builder.token }
  })
  assert.equal(unsupported.status, 405)
  const doc = JSON.parse(
    readFileSync('examples/workflows/01-hello.json', 'utf8')
  )
  const tool = async (token, name, args) =>
    (await rpc(token, 'tools/call', { name, arguments: args })).result
  const created = await tool(builder.token, 'workflows_create', {
    workflow: doc,
    idempotencyKey: 'create-example-1'
  })
  assert.equal(created.isError, false)
  const createdAgain = await tool(builder.token, 'workflows_create', {
    workflow: doc,
    idempotencyKey: 'create-example-1'
  })
  assert.deepEqual(createdAgain, created)
  assert.equal(
    (
      await tool(reader.token, 'workflows_update', {
        workflow: doc,
        expectedVersion: 1,
        idempotencyKey: 'reader-denied-1'
      })
    ).isError,
    true
  )
  const runArgs = {
    workflowId: doc.id,
    expectedVersion: 1,
    input: {},
    idempotencyKey: 'simulate-example-1'
  }
  const simulated = await tool(builder.token, 'runs_simulate', runArgs)
  assert.equal(simulated.isError, false)
  assert.equal(simulated.structuredContent.status, 'succeeded')
  if (restart) {
    origin = (await restart()) || origin
  }
  assert.deepEqual(
    await tool(builder.token, 'runs_simulate', runArgs),
    simulated
  )
  // Full Operator tool paths, including publication and real durable run control.
  const operator = await owner('mcp/credentials', 'POST', {
    name: 'Runtime operator',
    role: 'operator',
    expiresDays: 1
  })
  const controlled = JSON.parse(
    readFileSync('examples/workflows/04-branch-wait-merge.json', 'utf8')
  )
  controlled.id = 'mcp-control-check'
  controlled.nodes.find((n) => n.id === 'wait').config.delayMs = 10000
  assert.equal(
    (
      await tool(operator.token, 'workflows_create', {
        workflow: controlled,
        idempotencyKey: 'create-control-check'
      })
    ).isError,
    false
  )
  assert.equal(
    (
      await tool(operator.token, 'workflows_publish', {
        workflowId: controlled.id,
        expectedVersion: 1,
        idempotencyKey: 'publish-control-check'
      })
    ).structuredContent.version,
    2
  )
  assert.equal(
    (
      await tool(operator.token, 'workflows_pause', {
        workflowId: controlled.id,
        expectedVersion: 2,
        idempotencyKey: 'pause-control-workflow'
      })
    ).structuredContent.status,
    'paused'
  )
  assert.equal(
    (
      await tool(operator.token, 'workflows_resume', {
        workflowId: controlled.id,
        expectedVersion: 3,
        idempotencyKey: 'resume-control-workflow'
      })
    ).structuredContent.version,
    4
  )
  const real = await tool(operator.token, 'runs_start', {
    workflowId: controlled.id,
    expectedVersion: 4,
    input: {},
    idempotencyKey: 'start-control-run'
  })
  assert.equal(real.structuredContent.status, 'running')
  const runId = real.structuredContent.id
  assert.equal(
    (
      await tool(operator.token, 'runs_pause', {
        runId,
        idempotencyKey: 'pause-control-run'
      })
    ).structuredContent.status,
    'paused'
  )
  assert.equal(
    (
      await tool(operator.token, 'runs_resume', {
        runId,
        idempotencyKey: 'resume-control-run'
      })
    ).structuredContent.status,
    'running'
  )
  assert.equal(
    (
      await tool(operator.token, 'runs_cancel', {
        runId,
        idempotencyKey: 'cancel-control-run'
      })
    ).structuredContent.status,
    'cancelled'
  )
  // HTTP simulation must use its embedded fixture through the MCP interface too.
  const httpExample = JSON.parse(
    readFileSync('examples/workflows/06-public-http.json', 'utf8')
  )
  await tool(builder.token, 'workflows_create', {
    workflow: httpExample,
    idempotencyKey: 'create-http-simulation'
  })
  const httpRun = await tool(builder.token, 'runs_simulate', {
    workflowId: httpExample.id,
    expectedVersion: 1,
    input: {},
    idempotencyKey: 'start-http-simulation'
  })
  assert.equal(httpRun.structuredContent.status, 'succeeded')
  const details = await tool(reader.token, 'runs_get', {
    runId: httpRun.structuredContent.id
  })
  assert.deepEqual(
    details.structuredContent.nodes.find((n) => n.id === 'result').output,
    { postId: 1, userId: 1 }
  )
  const get = await tool(builder.token, 'workflows_get', { workflowId: doc.id })
  const latest = get.structuredContent
  await owner('workflows/' + doc.id, 'PUT', {
    ...latest,
    name: 'Browser update'
  })
  assert.equal(
    (
      await tool(builder.token, 'workflows_update', {
        workflow: doc,
        expectedVersion: 1,
        idempotencyKey: 'stale-update-1'
      })
    ).structuredContent.code,
    'conflict'
  )
  const audit = await owner('audit')
  assert.ok(audit.items.some((e) => e.actor === builder.id))
  assert.ok(audit.items.some((e) => e.outcome === 'forbidden'))
  assert.equal(JSON.stringify(audit).includes(builder.token), false)
  const cross = await boundedFetch(origin + '/api/mcp', {
    headers: { Authorization }
  })
  const other = await cross.json()
  assert.notEqual(other.endpoint, endpoint)
  assert.equal(
    (
      await boundedFetch(other.endpoint, {
        method: 'POST',
        headers: {
          Authorization: 'Bearer ' + builder.token,
          'Content-Type': 'application/json',
          Accept: 'application/json, text/event-stream'
        },
        body: '{"jsonrpc":"2.0","id":1,"method":"ping"}'
      })
    ).status,
    401
  )
  // OAuth discovery, DCR, owner approval, PKCE, audience binding and refresh rotation.
  async function oauth(url, values, status = 200, jsonBody = false) {
    const response = await boundedFetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': jsonBody
          ? 'application/json'
          : 'application/x-www-form-urlencoded'
      },
      body: jsonBody ? JSON.stringify(values) : new URLSearchParams(values)
    })
    assert.equal(response.status, status, await response.clone().text())
    return response.json()
  }
  const client = await oauth(
    discovery.registration_endpoint,
    {
      client_name: 'Runtime OAuth client',
      redirect_uris: ['http://127.0.0.1:3000/callback'],
      token_endpoint_auth_method: 'none'
    },
    201,
    true
  )
  const verifier = randomBytes(48).toString('base64url')
  const challenge = createHash('sha256').update(verifier).digest('base64url')
  const authUrl = new URL(discovery.authorization_endpoint)
  authUrl.search = new URLSearchParams({
    response_type: 'code',
    client_id: client.client_id,
    redirect_uri: client.redirect_uris[0],
    resource: endpoint,
    scope: 'read audit write simulate',
    state: 'runtime-client-state',
    code_challenge_method: 'S256',
    code_challenge: challenge
  }).toString()
  const authResponse = await boundedFetch(authUrl, { redirect: 'manual' })
  assert.equal(authResponse.status, 302)
  const requestId = new URL(
    authResponse.headers.get('location')
  ).searchParams.get('authorize')
  const request = await owner('mcp/oauth/' + requestId)
  assert.equal(request.resource, endpoint)
  const csrf = await boundedFetch(
    origin + '/api/mcp/oauth/' + requestId + '/approve',
    {
      method: 'POST',
      headers: {
        Authorization,
        cookie,
        Origin: 'https://foreign.example',
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ scopes: ['read'] })
    }
  )
  assert.equal(csrf.status, 403)
  const approval = await owner('mcp/oauth/' + requestId + '/approve', 'POST', {
    scopes: ['read', 'audit']
  })
  const callback = new URL(approval.redirect)
  assert.equal(callback.searchParams.get('state'), 'runtime-client-state')
  const exchange = {
    grant_type: 'authorization_code',
    client_id: client.client_id,
    redirect_uri: client.redirect_uris[0],
    resource: endpoint,
    code: callback.searchParams.get('code'),
    code_verifier: verifier
  }
  await oauth(
    discovery.token_endpoint,
    { ...exchange, code_verifier: 'bad' },
    400
  )
  await oauth(
    discovery.token_endpoint,
    { ...exchange, resource: 'https://foreign.example/mcp' },
    400
  )
  const access = await oauth(discovery.token_endpoint, exchange)
  assert.equal(access.scope, 'read audit')
  await oauth(discovery.token_endpoint, exchange, 400)
  assert.equal(
    (await rpc(access.access_token, 'tools/list')).result.tools.some(
      (t) => t.name === 'workflows_create'
    ),
    false
  )
  const refresh = {
    grant_type: 'refresh_token',
    client_id: client.client_id,
    resource: endpoint,
    refresh_token: access.refresh_token
  }
  const rotated = await oauth(discovery.token_endpoint, refresh)
  assert.notEqual(rotated.refresh_token, access.refresh_token)
  await oauth(discovery.token_endpoint, refresh, 400)
  await rpc(rotated.access_token, 'ping', {}, 401)
  await owner('mcp/credentials/' + builder.id, 'DELETE')
  await rpc(builder.token, 'ping', {}, 401)
  assert.equal(
    JSON.stringify(await owner('workspace')).includes('tokenHash'),
    false
  )
  console.log(
    'MCP checks passed: discovery, isolated bearer scopes, idempotency/restart, CAS, audit, OAuth consent/PKCE/audience, refresh replay and revocation.'
  )
}
