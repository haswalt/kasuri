import test from 'node:test'
import assert from 'node:assert/strict'
import { appendNode, createWorkflow, demoWorkflow, parseWorkflow, removeNode, validateWorkflow } from '../src/lib/workflows.ts'
test('append and remove maintain a connected graph with stable remaining node IDs', () => {
  const original = demoWorkflow()
  const updated = removeNode(original, original.nodes[1].id)
  assert.deepEqual(updated.nodes.map(n => n.id), [original.nodes[0].id, original.nodes[2].id, original.nodes[3].id])
  assert.equal(updated.edges.find(e => e.source === original.nodes[0].id)!.target, original.nodes[2].id)
  assert.deepEqual(validateWorkflow(updated), [])
})
test('invalid graph and missing HTTP configuration block publishing', () => {
  const w = appendNode(createWorkflow(), 'HTTP Request')
  assert.match(validateWorkflow(w).join(), /HTTP or HTTPS/)
  w.nodes[1].config.url = 'https://example.com'
  w.edges[0].source = 'missing'
  assert.match(validateWorkflow(w).join(), /missing node/)
})
test('import resets identity and publication state', () => {
  const w = { ...demoWorkflow(), status: 'active', revision: 4 }
  const imported = parseWorkflow(JSON.stringify(w))
  assert.notEqual(imported.id, w.id)
  assert.equal(imported.status, 'draft')
  assert.equal(imported.revision, 0)
})
test('import rejects malformed nodes and edges', () => {
  assert.throws(() => parseWorkflow('{"schemaVersion":2}'))
  assert.throws(() => parseWorkflow(JSON.stringify({ ...demoWorkflow(), nodes: [null] })))
  assert.throws(() => parseWorkflow(JSON.stringify({ ...demoWorkflow(), edges: [null] })))
})
test('in-progress invalid drafts can be restored without losing their configuration', () => {
  const w = appendNode(createWorkflow(), 'HTTP Request')
  assert.throws(() => parseWorkflow(JSON.stringify(w)))
  assert.equal(parseWorkflow(JSON.stringify(w), false).nodes.length, 2)
})
test('extra triggers and duplicate node IDs are invalid', () => {
  const w = appendNode(createWorkflow(), 'Schedule')
  w.nodes[1].id = w.nodes[0].id
  assert.match(validateWorkflow(w).join(), /first step/)
  assert.match(validateWorkflow(w).join(), /unique/)
})
test('branched graphs import with typed ports and retain JSON configuration', async () => {
  const { readFile } = await import('node:fs/promises')
  const graph = await readFile(new URL('../../../examples/basic-workflow.json', import.meta.url), 'utf8')
  const workflow = parseWorkflow(graph)
  assert.deepEqual(validateWorkflow(workflow), [])
  assert.equal(workflow.edges.find(e => e.source === 'condition')?.sourcePort, 'true')
})
test('cycles and incompatible condition ports fail graph validation', () => {
  const w = appendNode(createWorkflow(), 'IF / Condition')
  const next = appendNode(w, 'Merge')
  assert.equal(next.edges[1].sourcePort, 'true')
  next.edges[1].sourcePort = 'success'
  assert.match(validateWorkflow(next).join(), /incompatible/)
  next.edges[1].sourcePort = 'true'
  next.edges.push({ id: 'cycle', source: next.nodes[2].id, target: next.nodes[1].id })
  assert.match(validateWorkflow(next).join(), /cycle/)
})
