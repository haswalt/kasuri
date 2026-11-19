import type { Workflow, WorkflowNode } from '../../../../packages/contracts/src/index'
export type { Workflow, WorkflowNode, Run } from '../../../../packages/contracts/src/index'
export const uid = () => crypto.randomUUID()
export function createWorkflow(name = 'Untitled workflow'): Workflow {
  return { schemaVersion: 1, id: uid(), name, description: '', status: 'draft', revision: 0, updatedAt: new Date().toISOString(), nodes: [{ id: uid(), type: 'Manual Trigger', label: 'Start manually', config: {} }], edges: [] }
}
export function appendNode(workflow: Workflow, type: string): Workflow {
  const node = { id: uid(), type, label: type, config: {} }
  const last = workflow.nodes.at(-1)
  return { ...workflow, status: 'draft', nodes: [...workflow.nodes, node], edges: last ? [...workflow.edges, { id: uid(), source: last.id, target: node.id, sourcePort: last.type === 'IF / Condition' ? 'true' : 'success' }] : [] }
}
export function removeNode(workflow: Workflow, id: string): Workflow {
  const incoming = workflow.edges.filter(e => e.target === id)
  const outgoing = workflow.edges.filter(e => e.source === id)
  const edges = workflow.edges.filter(e => e.source !== id && e.target !== id)
  if (incoming.length === 1 && outgoing.length === 1 && (outgoing[0].sourcePort ?? 'success') === 'success') edges.push({ id: uid(), source: incoming[0].source, target: outgoing[0].target, sourcePort: incoming[0].sourcePort })
  return { ...workflow, status: 'draft', nodes: workflow.nodes.filter(node => node.id !== id), edges }
}
export function validateWorkflow(workflow: Workflow): string[] {
  const errors: string[] = []
  if (!workflow.name.trim()) errors.push('Give your workflow a name.')
  if (!workflow.nodes.length) errors.push('Add a trigger to start the workflow.')
  const triggers = workflow.nodes.filter(n => ['Manual Trigger', 'Webhook', 'Schedule', 'Execute Workflow Trigger'].includes(n.type))
  if (triggers.length !== 1) errors.push('Exactly one trigger is required; only the first step should be a trigger.')
  if (new Set(workflow.nodes.map(n => n.id)).size !== workflow.nodes.length) errors.push('Node IDs must be unique.')
  if (new Set(workflow.edges.map(e => e.id)).size !== workflow.edges.length) errors.push('Edge IDs must be unique.')
  const nodes = new Map(workflow.nodes.map(n => [n.id, n]))
  const counts = new Map(workflow.nodes.map(n => [n.id, 0]))
  const connections = new Set<string>()
  for (const edge of workflow.edges) {
    const source = nodes.get(edge.source)
    if (!source || !nodes.has(edge.target)) { errors.push('An edge references a missing node.'); continue }
    const port = edge.sourcePort ?? 'success'
    if (!(source.type === 'IF / Condition' ? ['true', 'false', 'error'] : ['success', 'error']).includes(port)) errors.push('An edge uses an incompatible output port.')
    const key = JSON.stringify([edge.source, edge.target, port])
    if (connections.has(key)) errors.push('Duplicate connection.')
    connections.add(key)
    counts.set(edge.target, (counts.get(edge.target) ?? 0) + 1)
  }
  if (triggers.some(t => counts.get(t.id) !== 0)) errors.push('A trigger cannot have incoming connections.')
  if (workflow.nodes.some(n => !triggers.includes(n) && counts.get(n.id) === 0)) errors.push('Connect every step to the graph.')
  const queue = workflow.nodes.filter(n => counts.get(n.id) === 0).map(n => n.id)
  let visited = 0
  for (let i = 0; i < queue.length; i++) {
    visited++
    for (const e of workflow.edges.filter(e => e.source === queue[i])) if (counts.has(e.target)) {
      counts.set(e.target, counts.get(e.target)! - 1)
      if (counts.get(e.target) === 0) queue.push(e.target)
    }
  }
  if (visited !== workflow.nodes.length) errors.push('Workflow contains a cycle.')
  workflow.nodes.forEach(node => {
    if (!node.label.trim()) errors.push('Every step needs a name.')
    if (node.type === 'HTTP Request') {
      try { if (typeof node.config.url !== 'string' || !['http:', 'https:'].includes(new URL(node.config.url).protocol)) throw new Error() }
      catch { errors.push(`${node.label}: enter an HTTP or HTTPS URL.`) }
    }
    if (node.type === 'Schedule' && (typeof node.config.cron !== 'string' || !node.config.cron.trim())) errors.push(`${node.label}: enter a cron expression.`)
  })
  return [...new Set(errors)]
}
export function parseWorkflow(text: string, validate = true): Workflow {
  const value = JSON.parse(text)
  if (!value || value.schemaVersion !== 1 || typeof value.name !== 'string' || typeof value.description !== 'string' || !Array.isArray(value.nodes) || !Array.isArray(value.edges) || value.nodes.length > 100) throw new Error('Expected a version 1 workflow with up to 100 steps.')
  if (value.nodes.some((n: WorkflowNode) => !n || typeof n.id !== 'string' || typeof n.type !== 'string' || typeof n.label !== 'string' || !n.config || Array.isArray(n.config) || typeof n.config !== 'object')) throw new Error('Invalid node configuration.')
  if (value.nodes.some((n: WorkflowNode) => n.position !== undefined && (!n.position || !Number.isFinite(n.position.x) || !Number.isFinite(n.position.y)))) throw new Error('Invalid node position.')
  if (value.edges.some((e: Workflow['edges'][number]) => !e || typeof e.id !== 'string' || typeof e.source !== 'string' || typeof e.target !== 'string' || (e.sourcePort !== undefined && typeof e.sourcePort !== 'string'))) throw new Error('Invalid workflow edges.')
  const workflow: Workflow = { ...value, id: uid(), status: 'draft', revision: 0, updatedAt: new Date().toISOString() }
  const errors = validateWorkflow(workflow)
  if (validate && errors.length) throw new Error(errors.join(' '))
  return workflow
}
