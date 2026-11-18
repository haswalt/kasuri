import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { fixture } from './runtime-checks.mjs'
export async function checkWorkspaces(origin, credentials, restart) {
  const Authorization = 'Basic ' + Buffer.from(credentials).toString('base64')
  let cookie = ''
  async function call(path, method = 'GET', body, workspace = 'personal', expected = 200, cookieOverride) {
    const response = await fetch(origin + '/api/' + path, {
      method, signal: AbortSignal.timeout(10000),
      headers: { Authorization, cookie: cookieOverride ?? cookie, 'X-Kasuri-Workspace': workspace, 'X-Kasuri-Delegated': '1', ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) },
      ...(body === undefined ? {} : { body: JSON.stringify(body) })
    })
    if (cookieOverride === undefined && response.headers.has('set-cookie')) cookie = response.headers.get('set-cookie').split(';')[0]
    assert.equal(response.status, expected, `${path}: ${await response.clone().text()}`)
    return response.json()
  }
  const original = await call('workspace')
  assert.equal(original.workflows.length, 0)
  const directory = await call('workspaces')
  assert.deepEqual(directory.workspaces, [{id: 'personal', name: 'Personal workspace'}])
  await call('workflows/check', 'PUT', fixture)
  await call('workspaces', 'POST', {name:'\n'}, 'personal', 400)
  const named = await call('workspaces', 'POST', {name: 'Team sandbox'}, 'personal', 201)
  assert.match(named.id, /^[a-f0-9]{64}$/)
  assert.equal((await call('workspace', 'GET', undefined, named.id)).workflows.length, 0)
  await call('workflows/check', 'PUT', {...fixture, name:'Isolated workflow'}, named.id)
  assert.equal((await call('workspace')).workflows[0].name, fixture.name)
  assert.equal((await call('workspace', 'GET', undefined, named.id)).workflows[0].name, 'Isolated workflow')
  const personalMcp = await call('mcp')
  const teamMcp = await call('mcp', 'GET', undefined, named.id)
  assert.notEqual(personalMcp.endpoint, teamMcp.endpoint)
  const discovery = await (await fetch(origin + '/.well-known/oauth-authorization-server' + new URL(teamMcp.issuer).pathname)).json()
  const registered = await fetch(discovery.registration_endpoint, { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({client_name:'Workspace isolation check',redirect_uris:['http://127.0.0.1:3000/callback'],token_endpoint_auth_method:'none'}) })
  assert.equal(registered.status,201)
  const client = await registered.json()
  const authorize = new URL(discovery.authorization_endpoint)
  authorize.search = new URLSearchParams({response_type:'code',client_id:client.client_id,redirect_uri:client.redirect_uris[0],resource:teamMcp.endpoint,scope:'read audit',state:'workspace-check',code_challenge_method:'S256',code_challenge:createHash('sha256').update('a'.repeat(64)).digest('base64url')}).toString()
  const consent = await fetch(authorize,{redirect:'manual'})
  assert.equal(consent.status,302)
  const location = new URL(consent.headers.get('location'))
  assert.equal(location.pathname, '/w/' + named.id + '/mcp')
  const consentId = location.searchParams.get('authorize')
  await call('mcp/oauth/' + consentId, 'GET', undefined, 'personal',400)
  assert.equal((await call('mcp/oauth/' + consentId, 'GET', undefined, named.id)).resource,teamMcp.endpoint)
  await call('workspaces/' + named.id, 'PATCH', {name:'Renamed sandbox'})
  await call('workspaces/' + 'a'.repeat(64), 'PATCH', {name:'Wrong owner'}, 'personal', 404)
  // A forged delegation header cannot bypass ownership, even with a valid second session.
  const stranger = await fetch(origin + '/api/workspace', {headers:{Authorization}})
  const strangerCookie = stranger.headers.get('set-cookie').split(';')[0]
  await call('workspace', 'GET', undefined, named.id, 404, strangerCookie)
  await call('workspaces/' + named.id, 'PATCH', {name:'Hijacked'}, 'personal', 404, strangerCookie)
  origin = await restart() || origin
  assert.equal((await call('workspaces')).workspaces.find(w=>w.id===named.id).name, 'Renamed sandbox')
  assert.equal((await call('workspace', 'GET', undefined, named.id)).workflows[0].name, 'Isolated workflow')
  assert.equal((await call('workspace')).workflows[0].name, fixture.name)
  console.log('Passed: workspace creation, rename, per-request isolation, ownership checks, distinct MCP endpoints and restart persistence.')
}
