import assert from 'node:assert/strict'
import { DatabaseSync } from 'node:sqlite'
import { readFileSync } from 'node:fs'

export function checkStorage() {
  const source = readFileSync('crates/workflow-worker/src/storage.rs','utf8')
  const sql = name => {
    const match = source.match(new RegExp(`const ${name}: &str = (?:r#"([\\s\\S]*?)"#|"([^"\\n]*)");`))
    assert.ok(match, `Storage SQL ${name} is available for regression checks`)
    return match[1] || match[2]
  }
  const db = new DatabaseSync(':memory:')
  try {
    db.exec(sql('CREATE_RECORDS'))
    db.exec('CREATE TABLE workspace(id INTEGER PRIMARY KEY, body TEXT NOT NULL)')
    const legacy = {management:{credentials:'synthetic'},auditSequence:2,revisions:[{workflowId:'first',revision:1,graph:{id:'first'}}],workflows:[{id:'first',nodes:[{id:'start'}]},{id:'second'}],runs:[{checkpoint:'synthetic checkpoint'}],audit:[{id:1},{id:2}],webhooks:[{workflowId:'first',endpointId:'hook'}],webhookEvents:[{event_id:'event'}]}
    db.prepare('INSERT INTO workspace VALUES(1,?)').run(JSON.stringify(legacy))
    const save = db.prepare(sql('STORE_RECORDS'))
    save.run(db.prepare('SELECT body FROM workspace').get().body)
    const rows = db.prepare(sql('LOAD_RECORDS')).all()
    assert.equal(rows.length,9)
    const metadata = JSON.parse(rows.find(r=>r.kind==='metadata').body)
    assert.equal(metadata.storageVersion,1)
    assert.deepEqual(metadata.management,legacy.management)
    for (const key of ['workflows','runs','audit','webhooks','webhookEvents','revisions']) assert.equal(key in metadata,false)
    for (const [kind,key] of [['workflow','workflows'],['run','runs'],['audit','audit'],['webhook','webhooks'],['webhook_event','webhookEvents'],['revision','revisions']]) {
      assert.deepEqual(rows.filter(r=>r.kind===kind).map(r=>JSON.parse(r.body)),legacy[key])
    }
    assert.throws(()=>save.run('{invalid JSON'))
    assert.deepEqual(db.prepare(sql('LOAD_RECORDS')).all(),rows,'Failed writes preserve the complete previous generation')
    save.run(JSON.stringify({...legacy,workflows:[],runs:[],audit:[],webhooks:[],webhookEvents:[],revisions:[]}))
    const empty = db.prepare(sql('LOAD_RECORDS')).all()
    assert.equal(empty.length,1,'Deleted records never leak from older generations')
    assert.equal(empty[0].kind,'metadata')
    db.exec('DELETE FROM workspace_records WHERE generation < (SELECT MAX(generation) FROM workspace_records)')
    db.exec('DELETE FROM workspace WHERE id=1')
    assert.equal(db.prepare('SELECT COUNT(*) AS n FROM workspace').get().n,0)
    assert.equal(db.prepare('SELECT COUNT(*) AS n FROM workspace_records').get().n,1)
    console.log('Passed: legacy record migration, collection ordering, atomic generation writes, deletion and old-generation cleanup.')
  } finally { db.close() }
}
