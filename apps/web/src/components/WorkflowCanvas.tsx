import { useEffect, useMemo, useRef, useState } from 'react'
import { ReactFlow, ReactFlowProvider, Background, Controls, Handle, Position, MarkerType, useReactFlow, useNodesState, useEdgesState } from '@xyflow/react'
import type { Node, NodeProps, Connection, Edge } from '@xyflow/react'
import type { Workflow, WorkflowNode } from '../lib/workflows'
import { addCanvasNode, connect, connectionError, edgeNear, isTrigger, layout, NODE_WIDTH, ports } from '../lib/canvas'
import '@xyflow/react/dist/style.css'

type CanvasNode = Node<{ node: WorkflowNode }, 'workflow'>
function WorkflowCard({ data, selected }: NodeProps<CanvasNode>) {
  return <div className={`flow-card ${selected ? 'node-selected' : ''}`}>
    {!isTrigger(data.node.type) && <Handle type="target" id="input" position={Position.Top} aria-label={`${data.node.label}: input`} />}
    <span className={`node-icon ${isTrigger(data.node.type) ? 'tone-0' : 'tone-1'}`}>{isTrigger(data.node.type) ? 'ϟ' : '⌘'}</span>
    <div><small>{isTrigger(data.node.type) ? 'TRIGGER' : 'ACTION'}</small><strong>{data.node.label}</strong><span>{data.node.type}</span></div>
    {ports(data.node.type).map((port, i, all) => <div key={port} className="flow-port" style={{ left: `${(i + 1) * 100 / (all.length + 1)}%` }}><span className={`port-label port-${port}`}>{port}</span><Handle type="source" id={port} position={Position.Bottom} className={`port-${port}`} aria-label={`${data.node.label}: ${port} output`} /></div>)}
  </div>
}
const nodeTypes = { workflow: WorkflowCard }
const edgeColor = (port: string) => port === 'error' ? '#c78282' : port === 'false' ? '#bd9b58' : port === 'true' ? '#639780' : '#8795c1'

