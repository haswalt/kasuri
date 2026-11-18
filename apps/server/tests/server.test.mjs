import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, rm, writeFile, mkdir } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { DatabaseSync } from 'node:sqlite'
import { createStandaloneServer } from '../../../dist/server/main.mjs'
const sleep = ms => new Promise(r => setTimeout(r, ms))
async function fixture(options = {}) {
  const dir = await mkdtemp(join(tmpdir(), 'kasuri-server-'))
  const config = { dataDir:join(dir,'data'), assetsDir:resolve('apps/web/dist'), host:'127.0.0.1', port:0, ...options }
  const app = await createStandaloneServer(config)
  let cookie = ''
  async function api(path, method='GET', body, origin=app.origin) {
    const response = await fetch(`${origin}/api/${path}`, { method, headers: { cookie, ...(body === undefined ? {} : { 'Content-Type':'application/json' }) }, ...(body === undefined ? {} : { body:JSON.stringify(body) }) })
    if (response.headers.has('set-cookie')) cookie = response.headers.get('set-cookie').split(';')[0]
    assert.ok(response.ok, `${path}: ${response.status} ${await response.clone().text()}`)
    return response.json()
  }
  return { dir, config, app, api, get cookie() { return cookie }, async clean() { await app.close(); await rm(dir,{recursive:true,force:true}) } }
}
test('standalone API runs actual Rust simulations and serves the frontend', async () => {
  const f = await fixture()
  try {
    const workspace = await f.api('workspace')
    assert.equal(workspace.connectors.length,9)
    const run = await f.api('runs','POST',{ workflowId:'core-data-demo', mode:'simulation', input:{ name:'Portable' } })
    assert.equal(run.status,'succeeded')
    assert.deepEqual(run.nodes.find(n=>n.id==='parse').output,{name:'Portable',source:'kasuri'})
    const page = await fetch(f.app.origin)
    assert.equal(page.status,200); assert.match(await page.text(),/Kasuri/)
    const denied = await fetch(`${f.app.origin}/api/runs`,{method:'POST',headers:{Origin:'https://evil.test'}})
    assert.equal(denied.status,403)
  } finally { await f.clean() }
})
test('a real wait and node outputs survive server restart', async () => {
  const f = await fixture(); let restarted
  try {
    await f.api('workspace')
    const run = await f.api('runs','POST',{workflowId:'core-data-demo',mode:'run',input:{name:'Recovered'}})
    assert.equal(run.status,'running')
    await f.app.close()
    restarted = await createStandaloneServer(f.config)
    let result
    for(let i=0;i<30;i++) { result = await f.api(`runs/${run.id}`,'GET',undefined,restarted.origin); if(result.status==='succeeded') break; await sleep(100) }
    assert.equal(result.status,'succeeded')
    assert.equal(result.nodes.find(n=>n.id==='wait').attempts,1)
    assert.deepEqual(result.nodes.find(n=>n.id==='parse').output,{name:'Recovered',source:'kasuri'})
  } finally { if(restarted) await restarted.close(); else await f.app.close(); await rm(f.dir,{recursive:true,force:true}) }
})
test('cron registration and due occurrence recover after restart without duplicate runs', async () => {
  const f = await fixture(); let restarted
  try {
    await f.api('workspace'); await f.api('workflows/scheduled-heartbeat/publish','POST')
    await f.app.close()
    const db = new DatabaseSync(join(f.config.dataDir,f.cookie.split('=')[1]+'.sqlite'))
    db.prepare('UPDATE workflows SET next_at=? WHERE id=?').run(Date.now()-1000,'scheduled-heartbeat'); db.close()
    restarted = await createStandaloneServer(f.config)
    let runs=[]
    for(let i=0;i<30;i++) { runs=(await f.api('workspace','GET',undefined,restarted.origin)).runs.filter(r=>r.mode==='schedule'); if(runs.length) break; await sleep(50) }
    assert.equal(runs.length,1); assert.equal(runs[0].status,'succeeded')
    await f.api('workflows/scheduled-heartbeat/pause','POST',undefined,restarted.origin)
    assert.equal((await f.api('workspace','GET',undefined,restarted.origin)).runs.filter(r=>r.mode==='schedule').length,1)
  } finally { if(restarted) await restarted.close(); else await f.app.close(); await rm(f.dir,{recursive:true,force:true}) }
})
test('a second server cannot own the same persistent data volume', async () => {
  const f = await fixture()
  try { await assert.rejects(createStandaloneServer(f.config),/already owned/) } finally { await f.clean() }
})
test('optional Basic authentication protects UI and API but allows health probes', async () => {
  const f = await fixture({basicAuth:'admin:test-password-only'})
  try {
    assert.equal((await fetch(f.app.origin)).status,401)
    assert.equal((await fetch(`${f.app.origin}/api/health`)).status,200)
    const response=await fetch(`${f.app.origin}/api/workspace`,{headers:{Authorization:'Basic '+Buffer.from('admin:test-password-only').toString('base64')}})
    assert.equal(response.status,200)
  } finally { await f.clean() }
})
test('static files stay inside the assets directory and missing assets return 404', async () => {
  const dir=await mkdtemp(join(tmpdir(),'kasuri-assets-')); const assets=join(dir,'public'); await mkdir(assets)
  await writeFile(join(assets,'index.html'),'<h1>Kasuri</h1>'); await writeFile(join(dir,'private.txt'),'do-not-serve')
  const f=await fixture({assetsDir:assets})
  try {
    assert.equal((await fetch(`${f.app.origin}/missing.js`)).status,404)
    assert.equal((await fetch(`${f.app.origin}/some/client/route`)).status,200)
    const response=await fetch(`${f.app.origin}/..%2fprivate.txt`)
    assert.equal(response.status,403)
  } finally { await f.clean(); await rm(dir,{recursive:true,force:true}) }
})
test('persistent self-hosted workspaces have no automatic data expiry', async () => {
  const f = await fixture({sessionTtlHours:0})
  try { assert.equal((await f.api('workspace')).expiresAt,null) } finally { await f.clean() }
})
