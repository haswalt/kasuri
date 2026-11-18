import test from 'node:test'
import assert from 'node:assert/strict'
import { addCanvasNode, connect, connectionError, edgeNear, layout } from '../src/lib/canvas.ts'
import { appendNode, createWorkflow, parseWorkflow, validateWorkflow } from '../src/lib/workflows.ts'

test('drop connects to an open output and keeps the chosen position', () => {
  const original = createWorkflow()
  const { workflow, nodeId } = addCanvasNode(original, 'Merge', { x: 80, y: 200 })
  assert.deepEqual(workflow.nodes.find(n => n.id === nodeId)?.position, { x: 80, y: 200 })
  assert.equal(workflow.edges[0].source, original.nodes[0].id)
  assert.equal(workflow.edges[0].target, nodeId)
  assert.deepEqual(validateWorkflow(workflow), [])
})
test('drop on edge inserts a node, retains branch port and shifts descendants', () => {
  let original = appendNode(createWorkflow(), 'IF / Condition')
  original = appendNode(original, 'Merge')
  const edge = original.edges[1]
  const { workflow, nodeId } = addCanvasNode(original, 'Wait / Delay', { x: 0, y: 280 }, { edgeId: edge.id })
  assert.equal(workflow.edges.find(e => e.id === edge.id)?.sourcePort, 'true')
  assert.equal(workflow.edges.find(e => e.id === edge.id)?.target, nodeId)
  assert.equal(workflow.edges.find(e => e.source === nodeId)?.target, edge.target)
  assert.ok(workflow.nodes.find(n => n.id === edge.target)!.position!.y >= 440)
  assert.deepEqual(validateWorkflow(workflow), [])
})
test('autoconnection chooses the unused condition branch and never chooses an error output', () => {
  let original = appendNode(createWorkflow(), 'IF / Condition')
  original = appendNode(original, 'Merge')
  const condition = original.nodes[1]
  const added = addCanvasNode(original, 'Merge', { x: 260, y: 320 }, { nodeId: condition.id })
  assert.equal(added.workflow.edges.find(e => e.target === added.nodeId)?.sourcePort, 'false')
  const completed = { ...original, edges: [...original.edges, { id: 'false', source: condition.id, target: original.nodes[2].id, sourcePort: 'false' }, { id: 'tail', source: original.nodes[2].id, target: condition.id }] }
  // All normal ports are occupied; dropping must not use the spare error handles.
  const isolated = addCanvasNode(completed, 'Merge', { x: 0, y: 500 })
  assert.equal(isolated.workflow.edges.length, completed.edges.length)
})
test('connection validation rejects cycles, duplicates, self edges and incoming trigger edges', () => {
  const workflow = appendNode(appendNode(createWorkflow(), 'Merge'), 'Merge')
  const [start, a, b] = workflow.nodes
  assert.match(connectionError(workflow, b.id, a.id, 'success')!, /cycle/)
  assert.match(connectionError(workflow, a.id, b.id, 'success')!, /already/)
  assert.match(connectionError(workflow, a.id, a.id, 'success')!, /itself/)
  assert.match(connectionError(workflow, b.id, start.id, 'success')!, /Triggers/)
  assert.throws(() => connect(workflow, start.id, b.id, 'false'), /valid output/)
})
test('reconnection excludes the original edge and keeps its identity', () => {
  const workflow = appendNode(appendNode(createWorkflow(), 'Merge'), 'Merge')
  const edge = workflow.edges[0]
  const reconnected = connect(workflow, edge.source, workflow.nodes[2].id, 'success', edge.id)
  assert.equal(reconnected.edges.length, workflow.edges.length)
  assert.equal(reconnected.edges.find(e => e.id === edge.id)?.target, workflow.nodes[2].id)
})
test('layout and edge hit testing work for old graphs without saved positions', () => {
  const workflow = appendNode(createWorkflow(), 'Merge')
  assert.deepEqual(layout(workflow).get(workflow.nodes[1].id), { x: 0, y: 160 })
  assert.equal(edgeNear(workflow, { x: 100, y: 125 }, 24), workflow.edges[0].id)
  assert.equal(edgeNear(workflow, { x: 500, y: 125 }, 24), undefined)
})
test('export/import retains positions and rejects malformed coordinates', () => {
  const workflow = createWorkflow()
  workflow.nodes[0].position = { x: -120, y: 300 }
  assert.deepEqual(parseWorkflow(JSON.stringify(workflow)).nodes[0].position, { x: -120, y: 300 })
  assert.throws(() => parseWorkflow(JSON.stringify({ ...workflow, nodes: [{ ...workflow.nodes[0], position: { x: 'bad', y: 0 } }] })), /position/)
})
test('a second trigger cannot be dropped into an existing workflow', () => {
  assert.throws(() => addCanvasNode(createWorkflow(), 'Schedule', { x: 0, y: 0 }), /existing trigger/)
})
test('a deleted trigger can be replaced and reconnected to the remaining graph', () => {
  const original = appendNode(createWorkflow(), 'Merge')
  const withoutTrigger = { ...original, nodes: original.nodes.slice(1), edges: [] }
  const replacement = addCanvasNode(withoutTrigger, 'Manual Trigger', { x: 0, y: 0 })
  assert.equal(replacement.workflow.edges[0].source, replacement.nodeId)
  assert.deepEqual(validateWorkflow(replacement.workflow), [])
})