export function WorkflowCanvas(props: { workflow: Workflow; selectedId?: string; select: (id?: string) => void; update: (workflow: Workflow) => void; showPalette: () => void; addType?: string; onAdded: () => void; dragging: boolean; allowedTypes: string[] }) {
  return <ReactFlowProvider><Canvas {...props} /></ReactFlowProvider>
}
function Canvas({ workflow, selectedId, select, update, showPalette, addType, onAdded, dragging, allowedTypes }: Parameters<typeof WorkflowCanvas>[0]) {
  const reconnectingId = useRef<string | undefined>(undefined)
  const [nodes, setNodes, onNodesChange] = useNodesState<CanvasNode>([])
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([])
  const [message, setMessage] = useState('Drag a node onto the canvas. Drop on a line to insert it.')
  const { screenToFlowPosition, getZoom, fitView, setCenter } = useReactFlow<CanvasNode>()
  const graphKey = JSON.stringify({ nodes: workflow.nodes, edges: workflow.edges })
  const positions = useMemo(() => layout(workflow), [graphKey])
  useEffect(() => {
    setNodes(workflow.nodes.map(node => ({ id: node.id, type: 'workflow', position: positions.get(node.id)!, data: { node }, selected: node.id === selectedId, width: NODE_WIDTH, ariaLabel: `${node.label}, ${node.type}` })))
    setEdges(workflow.edges.map(edge => {
      const port = edge.sourcePort ?? 'success'
      return { ...edge, sourceHandle: port, targetHandle: 'input', type: 'smoothstep', label: port === 'success' ? undefined : port, markerEnd: { type: MarkerType.ArrowClosed, color: edgeColor(port) }, style: { stroke: edgeColor(port), strokeWidth: 1.8, ...(port === 'error' ? { strokeDasharray: '5 4' } : {}) }, interactionWidth: 24 }
    }))
  }, [graphKey, selectedId])
  const root = useRef<HTMLElement>(null)
  function add(type: string, position: { x: number; y: number }, edgeId?: string, nodeId?: string) {
    try {
      if (!allowedTypes.includes(type)) throw new Error('Choose an implemented node from the palette.')
      const result = addCanvasNode(workflow, type, position, { edgeId, nodeId })
      update(result.workflow); select(result.nodeId); setMessage(result.message)
    } catch (error) { setMessage(error instanceof Error ? error.message : 'Could not add a node.') }
  }
  useEffect(() => {
    if (!addType) return
    const last = [...positions.values()].sort((a, b) => b.y - a.y)[0] ?? { x: 0, y: -160 }
    const position = { x: last.x, y: last.y + 160 }
    add(addType, position, undefined, selectedId)
    void setCenter(position.x + 100, position.y + 50, { zoom: Math.max(.8, getZoom()), duration: 200 })
    onAdded()
  }, [addType])
  function createConnection(connection: Connection, edgeId?: string) {
    try { update(connect(workflow, connection.source, connection.target, connection.sourceHandle ?? 'success', edgeId)); setMessage('Connection saved in the draft.') }
    catch (error) { setMessage(error instanceof Error ? error.message : 'Invalid connection.') }
  }
  return <section ref={root} className="graph-canvas" aria-label="Workflow canvas" onDragOverCapture={event => { event.preventDefault(); event.dataTransfer.dropEffect = 'copy' }}
 onDropCapture={event => {
        event.preventDefault(); event.stopPropagation(); const type = event.dataTransfer.getData('application/kasuri-node') || event.dataTransfer.getData('text/plain')
        if (!type) return
        const position = screenToFlowPosition({ x: event.clientX, y: event.clientY })
        const target = event.target instanceof Element ? event.target : null
        let edgeId = target?.closest('.react-flow__edge')?.getAttribute('data-id') ?? undefined
        let nearest = 24 / getZoom()
        // During a palette drag an HTML overlay accepts drops over SVG edges consistently.
        // Hit-test the rendered path so insertion follows curves as well as straight lines.
        for (const group of root.current?.querySelectorAll('.react-flow__edge') ?? []) {
          const path = group.querySelector<SVGPathElement>('.react-flow__edge-path')
          if (!path) continue
          const length = path.getTotalLength(); const samples = Math.min(100, Math.max(2, Math.ceil(length / 12)))
          for (let i = 0; i <= samples; i++) {
            const p = path.getPointAtLength(length * i / samples)
            const distance = Math.hypot(p.x - position.x, p.y - position.y)
            if (distance < nearest) { nearest = distance; edgeId = group.getAttribute('data-id') ?? undefined }
          }
        }
        edgeId ??= edgeNear(workflow, position, 24 / getZoom())
        add(type, { x: position.x - NODE_WIDTH / 2, y: position.y }, edgeId, selectedId)
      }}
>
    <div className="canvas-toolbar"><span>{workflow.nodes.length} nodes · {workflow.edges.length} connections</span><div><button onClick={showPalette}>＋ Nodes</button><button onClick={() => { const arranged = layout({ ...workflow, nodes: workflow.nodes.map(n => ({ ...n, position: undefined })) }); update({ ...workflow, nodes: workflow.nodes.map(n => ({ ...n, position: arranged.get(n.id) })) }); requestAnimationFrame(() => { void fitView({ padding: .2, duration: 250 }) }) }}>Arrange</button></div></div>
    <ReactFlow<CanvasNode> nodes={nodes} edges={edges} nodeTypes={nodeTypes} onNodesChange={onNodesChange} onEdgesChange={onEdgesChange}
      onNodeClick={(_, node) => select(node.id)} onPaneClick={() => select(undefined)}
      onNodeDragStop={(_, moved, group) => { const movedPositions = new Map((group.length ? group : [moved]).map(n => [n.id, n.position])); update({ ...workflow, nodes: workflow.nodes.map(n => movedPositions.has(n.id) ? { ...n, position: movedPositions.get(n.id) } : n) }) }}
      onConnect={connection => createConnection(connection)} onReconnectStart={(_, edge) => { reconnectingId.current = edge.id }} onReconnectEnd={() => { reconnectingId.current = undefined }} onReconnect={(old, connection) => createConnection(connection, old.id)}
      isValidConnection={c => !connectionError(workflow, c.source, c.target, c.sourceHandle ?? 'success', reconnectingId.current)}
      onDelete={({ nodes: deletedNodes, edges: deletedEdges }) => { const nodeIds = new Set(deletedNodes.map(n => n.id)); const edgeIds = new Set(deletedEdges.map(e => e.id)); update({ ...workflow, nodes: workflow.nodes.filter(n => !nodeIds.has(n.id)), edges: workflow.edges.filter(e => !edgeIds.has(e.id) && !nodeIds.has(e.source) && !nodeIds.has(e.target)) }); if (selectedId && nodeIds.has(selectedId)) select(undefined) }}
      className="light" fitView fitViewOptions={{ padding: .2, maxZoom: 1 }} minZoom={.25} maxZoom={1.6} deleteKeyCode={['Backspace', 'Delete']} snapToGrid snapGrid={[10, 10]} connectOnClick edgesReconnectable selectionOnDrag panOnDrag={[1, 2]} panOnScroll zoomOnScroll={false}>
      {dragging && <div className="canvas-drop-overlay" aria-hidden="true" />}
      <Background gap={18} size={1} color="#d4d9e5" /><Controls showInteractive={false} />
    </ReactFlow>
    <div className="canvas-help" role="status">{message} <span>Drag the background to select · Scroll to pan · Pinch or use +/− to zoom</span></div>
  </section>
}
