import assert from 'node:assert/strict'
import { setTimeout as sleep } from 'node:timers/promises'
import { readFileSync } from 'node:fs'
import { versionedTestClient } from './versioned-test-client.mjs'

export async function checkMcpClient(origin, restart) {
  let versions = versionedTestClient()
  const Authorization = 'Basic ' + Buffer.from('test:runtime-check').toString('base64')
  let cookie = ''
  async function api(path, method = 'GET', body) {
    const prepared = versions.prepare(path, method, body)
    const response = await fetch(origin + '/api/' + path, {
      method, headers: { Authorization, cookie, ...(prepared ? {'Content-Type':'application/json'} : {}) },
      body: JSON.stringify(prepared)
    })
    if (response.headers.has('set-cookie')) cookie = response.headers.get('set-cookie').split(';')[0]
    const text = await response.text()
    assert.ok(response.ok, `${path} ${response.status}: ${text}`)
    return versions.remember(JSON.parse(text))
  }

  await api('workspace')
  const workspaceId = cookie.split('=')[1].split('.')[0]
  const profile = {
    url:'https://fixture.example/mcp-client', workspaceIds:[workspaceId],
    allowedTools:['check_availability','returns_error'], bearerToken:'fixture-mcp-token'
  }
  await restart({MCP_CONNECTIONS:JSON.stringify({
    availability:profile,
    blocked:{...profile,url:'https://private-dns.example/mcp-client'},
    redirect:{...profile,url:'https://fixture.example/redirect'},
  })})
  const listed = await api('mcp-client/connections')
  assert.deepEqual(listed.connections.find(item => item.alias === 'availability'),{
    alias:'availability', tools:['check_availability','returns_error']
  })
  assert(!JSON.stringify(listed).includes(profile.bearerToken))
  assert(!JSON.stringify(listed).includes(profile.url))
  const workflow = JSON.parse(readFileSync('examples/workflows/36-mcp-client-booking-availability.json','utf8'))
  const lookup = workflow.nodes.find(node => node.id === 'lookup')
  await api('workflows/'+workflow.id,'PUT',workflow)
  async function run(mode = 'run', workflowId = workflow.id) {
    let record = await api('runs','POST',{workflowId,mode,input:{}})
    for (let i=0;record.status==='running'&&i<100;i++) {
      await sleep(100)
      record=await api('runs/'+record.id)
    }
    return record
  }
  const node = (run, id = 'lookup') => run.nodes.find(result => result.id === id)
  const simulated = await run('simulation')
  assert.equal(simulated.status, 'succeeded')
  assert.equal(node(simulated).output.content[0].text,'Available')
  async function configure(config) {
    lookup.config = config
    await api('workflows/'+workflow.id,'PUT',workflow)
  }
  await configure({connection:'availability',tool:'check_availability'})
  const real = await run()
  assert.equal(real.status,'succeeded',JSON.stringify(real))
  assert.deepEqual(node(real, 'reply').output,{available:true})
  assert.deepEqual(node(real).output, {
    tool:'check_availability',content:[{type:'text',text:'[REDACTED]'}],
    structuredContent:{available:true,session:'[REDACTED]'}
  })

  const discovery = JSON.parse(readFileSync('examples/workflows/37-mcp-client-list-tools.json','utf8'))
  discovery.nodes.find(node => node.id === 'discover').config.connection = 'availability'
  await api('workflows/'+discovery.id,'PUT',discovery)
  const discovered = await run('run', discovery.id)
  assert.equal(discovered.status,'succeeded',JSON.stringify(discovered))
  const tools = node(discovered, 'discover').output
  assert.deepEqual(tools.tools.map(tool=>tool.name), ['check_availability','returns_error'])
  assert.equal(tools.truncated,false)

  for (const [config, expected] of [
    [{connection:'availability',tool:'hidden'},'egress_denied'],
    [{connection:'blocked',tool:'check_availability'},'egress_denied'],
    [{connection:'redirect',tool:'check_availability'},'egress_denied'],
    [{connection:'availability',tool:'returns_error'},'external'],
  ]) {
    await configure(config)
    const result = await run()
    assert.equal(result.status,'failed',JSON.stringify(result))
    assert.equal(node(result).error,expected)
    assert(!JSON.stringify(result).includes('private error detail'))
  }
  cookie = ''
  versions = versionedTestClient()
  assert.deepEqual(await api('mcp-client/connections'),{connections:[]})
  await configure({connection:'availability',tool:'check_availability'})
  assert.equal(node(await run()).error,'egress_denied')
  console.log('Passed: offline fixtures, approved MCP calls, version/session negotiation, split SSE messages, filtered schema discovery, redaction, tool errors, and workspace and egress enforcement.')
}
