import { cx } from '../../styled-system/css'
import { workflowNode, connectorPort } from '../../styled-system/recipes'
import { Button } from '../design/ui'
import { styles } from '../design/styles'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  ReactFlow,
  ReactFlowProvider,
  Background,
  Controls,
  Handle,
  Position,
  MarkerType,
  ConnectionLineType,
  useReactFlow,
  useNodesState,
  useEdgesState
} from '@xyflow/react'
import type { Node, NodeProps, Connection, Edge } from '@xyflow/react'
import type { Workflow, WorkflowNode } from '../lib/workflows'
import {
  addCanvasNode,
  connect,
  connectionError,
  edgeNear,
  isTrigger,
  layout,
  NODE_WIDTH,
  NODE_GAP,
  ports
} from '../lib/canvas'
import { Tile, Icon } from '../design/ui'

type CanvasNode = Node<{ node: WorkflowNode; status?: string }, 'workflow'>
function WorkflowCard({ data, selected }: NodeProps<CanvasNode>) {
  return (
    <div
      className={workflowNode({
        state:
          workflowNode.variantMap.state.find(
            (state) => state === data.status
          ) || 'idle',
        selected: Boolean(selected)
      })}
    >
      {!isTrigger(data.node.type) && (
        <Handle
          type="target"
          id="input"
          position={Position.Top}
          aria-label={`${data.node.label}: input`}
        />
      )}
      <Tile kind={data.node.type} size={30} />
      <div className={styles.nodeCardCopy}>
        <small>
          {data.status ? data.status.toUpperCase() : data.node.type}
        </small>
        <strong>{data.node.label}</strong>
      </div>
      {ports(data.node.type).map((port, i, all) => (
        <div
          key={port}
          className={styles.flowPort}
          style={
            port === 'error'
              ? { left: '100%', top: '50%' }
              : { left: `${((i + 1) * 100) / all.length}%` }
          }
        >
          <span
            data-port={port}
            className={cx(
              styles.portLabel,
              connectorPort({ error: port === 'error' })
            )}
          >
            {port}
          </span>
          <Handle
            type="source"
            id={port}
            position={port === 'error' ? Position.Right : Position.Bottom}
            data-port={port}
            className={connectorPort({ error: port === 'error' })}
            aria-label={`${data.node.label}: ${port} output`}
          />
        </div>
      ))}
    </div>
  )
}
const nodeTypes = { workflow: WorkflowCard }
const edgeColor = (port: string) =>
  port === 'error'
    ? 'var(--fail)'
    : port === 'true'
      ? 'var(--ok)'
      : 'var(--line-strong)'

