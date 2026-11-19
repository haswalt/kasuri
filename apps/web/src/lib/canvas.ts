import type { Workflow, WorkflowNode } from './workflows.ts'
import { uid } from './workflows.ts'
export type Point = { x: number; y: number }
export const isTrigger = (type: string) => ['Manual Trigger', 'Webhook', 'Schedule'].includes(type)
export const ports = (type: string) => type === 'IF / Condition' ? ['true', 'false', 'error'] : ['success', 'error']
export const NODE_WIDTH = 200
export const NODE_HEIGHT = 56
export const NODE_GAP = 112

/** Stable topological layout for imported/older graphs without saved positions. */
export function layout(workflow: Workflow): Map<string, Point> {
  const rank = new Map(workflow.nodes.map(n => [n.id, 0]))
  const incoming = new Map(workflow.nodes.map(n => [n.id, workflow.edges.filter(e => e.target === n.id).length]))
  const queue = workflow.nodes.filter(n => incoming.get(n.id) === 0).map(n => n.id)
  for (let i = 0; i < queue.length; i++) for (const edge of workflow.edges.filter(e => e.source === queue[i])) {
    if (!incoming.has(edge.target)) continue
    rank.set(edge.target, Math.max(rank.get(edge.target)!, rank.get(edge.source)! + 1))
    incoming.set(edge.target, incoming.get(edge.target)! - 1)
    if (incoming.get(edge.target) === 0) queue.push(edge.target)
  }
  const rows = new Map<number, number>()
  return new Map(workflow.nodes.map(node => {
    const depth = rank.get(node.id)!
    const column = rows.get(depth) ?? 0; rows.set(depth, column + 1)
    return [node.id, node.position ?? { x: column * 360, y: depth * NODE_GAP }]
  }))
}

export function connectionError(workflow: Workflow, source: string, target: string, port: string, excludeEdgeId?: string): string | null {
  const from = workflow.nodes.find(n => n.id === source)
  const to = workflow.nodes.find(n => n.id === target)
  if (!from || !to) return 'Both steps must exist.'
  if (source === target) return 'A step cannot connect to itself.'
  if (isTrigger(to.type)) return 'Triggers cannot have incoming connections.'
  if (!ports(from.type).includes(port)) return 'Choose a valid output port.'
  const edges = workflow.edges.filter(e => e.id !== excludeEdgeId)
  if (edges.some(e => e.source === source && e.target === target && (e.sourcePort ?? 'success') === port)) return 'That connection already exists.'
  const seen = new Set<string>(); const pending = [target]
  while (pending.length) {
    const id = pending.pop()!
    if (id === source) return 'That connection would create a cycle.'
    if (seen.has(id)) continue
    seen.add(id)
    pending.push(...edges.filter(e => e.source === id).map(e => e.target))
  }
  return null
}
export function connect(workflow: Workflow, source: string, target: string, port: string, edgeId?: string): Workflow {
  const error = connectionError(workflow, source, target, port, edgeId)
  if (error) throw new Error(error)
  return { ...workflow, status: 'draft', edges: [...workflow.edges.filter(e => e.id !== edgeId), { id: edgeId ?? uid(), source, target, sourcePort: port }] }
}

export function addCanvasNode(workflow: Workflow, type: string, position: Point, anchor?: { edgeId?: string; nodeId?: string }): { workflow: Workflow; nodeId: string; message: string } {
  if (workflow.nodes.length >= 100) throw new Error('A workflow supports at most 100 nodes.')
  if (isTrigger(type) && workflow.nodes.some(n => isTrigger(n.type))) throw new Error('Change the existing trigger instead of adding a second one.')
  const node: WorkflowNode = { id: uid(), type, label: type, position, config: type === 'Parse Body' ? {format:'json'} : type === 'Wait / Delay' ? { delayMs: '1000' } : type === 'Schedule' ? { cron: '* * * * *', timezone: 'UTC' } : type === 'JSON Utilities' ? { operation: 'stringify' } : {} }
  let next: Workflow = { ...workflow, status: 'draft', nodes: [...workflow.nodes, node] }
  if (isTrigger(type)) {
    const target = workflow.nodes.find(n => !workflow.edges.some(e => e.target === n.id))
    if (target) next = connect(next, node.id, target.id, 'success')
    return { workflow: next, nodeId: node.id, message: target ? `Trigger connected to ${target.label}.` : 'Trigger added.' }
  }
  const edge = workflow.edges.find(e => e.id === anchor?.edgeId)
  if (edge) {
    const positions = layout(workflow)
    const shift = Math.max(0, position.y + NODE_GAP - positions.get(edge.target)!.y)
    const downstream = new Set<string>(); const pending = [edge.target]
    while (pending.length) { const id = pending.pop()!; if (downstream.has(id)) continue; downstream.add(id); pending.push(...workflow.edges.filter(e => e.source === id).map(e => e.target)) }
    next.nodes = next.nodes.map(n => downstream.has(n.id) ? { ...n, position: { x: positions.get(n.id)!.x, y: positions.get(n.id)!.y + shift } } : n)
    next.edges = workflow.edges.filter(e => e.id !== edge.id)
    next = connect(next, edge.source, node.id, edge.sourcePort ?? 'success', edge.id)
    next = connect(next, node.id, edge.target, type === 'IF / Condition' ? 'true' : 'success')
    return { workflow: next, nodeId: node.id, message: 'Inserted into the connection.' }
  }
  const positions = layout(workflow)
  const available = workflow.nodes.flatMap(n => ports(n.type).filter(p => p !== 'error' && !workflow.edges.some(e => e.source === n.id && (e.sourcePort ?? 'success') === p)).map(port => ({ node: n, port })))
  available.sort((a, b) => {
    if (a.node.id === anchor?.nodeId && b.node.id !== anchor?.nodeId) return -1
    if (b.node.id === anchor?.nodeId && a.node.id !== anchor?.nodeId) return 1
    const distance = (n: WorkflowNode) => { const p = positions.get(n.id)!; return Math.hypot(p.x - position.x, p.y + NODE_GAP - position.y) }
    return distance(a.node) - distance(b.node)
  })
  const from = available[0]
  if (from && !isTrigger(type)) {
    next = connect(next, from.node.id, node.id, from.port)
    return { workflow: next, nodeId: node.id, message: `Connected from ${from.node.label} (${from.port}).` }
  }
  return { workflow: next, nodeId: node.id, message: workflow.nodes.length ? 'Added without a connection. Draw from an output handle to connect it.' : 'Trigger added.' }
}

/** Match the space between vertically laid-out nodes when a drop misses the thin SVG stroke. */
export function edgeNear(workflow: Workflow, point: Point, tolerance: number): string | undefined {
  const positions = layout(workflow)
  let nearest: { id: string; distance: number } | undefined
  for (const edge of workflow.edges) {
    const from = positions.get(edge.source); const to = positions.get(edge.target)
    if (!from || !to) continue
    const a = { x: from.x + NODE_WIDTH / 2, y: from.y + NODE_HEIGHT }
    const b = { x: to.x + NODE_WIDTH / 2, y: to.y }
    const dx = b.x - a.x; const dy = b.y - a.y
    const length = dx * dx + dy * dy
    if (!length) continue
    const t = ((point.x - a.x) * dx + (point.y - a.y) * dy) / length
    if (t <= 0 || t >= 1) continue
    const distance = Math.hypot(point.x - a.x - t * dx, point.y - a.y - t * dy)
    if (distance < tolerance && (!nearest || distance < nearest.distance)) nearest = { id: edge.id, distance }
  }
  return nearest?.id
}
