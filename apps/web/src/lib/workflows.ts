export type WorkflowNode = { id: string; type: string; label: string; config: Record<string, string> }
export type Workflow = {
  schemaVersion: 1; id: string; name: string; description: string
  status: 'draft' | 'active' | 'paused'; revision: number; updatedAt: string
  nodes: WorkflowNode[]; edges: { id: string; source: string; target: string }[]
}
export type Run = { id: string; workflowId: string; name: string; createdAt: string; revision: number; nodes: { label: string; type: string }[] }
export const uid = () => crypto.randomUUID()
export function createWorkflow(name = 'Untitled workflow'): Workflow {
  return { schemaVersion: 1, id: uid(), name, description: '', status: 'draft', revision: 0, updatedAt: new Date().toISOString(), nodes: [{ id: uid(), type: 'Manual Trigger', label: 'Start manually', config: {} }], edges: [] }
}
export function appendNode(workflow: Workflow, type: string): Workflow {
  const node = { id: uid(), type, label: type, config: {} }
  const last = workflow.nodes.at(-1)
  return { ...workflow, status: 'draft', nodes: [...workflow.nodes, node], edges: last ? [...workflow.edges, { id: uid(), source: last.id, target: node.id }] : [] }
}
export function removeNode(workflow: Workflow, id: string): Workflow {
  const nodes = workflow.nodes.filter(node => node.id !== id)
  return { ...workflow, status: 'draft', nodes, edges: nodes.slice(1).map((node, i) => ({ id: uid(), source: nodes[i].id, target: node.id })) }
}
export function validateWorkflow(workflow: Workflow): string[] {
  const errors: string[] = []
  if (!workflow.name.trim()) errors.push('Give your workflow a name.')
  if (!workflow.nodes.length) errors.push('Add a trigger to start the workflow.')
  else if (!['Manual Trigger', 'Webhook', 'Schedule'].includes(workflow.nodes[0].type)) errors.push('The first step must be a trigger.')
  if (workflow.nodes.slice(1).some(node => ['Manual Trigger', 'Webhook', 'Schedule'].includes(node.type))) errors.push('Only the first step can be a trigger.')
  if (new Set(workflow.nodes.map(node => node.id)).size !== workflow.nodes.length) errors.push('Node IDs must be unique.')
  workflow.nodes.forEach(node => {
    if (!node.label.trim()) errors.push('Every step needs a name.')
    if (node.type === 'HTTP Request') {
      try { if (!['http:', 'https:'].includes(new URL(node.config.url).protocol)) throw new Error() }
      catch { errors.push(`${node.label}: enter an HTTP or HTTPS URL.`) }
    }
    if (node.type === 'Schedule' && !node.config.cron?.trim()) errors.push(`${node.label}: enter a cron expression.`)
  })
  // This first editor supports a connected, sequential graph only.
  if (workflow.edges.length !== Math.max(0, workflow.nodes.length - 1) || workflow.edges.some((edge, i) => edge.source !== workflow.nodes[i]?.id || edge.target !== workflow.nodes[i + 1]?.id)) errors.push('This prototype supports connected, sequential workflows only.')
  return errors
}
export function parseWorkflow(text: string, validate = true): Workflow {
  const value = JSON.parse(text)
  if (!value || value.schemaVersion !== 1 || typeof value.name !== 'string' || typeof value.description !== 'string' || !Array.isArray(value.nodes) || !Array.isArray(value.edges) || value.nodes.length > 100) throw new Error('Expected a version 1 workflow with up to 100 steps.')
  if (value.nodes.some((n: WorkflowNode) => !n || typeof n.id !== 'string' || typeof n.type !== 'string' || typeof n.label !== 'string' || !n.config || Array.isArray(n.config) || typeof n.config !== 'object' || Object.values(n.config).some(v => typeof v !== 'string'))) throw new Error('Invalid node configuration.')
  if (value.edges.some((e: { id: string; source: string; target: string }) => !e || typeof e.id !== 'string' || typeof e.source !== 'string' || typeof e.target !== 'string')) throw new Error('Invalid workflow edges.')
  const workflow: Workflow = { ...value, id: uid(), status: 'draft', revision: 0, updatedAt: new Date().toISOString() }
  const errors = validateWorkflow(workflow)
  if (validate && errors.length) throw new Error(errors.join(' '))
  return workflow
}
export function demoWorkflow(): Workflow {
  let workflow = createWorkflow('Webhook enrichment')
  workflow.description = 'Receive an event, enrich its payload and notify the team.'
  workflow.nodes[0] = { ...workflow.nodes[0], type: 'Webhook', label: 'Receive event' }
  workflow = appendNode(workflow, 'HTTP Request')
  workflow.nodes[1] = { ...workflow.nodes[1], label: 'Enrich payload', config: { url: 'https://api.example.com/enrich', method: 'GET' } }
  workflow = appendNode(workflow, 'Map / Transform')
  workflow = appendNode(workflow, 'Slack')
  return workflow
}