export function WorkflowCanvas(props: {
  workflow: Workflow
  selectedId?: string
  select: (id?: string) => void
  update: (workflow: Workflow) => void
  showPalette: () => void
  addType?: string
  onAdded: () => void
  dragging: boolean
  allowedTypes: string[]
  states?: Record<string, string>
  readOnly?: boolean
}) {
  return (
    <ReactFlowProvider>
      <Canvas {...props} />
    </ReactFlowProvider>
  )
}
function Canvas({
  workflow,
  selectedId,
  select,
  update,
  showPalette,
  addType,
  onAdded,
  dragging,
  allowedTypes,
  states,
  readOnly
}: Parameters<typeof WorkflowCanvas>[0]) {
  const [zoom, setZoom] = useState(100)
  const reconnectingId = useRef<string | undefined>(undefined)
  const [nodes, setNodes, onNodesChange] = useNodesState<CanvasNode>([])
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([])
  const [message, setMessage] = useState(
    'Drag a node onto the canvas. Drop on a line to insert it.'
  )
  const { screenToFlowPosition, getZoom, fitView, setCenter } =
    useReactFlow<CanvasNode>()
  const graphKey = JSON.stringify({
    nodes: workflow.nodes,
    edges: workflow.edges
  })
  const positions = useMemo(() => layout(workflow), [graphKey])
  useEffect(() => {
    setNodes(
      workflow.nodes.map((node) => ({
        id: node.id,
        type: 'workflow',
        position: positions.get(node.id)!,
        data: { node, status: states?.[node.id] },
        selected: node.id === selectedId,
        width: NODE_WIDTH,
        ariaRole: 'button',
        ariaLabel: `${node.label}, ${node.type}`
      }))
    )
    setEdges(
      workflow.edges.map((edge) => {
        const port = edge.sourcePort ?? 'success'
        const active = states?.[edge.target] === 'running'
        const skipped = states?.[edge.target] === 'skipped'
        const done =
          states?.[edge.source] === 'succeeded' ||
          (states?.[edge.source] === 'failed' && port === 'error')
        const color = active
          ? 'var(--run)'
          : skipped
            ? 'var(--line)'
            : done
              ? port === 'error'
                ? 'var(--fail)'
                : 'var(--ok)'
              : edgeColor(port)
        return {
          ...edge,
          animated: active,
          sourceHandle: port,
          targetHandle: 'input',
          type: 'default',
          pathOptions: { curvature: 0.4 },
          label: port === 'success' ? undefined : port,
          markerEnd: { type: MarkerType.Arrow, width: 12, height: 12, color },
          style: {
            stroke: color,
            strokeWidth: active ? 1.75 : 1.25,
            ...(active || skipped || port === 'error'
              ? { strokeDasharray: active ? '5 5' : '4 4' }
              : {})
          },
          interactionWidth: 24
        }
      })
    )
  }, [graphKey, selectedId, JSON.stringify(states)])
  // React Flow subscribes to this callback. Keep it stable while playback changes
  // selectedId, so registration cannot feed the previous selection back upstream.
  const onSelectionChange = useCallback(
    ({ nodes: selected }: { nodes: CanvasNode[] }) => {
      if (selected.length === 1) select(selected[0].id)
    },
    [select]
  )
  const root = useRef<HTMLElement>(null)
  function add(
    type: string,
    position: { x: number; y: number },
    edgeId?: string,
    nodeId?: string
  ) {
    try {
      if (!allowedTypes.includes(type))
        throw new Error('Choose an implemented node from the palette.')
      const result = addCanvasNode(workflow, type, position, { edgeId, nodeId })
      update(result.workflow)
      select(result.nodeId)
      setMessage(result.message)
    } catch (error) {
      setMessage(
        error instanceof Error ? error.message : 'Could not add a node.'
      )
    }
  }
  useEffect(() => {
    if (!addType) return
    const last = [...positions.values()].sort((a, b) => b.y - a.y)[0] ?? {
      x: 0,
      y: -NODE_GAP
    }
    const position = { x: last.x, y: last.y + NODE_GAP }
    add(addType, position, undefined, selectedId)
    void setCenter(position.x + 100, position.y + 50, {
      zoom: Math.max(0.8, getZoom()),
      duration: 200
    })
    onAdded()
  }, [addType])
  function createConnection(connection: Connection, edgeId?: string) {
    try {
      update(
        connect(
          workflow,
          connection.source,
          connection.target,
          connection.sourceHandle ?? 'success',
          edgeId
        )
      )
      setMessage('Connection saved in the draft.')
    } catch (error) {
      setMessage(error instanceof Error ? error.message : 'Invalid connection.')
    }
  }
  return (
    <section
      ref={root}
      className={styles.graphCanvas}
      aria-label="Workflow canvas"
      onDragOverCapture={(event) => {
        event.preventDefault()
        event.dataTransfer.dropEffect = 'copy'
      }}
      onDropCapture={(event) => {
        event.preventDefault()
        event.stopPropagation()
        const type =
          event.dataTransfer.getData('application/kasuri-node') ||
          event.dataTransfer.getData('text/plain')
        if (!type || readOnly) return
        const position = screenToFlowPosition({
          x: event.clientX,
          y: event.clientY
        })
        const target = event.target instanceof Element ? event.target : null
        let edgeId =
          target?.closest('.react-flow__edge')?.getAttribute('data-id') ??
          undefined
        let nearest = 24 / getZoom()
        // During a palette drag an HTML overlay accepts drops over SVG edges consistently.
        // Hit-test the rendered path so insertion follows curves as well as straight lines.
        for (const group of root.current?.querySelectorAll(
          '.react-flow__edge'
        ) ?? []) {
          const path = group.querySelector<SVGPathElement>(
            '.react-flow__edge-path'
          )
          if (!path) continue
          const length = path.getTotalLength()
          const samples = Math.min(100, Math.max(2, Math.ceil(length / 12)))
          for (let i = 0; i <= samples; i++) {
            const p = path.getPointAtLength((length * i) / samples)
            const distance = Math.hypot(p.x - position.x, p.y - position.y)
            if (distance < nearest) {
              nearest = distance
              edgeId = group.getAttribute('data-id') ?? undefined
            }
          }
        }
        edgeId ??= edgeNear(workflow, position, 24 / getZoom())
        add(
          type,
          { x: position.x - NODE_WIDTH / 2, y: position.y },
          edgeId,
          selectedId
        )
      }}
    >
      <div className={styles.canvasToolbar}>
        <span className={styles.eyebrow}>CANVAS</span>
        <small className={styles.mono}>
          {workflow.nodes.length} NODES · {workflow.edges.length} EDGES
        </small>
        <div>
          <Button
            className={styles.iconButton}
            aria-label="Search nodes"
            disabled={readOnly}
            onClick={showPalette}
          >
            <Icon name="search" />
          </Button>
          <Button
            disabled={readOnly}
            onClick={() => {
              const arranged = layout({
                ...workflow,
                nodes: workflow.nodes.map((n) => ({
                  ...n,
                  position: undefined
                }))
              })
              update({
                ...workflow,
                nodes: workflow.nodes.map((n) => ({
                  ...n,
                  position: arranged.get(n.id)
                }))
              })
              requestAnimationFrame(() => {
                void fitView({ padding: 0.25, maxZoom: 1, duration: 250 })
              })
            }}
          >
            Arrange
          </Button>
        </div>
      </div>
      <ReactFlow<CanvasNode>
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onMove={(_, viewport) => setZoom(Math.round(viewport.zoom * 100))}
        onSelectionChange={onSelectionChange}
        onNodeClick={(_, node) => select(node.id)}
        onPaneClick={() => select(undefined)}
        onNodeDragStop={(_, moved, group) => {
          const movedPositions = new Map(
            (group.length ? group : [moved]).map((n) => [n.id, n.position])
          )
          update({
            ...workflow,
            nodes: workflow.nodes.map((n) =>
              movedPositions.has(n.id)
                ? { ...n, position: movedPositions.get(n.id) }
                : n
            )
          })
        }}
        connectionLineType={ConnectionLineType.Bezier}
        onConnect={(connection) => createConnection(connection)}
        onReconnectStart={(_, edge) => {
          reconnectingId.current = edge.id
        }}
        onReconnectEnd={() => {
          reconnectingId.current = undefined
        }}
        onReconnect={(old, connection) => createConnection(connection, old.id)}
        isValidConnection={(c) =>
          !connectionError(
            workflow,
            c.source,
            c.target,
            c.sourceHandle ?? 'success',
            reconnectingId.current
          )
        }
        onDelete={({ nodes: deletedNodes, edges: deletedEdges }) => {
          const nodeIds = new Set(deletedNodes.map((n) => n.id))
          const edgeIds = new Set(deletedEdges.map((e) => e.id))
          update({
            ...workflow,
            nodes: workflow.nodes.filter((n) => !nodeIds.has(n.id)),
            edges: workflow.edges.filter(
              (e) =>
                !edgeIds.has(e.id) &&
                !nodeIds.has(e.source) &&
                !nodeIds.has(e.target)
            )
          })
          if (selectedId && nodeIds.has(selectedId)) select(undefined)
        }}
        nodesDraggable={!readOnly}
        nodesConnectable={!readOnly}
        edgesReconnectable={!readOnly}
        className={'kasuri-flow'}
        fitView
        fitViewOptions={{ padding: 0.2, maxZoom: 1 }}
        minZoom={0.25}
        maxZoom={1.6}
        deleteKeyCode={readOnly ? null : ['Backspace', 'Delete']}
        snapToGrid
        snapGrid={[10, 10]}
        connectOnClick
        selectionOnDrag
        panOnDrag={[1, 2]}
        panOnScroll
        zoomOnScroll={false}
      >
        {dragging && (
          <div className={styles.canvasDropOverlay} aria-hidden="true" />
        )}
        <Background gap={20} size={1} color="var(--dot)" />
        <Controls
          orientation="horizontal"
          fitViewOptions={{ padding: 0.25, maxZoom: 1 }}
          showInteractive={false}
        >
          <span className={styles.zoomValue}>{zoom}%</span>
        </Controls>
      </ReactFlow>
      <div className={styles.canvasHelp} role="status">
        {message} <span>Scroll to pan · Pinch to zoom</span>
      </div>
    </section>
  )
}
