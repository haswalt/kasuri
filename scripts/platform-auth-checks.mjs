import assert from 'node:assert/strict'
import { generateKeyPairSync, sign } from 'node:crypto'

export async function checkPlatformAuth(origin, restart) {
  const basic = 'Basic ' + Buffer.from('test:runtime-check').toString('base64')
  const legacy = await fetch(origin + '/api/mcp', { headers: { Authorization: basic } })
  assert.equal(legacy.status, 200)
  const legacyMcp = await legacy.json()
  const legacyCookie = legacy.headers.get('set-cookie').split(';')[0]
  const oldCredential = await fetch(origin + '/api/mcp/credentials', {
    method: 'POST', headers: { Authorization: basic, Cookie: legacyCookie, 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: 'Legacy integration', role: 'reader' })
  }).then(response => response.json())

  const issuer = 'https://test-team.cloudflareaccess.com'
  const audience = 'fixture-audience'
  const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 })
  const jwk = { ...publicKey.export({ format: 'jwk' }), kid: 'test-key', alg: 'RS256', use: 'sig' }
  const base64 = value => Buffer.from(JSON.stringify(value)).toString('base64url')
  function token(subject, overrides = {}, signingKey = privateKey) {
    const signed = [base64({ alg: 'RS256', kid: jwk.kid }), base64({
      iss: issuer, aud: [audience], sub: subject, email: subject + '@example.com', type: 'app',
      exp: Math.floor(Date.now() / 1000) + 600, ...overrides
    })].join('.')
    return signed + '.' + sign('sha256', Buffer.from(signed), signingKey).toString('base64url')
  }
  async function browser(path, assertion, { headers = {}, method = 'GET', body } = {}) {
    return fetch(origin + path, { method, headers: {
      ...(assertion ? { 'Cf-Access-Jwt-Assertion': assertion } : {}),
      ...(body ? { 'Content-Type': 'application/json' } : {}), ...headers
    }, body: JSON.stringify(body) })
  }
  async function rpc(endpoint, credential, name) {
    const response = await fetch(endpoint, { method: 'POST', headers: {
      Authorization: 'Bearer ' + credential, 'Content-Type': 'application/json',
      Accept: 'application/json, text/event-stream'
    }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name } }) })
    assert.equal(response.status, 200)
    return (await response.json()).result.structuredContent
  }
  await restart({ PLATFORM_AUTH: 'access' }, { keys: [jwk] })
  assert.equal((await browser('/api/workspaces')).status, 503)
  const policy = {
    PLATFORM_AUTH: 'access', PLATFORM_ACCESS_ISSUER: issuer, PLATFORM_ACCESS_AUDIENCE: audience,
    PLATFORM_ACCESS_LEGACY_OWNERS: JSON.stringify({ alice: legacyMcp.workspaceId })
  }
  await restart(policy, { keys: [jwk] })
  for (const path of ['/index.html', '/w/personal/workflows', '/api/workspaces', '/api/mcp']) {
    assert.equal((await browser(path)).status, 401)
  }
  assert.equal((await browser('/api/workspaces', null, { headers: {
    Cookie: legacyCookie, 'X-Kasuri-Principal': base64({ issuer, subject: 'alice' }), 'X-Kasuri-Delegated': '1'
  } })).status, 401)
  assert.equal((await browser('/api/health')).status, 200)
  const expiredAt = Math.floor(Date.now() / 1000) - 1
  const forged = generateKeyPairSync('rsa', { modulusLength: 2048 }).privateKey
  for (const assertion of [
    token('alice', { exp: expiredAt }), token('alice', { nbf: expiredAt + 300 }),
    token('alice', { aud: ['other'] }), token('alice', { iss: 'https://other.cloudflareaccess.com' }),
    token(''), token('alice', { service_token_status: true }), token('alice', {}, forged)
  ]) {
    assert.equal((await browser('/api/workspaces', assertion)).status, 401)
  }
  const alice = token('alice')
  const bob = token('bob')
  const migrated = await browser('/api/mcp', alice)
  assert.equal(migrated.status, 200, await migrated.clone().text())
  assert.equal((await migrated.json()).workspaceId, legacyMcp.workspaceId)
  const current = async assertion => {
    const response = await browser('/api/workspaces', assertion)
    assert.equal(response.status, 200, await response.clone().text())
    assert.equal(response.headers.get('set-cookie'), null)
    return response.json()
  }
  const child = await browser('/api/workspaces', alice, { method: 'POST', body: { name: 'Alice workspace' } })
  assert.equal(child.status, 201, await child.clone().text())
  const childId = (await child.json()).id
  const directory = await current(token('alice', { email: 'updated@example.com' }))
  assert.equal(directory.personalWorkspaceId, legacyMcp.workspaceId)
  assert.equal(directory.user.email, 'updated@example.com')
  assert.ok(directory.workspaces.some(workspace => workspace.id === childId))
  assert.ok(!(await current(bob)).workspaces.some(workspace => workspace.id === childId))
  assert.equal((await browser('/api/mcp', bob, { headers: {
    'X-Kasuri-Workspace': childId, 'X-Kasuri-Delegated': '1'
  } })).status, 404)
  const old = await rpc(legacyMcp.endpoint, oldCredential.token, 'workspace_identity_get')
  assert.equal(old.workspaceId, legacyMcp.workspaceId)
  assert.equal(old.authorizedBy, null)
  assert.equal(old.principal.name, 'Legacy integration')
  const options = { headers: { 'X-Kasuri-Workspace': childId } }
  const newMcp = await browser('/api/mcp', alice, options).then(response => response.json())
  const newCredential = await browser('/api/mcp/credentials', alice, {
    ...options, method: 'POST', body: { name: 'New integration', scopes: ['data_read'] }
  }).then(response => response.json())
  const identity = await rpc(newMcp.endpoint, newCredential.token, 'workspace_identity_get')
  assert.equal(identity.workspaceId, childId)
  assert.equal(identity.authorizedBy.subject, 'alice')
  assert.deepEqual(identity.principal.scopes, ['data_read'])
  assert.ok(!JSON.stringify(identity).includes(newCredential.token))
  await restart({ ...policy, PLATFORM_ACCESS_LEGACY_OWNERS: JSON.stringify({ bob: legacyMcp.workspaceId }) }, { keys: [jwk] })
  assert.equal((await browser('/api/mcp', bob)).status, 403)
  console.log('Passed: Access JWT verification, legacy migration, browser session continuity, workspace isolation and MCP identity.')
}
