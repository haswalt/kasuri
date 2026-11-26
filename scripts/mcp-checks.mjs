import assert from 'node:assert/strict'
import { createHash, randomBytes } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { setTimeout as delay } from 'node:timers/promises'

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
  assert.match(new URL(endpoint).pathname, /^\/mcp\/[a-f0-9]{64}$/)
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
  const clairic = {
    schemaVersion: 1, id: 'clairic-diagnostics', name: 'Clairic diagnostics', status: 'draft', updatedAt: '',
    nodes: [
      { id: 'start', type: 'Manual Trigger', label: 'Start', config: {} },
      { id: 'extract', type: 'Clairic', label: 'Extract', config: { fields: [{ name: 'name', type: 'text' }, { name: 'name', type: 'text' }] } },
      { id: 'recover', type: 'Error Handler', label: 'Recover', config: {} }
    ],
    edges: [
      { id: 'start-extract', source: 'start', target: 'extract' },
      { id: 'extract-recover', source: 'extract', sourcePort: 'error', target: 'recover' }
    ]
  }
  const validation = (await rpc(builder.token, 'tools/call', {
    name: 'workflows_validate', arguments: { workflow: clairic }
  })).result.structuredContent
  assert.equal(validation.valid, false)
  assert.ok(validation.errors.some(error => error.nodeId === 'extract' && error.path === '/nodes/1/config/fields/1/name' && error.message.includes('unique')))
  const invalidDraft = await owner('workflows/' + clairic.id, 'PUT', clairic)
  const rejected = await owner('workflows/' + clairic.id + '/publish', 'POST', { expectedVersion: invalidDraft.version }, 400)
  assert.equal(rejected.errors[0].path, '/nodes/1/config/fields/1/name')
  clairic.nodes[1].config.fields.pop()
  const updated = await owner('workflows/' + clairic.id, 'PUT', { ...clairic, version: invalidDraft.version })
  let clairicRun = await owner('runs', 'POST', { workflowId: clairic.id, expectedVersion: updated.version, mode: 'run', input: 'Synthetic enquiry' }, 201)
  for (let attempt = 0; attempt < 100 && clairicRun.status === 'running'; attempt++) {
    await delay(100)
    clairicRun = await owner('runs/' + clairicRun.id)
  }
  assert.equal(clairicRun.status, 'succeeded')
  const extraction = clairicRun.nodes.find(node => node.id === 'extract')
  assert.equal(extraction.status, 'error_handled')
  assert.equal(extraction.error, 'missing_binding')
  assert.match(extraction.errorDetail.message, /Workers AI binding named AI/)
  assert.equal(clairicRun.nodes.find(node => node.id === 'recover').output.error.detail.message, extraction.errorDetail.message)
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
  // Exercise the new internal nodes through the public MCP builder surface.
  for (const [file,id] of [['17-body-parsing.json','mcp-parser-example'],['15-schema-validation.json','mcp-schema-example'],['13-webhook-response.json','mcp-response-example']]) {
    const workflow=JSON.parse(readFileSync('examples/workflows/'+file,'utf8'));workflow.id=id
    const made=await tool(builder.token,'workflows_create',{workflow,idempotencyKey:'create-'+id});assert.equal(made.isError,false)
    const run=await tool(builder.token,'runs_simulate',{workflowId:id,expectedVersion:1,input:{},idempotencyKey:'simulate-'+id});assert.equal(run.isError,false);assert.equal(run.structuredContent.status,'succeeded')
  }
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
  const revisions = await tool(reader.token, 'workflows_revisions', {workflowId: controlled.id})
  assert.equal(revisions.isError,false)
  assert.equal(revisions.structuredContent.items[0].revision,1)
  const currentControl = (await tool(reader.token,'workflows_get',{workflowId:controlled.id})).structuredContent
  const restoreArgs = {workflowId:controlled.id,revision:1,expectedVersion:currentControl.version,idempotencyKey:'restore-control-revision'}
  assert.equal((await tool(reader.token,'workflows_restore_revision',restoreArgs)).isError,true)
  const restored = await tool(builder.token,'workflows_restore_revision',restoreArgs)
  assert.equal(restored.isError,false)
  assert.equal(restored.structuredContent.revision,1)
  assert.deepEqual(await tool(builder.token,'workflows_restore_revision',restoreArgs),restored)
  const afterRestore = (await tool(reader.token,'workflows_get',{workflowId:controlled.id})).structuredContent
  assert.equal(afterRestore.lifecycle.publishedRevision,1)
  assert.equal(afterRestore.lifecycle.enabled,true)
  const archiveArgs={workflowId:controlled.id,expectedVersion:afterRestore.version,idempotencyKey:'archive-control-workflow'}
  assert.equal((await tool(operator.token,'workflows_archive',archiveArgs)).isError,true,'Existing Operator permissions cannot delete workflows')
  const manager=await owner('mcp/credentials','POST',{name:'Runtime manager',role:'manager',expiresDays:1})
  const archived=await tool(manager.token,'workflows_archive',archiveArgs)
  assert.equal(archived.isError,false)
  const remainingWorkflows=await tool(reader.token,'workflows_list',{})
  assert.equal(remainingWorkflows.structuredContent.items.some(w=>w.id===controlled.id),false)
  const trash=await tool(reader.token,'workflows_trash',{})
  assert.equal(trash.structuredContent.items.some(w=>w.id===controlled.id),true)
  const recovered=await tool(manager.token,'workflows_restore',{workflowId:controlled.id,expectedVersion:archived.structuredContent.version,idempotencyKey:'restore-trashed-control'})
  assert.equal(recovered.isError,false)
  assert.equal(recovered.structuredContent.status,'paused')
  assert.equal((await tool(reader.token,'workflows_get',{workflowId:controlled.id})).structuredContent.lifecycle.enabled,false)

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
  assert.equal(request.scopeOmitted, false)
  await owner('mcp/oauth/' + requestId + '/approve', 'POST', {
    scopes: ['read', 'data_read', 'data_write']
  }, 400)
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
  const legacyEndpoint=endpoint+'/mcp'
  const legacyMetadata=origin+'/.well-known/oauth-protected-resource'+new URL(legacyEndpoint).pathname
  assert.equal((await (await boundedFetch(legacyMetadata)).json()).resource,legacyEndpoint)
  const legacyAuthorize=new URL(authUrl)
  legacyAuthorize.searchParams.set('resource',legacyEndpoint)
  legacyAuthorize.searchParams.set('state','legacy-client-state')
  const legacyRequest=await boundedFetch(legacyAuthorize,{redirect:'manual'})
  assert.equal(legacyRequest.status,302)
  const legacyId=new URL(legacyRequest.headers.get('location')).searchParams.get('authorize')
  const legacyConsent=await owner('mcp/oauth/'+legacyId+'/approve','POST',{scopes:['read']})
  const legacyAccess=await oauth(discovery.token_endpoint,{...exchange,resource:legacyEndpoint,code:new URL(legacyConsent.redirect).searchParams.get('code')})
  const legacyRpc=await boundedFetch(legacyEndpoint,{method:'POST',headers:{Authorization:'Bearer '+legacyAccess.access_token,'Content-Type':'application/json',Accept:'application/json, text/event-stream'},body:JSON.stringify({jsonrpc:'2.0',id:1,method:'ping'})})
  assert.equal(legacyRpc.status,200)
  await rpc(legacyAccess.access_token,'ping',{},401)
  const legacyRefresh=await oauth(discovery.token_endpoint,{grant_type:'refresh_token',client_id:client.client_id,resource:legacyEndpoint,refresh_token:legacyAccess.refresh_token})
  assert.ok(legacyRefresh.access_token)
  const noScope = new URL(authUrl)
  noScope.searchParams.delete('scope')
  noScope.searchParams.set('state', 'data-editor-consent')
  const dataRequest = await boundedFetch(noScope, { redirect: 'manual' })
  assert.equal(dataRequest.status, 302)
  const dataId = new URL(dataRequest.headers.get('location')).searchParams.get('authorize')
  const dataConsent = await owner('mcp/oauth/' + dataId)
  assert.equal(dataConsent.scopeOmitted, true)
  assert.deepEqual(dataConsent.scopes, ['read', 'audit', 'data_read', 'data_write'])
  const dataApproval = await owner('mcp/oauth/' + dataId + '/approve', 'POST', {
    scopes: ['read', 'data_read', 'data_write']
  })
  const dataAccess = await oauth(discovery.token_endpoint, {
    ...exchange, code: new URL(dataApproval.redirect).searchParams.get('code')
  })
  assert.equal(dataAccess.scope, 'read data_read data_write')
  const dataTools = (await rpc(dataAccess.access_token, 'tools/list')).result.tools.map(tool => tool.name)
  assert.ok(dataTools.includes('data_table_create'))
  assert.ok(dataTools.includes('data_table_rename'))
  assert.ok(!dataTools.includes('workflows_create'))
  const tableTool = async (name, args) => {
    const { result } = await rpc(dataAccess.access_token, 'tools/call', { name, arguments: args })
    return result.structuredContent
  }
  const createTable = {
    definition: { id: 'oauth-table', name: 'OAuth table', kind: 'table', fields: [{ name: 'status', kind: 'string' }] },
    idempotencyKey: 'oauth-table-create'
  }
  const denied = await rpc(reader.token, 'tools/call', { name: 'data_table_create', arguments: createTable })
  assert.equal(denied.result.structuredContent.code, 'forbidden')
  const createdTable = await tableTool('data_table_create', createTable)
  assert.equal(createdTable.status, 'created')
  assert.equal(createdTable.tableId, 'oauth-table')
  assert.deepEqual(await tableTool('data_table_create', createTable), createdTable)
  const table = await tableTool('data_table_get', { tableId: createdTable.tableId })
  const rename = { tableId: createdTable.tableId, name: 'Renamed OAuth table', expectedVersion: table.table.version }
  const renamed = await tableTool('data_table_rename', { ...rename, idempotencyKey: 'oauth-table-rename' })
  assert.equal(renamed.status, 'updated')
  assert.equal((await tableTool('data_table_get', { tableId: createdTable.tableId })).table.definition.name, rename.name)
  assert.equal((await tableTool('data_table_rename', { ...rename, idempotencyKey: 'oauth-table-stale-rename' })).status, 'conflict')
  await owner('mcp/credentials/' + builder.id, 'DELETE')
  await rpc(builder.token, 'ping', {}, 401)
  assert.equal(
    JSON.stringify(await owner('workspace')).includes('tokenHash'),
    false
  )
  console.log(
    'MCP checks passed: discovery, isolated bearer scopes, idempotency/restart, CAS, audit, OAuth consent/PKCE/audience, scoped table management, refresh replay and revocation.'
  )
}
