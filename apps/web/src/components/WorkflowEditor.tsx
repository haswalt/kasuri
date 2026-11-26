import { Button } from '../design/ui'
import { styles } from '../design/styles'
import { useCallback, useEffect, useRef, useState, type CSSProperties } from 'react'
import type { Workflow } from '../lib/workflows'
import { removeNode, validateWorkflow } from '../lib/workflows'
import { connect, ports, isTrigger } from '../lib/canvas'
import { InvocationBuilder } from './InvocationBuilder'
import { CollectionBuilder } from './CollectionBuilder'
import { workflowInputHints } from '../lib/input-hints'
import { ClairicBuilder } from './ClairicBuilder'
import { McpClientBuilder } from './McpClientBuilder'
import { InputBindingsBuilder } from './InputBindingsBuilder'
import { ExpressionBuilder } from './ExpressionBuilder'
import { TransformBuilder } from './TransformBuilder'
import { DataCommandBuilder } from './DataCommandBuilder'
import { WebhookBuilder } from './WebhookBuilder'
import { ValidateSchemaBuilder } from './ValidateSchemaBuilder'
import { RoutingBuilder } from './RoutingBuilder'
import { ConditionBuilder } from './ConditionBuilder'
import { WorkflowCanvas } from './WorkflowCanvas'
import { fields } from '../lib/catalogue'
import { api } from '../lib/api'
import { Icon, Tile, Pill, Empty } from '../design/ui'
import { category } from '../design/ui-model'
import { go, path } from '../lib/navigation'
import type {
  NodeManifest,
  Run,
  WebhookEndpoint
} from '../../../../packages/contracts/src/index'
type Props = {
  workflow: Workflow
  connectors: NodeManifest[]
  update: (w: Workflow) => void
  mode: 'build' | 'simulate'
  input: string
  setInput: (v: string) => void
  busy: boolean
  runSimulation: () => Promise<Run>
  runLive: () => void
}
export function WorkflowEditor({
  workflow,
  connectors,
  update,
  mode,
  input,
  setInput,
  busy,
  runSimulation,
  runLive
}: Props) {
  const [nodeId, setSelectedNode] = useState<string | undefined>(
      workflow.nodes[0]?.id
    ),
    [search, setSearch] = useState(''),
    [dragging, setDragging] = useState(false),
    [addType, setAddType] = useState<string>(),
    [error, setError] = useState(''),
    [tab, setTab] = useState('Parameters')
  const [edgeId, setSelectedEdge] = useState<string>()
  const [inspectorWidth, setInspectorWidth] = useState(400)
  const resizeStart = useRef<{ x: number; width: number }>(null)
  function sizeInspector(width: number) {
    const max = Math.min(680, Math.max(280, window.innerWidth - 480))
    setInspectorWidth(Math.min(max, Math.max(280, width)))
  }
  const setNodeId = useCallback((id?: string) => {
    setSelectedNode(id)
    setSelectedEdge(undefined)
  }, [])
  const selectEdge = useCallback((id: string) => {
    setSelectedNode(undefined)
    setSelectedEdge(id)
  }, [])
  const edge = workflow.edges.find((e) => e.id === edgeId)
  function editConnection(source: string, target: string, port: string) {
    if (!edge) return
    try {
      update(connect(workflow, source, target, port, edge.id))
      setError('')
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Invalid connection.')
    }
  }
  const [webhook, setWebhook] = useState<WebhookEndpoint>(),
    [webhookBusy, setWebhookBusy] = useState(false)
  const [result, setResult] = useState<Run | null>(null),
    [cursor, setCursor] = useState(0),
    [playing, setPlaying] = useState(false),
    [speed, setSpeed] = useState(1),
    [pending, setPending] = useState(false)
  const node = workflow.nodes.find((n) => n.id === nodeId),
    manifest = connectors.find((c) => c.kind === node?.type)
  const simulate = mode === 'simulate',
    errors = validateWorkflow(workflow)
  const graphKey = JSON.stringify({
    nodes: workflow.nodes,
    edges: workflow.edges,
    input
  })
  useEffect(() => {
    setResult(null)
    setCursor(0)
    setPlaying(false)
  }, [graphKey])
  useEffect(() => {
    if (!playing || !result) return
    if (cursor >= result.nodes.length) {
      setPlaying(false)
      return
    }
    const timer = setTimeout(() => {
      setNodeId(result.nodes[cursor].id)
      setCursor((v) => v + 1)
    }, 1000 / speed)
    return () => clearTimeout(timer)
  }, [playing, cursor, result, speed, setNodeId])
  useEffect(() => {
    if (node?.type !== 'Webhook') return
    let cancelled = false
    void api
      .webhook(workflow.id)
      .then((value) => {
        if (!cancelled)
          setWebhook((previous) => ({
            ...value,
            token:
              previous?.endpointId === value.endpointId &&
              ['bearer', 'query'].includes(value.authentication)
                ? previous?.token
                : undefined
          }))
      })
      .catch(() => {
        if (!cancelled) setWebhook(undefined)
      })
    return () => {
      cancelled = true
    }
  }, [workflow.id, workflow.status, node?.type])
  function editNode(patch: Partial<Workflow['nodes'][number]>) {
    if (node)
      update({
        ...workflow,
        status: 'draft',
        schemaVersion: patch.inputBindings&&Object.keys(patch.inputBindings).length?2:workflow.schemaVersion,
        nodes: workflow.nodes.map((n) =>
          n.id === node.id ? { ...n, ...patch } : n
        )
      })
  }
  async function rotateWebhook() {
    setWebhookBusy(true)
    setError('')
    try {
      setWebhook(await api.rotateWebhook(workflow.id))
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not configure webhook.')
    } finally {
      setWebhookBusy(false)
    }
  }
  function changeEdge(id: string, target: string, port: string) {
    try {
      update(connect(workflow, node!.id, target, port, id))
      setError('')
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Invalid connection.')
    }
  }
  async function play() {
    if (playing) {
      setPlaying(false)
      return
    }
    if (result) {
      if (cursor >= result.nodes.length) setCursor(0)
      setPlaying(true)
      return
    }
    setPending(true)
    setError('')
    try {
      const run = await runSimulation()
      setResult(run)
      setCursor(0)
      setPlaying(true)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Simulation failed.')
    } finally {
      setPending(false)
    }
  }
  const states: Record<string, string> = {}
  if (result) {
    result.nodes.forEach((n, i) => {
      if (i < cursor) states[n.id] = n.status
      else if (i === cursor && playing) states[n.id] = 'running'
    })
  }
  const selectedResult = result?.nodes.find((n) => n.id === nodeId)
  const inputHints=workflowInputHints(workflow,node?.id,result,input)
  const parameterKeys = [
    ...new Set([
      ...(fields[node?.type || ''] || []).map((f) => f.key),
      ...Object.keys(
        (manifest?.configSchema.properties as Record<string, unknown>) || {}
      )
    ])
  ]
  const groups = [
    ['trigger', 'Triggers'],
    ['logic', 'Logic & data'],
    ['http', 'HTTP & AI'],
    ['cf', 'Cloudflare'],
    ['app', 'Apps'],
    ['error', 'Error handling']
  ]
  return (
    <>
      <div
        className={
          styles.simulationToolbar +
          ' ' +
          (!simulate ? styles.buildToolbar : '')
        }
      >
        {simulate ? (
          <>
            <div>
              <span className={styles.eyebrow}>SIMULATION</span>
              <p>Draft results · external calls are mocked, never sent</p>
            </div>
            <Button
              visual="primary"
              disabled={busy || pending}
              onClick={() => void play()}
            >
              <Icon
                name={
                  playing
                    ? 'pause'
                    : cursor && result && cursor >= result.nodes.length
                      ? 'reset'
                      : 'play'
                }
              />
              {pending
                ? 'Simulating…'
                : playing
                  ? 'Pause'
                  : result && cursor >= result.nodes.length
                    ? 'Replay results'
                    : 'Play'}
            </Button>
            <Button
              disabled={!result || cursor >= result.nodes.length}
              onClick={() => {
                setPlaying(false)
                setNodeId(result!.nodes[cursor].id)
                setCursor(cursor + 1)
              }}
            >
              <Icon name="step" />
              Step
            </Button>
            <Button
              disabled={!result}
              onClick={() => {
                setResult(null)
                setCursor(0)
                setPlaying(false)
              }}
            >
              <Icon name="reset" />
              Reset
            </Button>
            <span className={styles.segmented}>
              {[0.5, 1, 2].map((s) => (
                <Button
                  key={s}
                  aria-pressed={speed === s}
                  onClick={() => setSpeed(s)}
                >
                  {s}×
                </Button>
              ))}
            </span>
            <span className={styles.simulationProgress}>
              <small>
                Step {cursor} of {result?.nodes.length || 0}
              </small>
              <progress value={cursor} max={result?.nodes.length || 1} />
            </span>
            <Pill
              status={
                pending || playing
                  ? 'running'
                  : result && cursor >= result.nodes.length
                    ? result.status
                    : 'queued'
              }
              label={
                pending
                  ? 'Computing'
                  : playing
                    ? 'Playing results'
                    : result && cursor >= result.nodes.length
                      ? undefined
                      : 'Ready'
              }
            />
          </>
        ) : (
          <>
            <span>
              <Icon name="pencil" />
              Build your draft
            </span>
            <p>Drag nodes onto the canvas. Drop on a connection to insert, or onto the current trigger to replace it.</p>
            <Button disabled={busy} onClick={runLive}>
              <Icon name="play" />
              Run now
            </Button>
          </>
        )}
      </div>
      {error && (
        <div className={styles.editorError} role="alert">
          <Icon name="warning" />
          {error}
          <Button
            className={styles.iconButton}
            aria-label="Dismiss editor error"
            onClick={() => setError('')}
          >
            <Icon name="close" />
          </Button>
        </div>
      )}
      <div className={styles.editorLayout} style={{ '--inspector-width': `${inspectorWidth}px` } as CSSProperties}>
        <aside className={styles.nodeSidebar}>
          {simulate ? (
            <>
              <div className={styles.panelHeading}>
                <span className={styles.eyebrow}>TEST INPUT</span>
              </div>
              <div className={'panel-body'}>
                <p className={styles.muted}>
                  This input runs against the saved draft. HTTP nodes require
                  explicit simulation response fixtures.
                </p>
                <label>
                  Current input · JSON
                  <textarea
                    aria-label="Test input JSON"
                    className={styles.mono}
                    rows={15}
                    value={input}
                    onChange={(e) => setInput(e.target.value)}
                    spellCheck={false}
                  />
                </label>
                <Button onClick={() => go(path('runs'))}>
                  <Icon name="plus" />
                  Pin input from a past run
                </Button>
                <div className={styles.infoBox}>
                  Playback inspects final node snapshots returned by the Rust
                  simulation. Per-attempt event streaming is not available yet.
                </div>
              </div>
            </>
          ) : (
            <>
              <div className={styles.paletteSearch}>
                <Icon name="search" />
                <input
                  data-search
                  aria-label="Search nodes"
                  placeholder="Search nodes…"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                />
                <kbd>/</kbd>
              </div>
              <div className={styles.paletteContent}>
                {groups.map(([key, label]) => {
                  const items = connectors.filter(
                    (c) =>
                      category(c.kind) === key &&
                      c.kind.toLowerCase().includes(search.toLowerCase())
                  )
                  return items.length ? (
                    <section key={key}>
                      <div className={styles.paletteGroup}>
                        <span>{label}</span>
                        <small>{items.length}</small>
                      </div>
                      {items.map((c) => {
                        const replacing = isTrigger(c.kind) && workflow.nodes.some(n => isTrigger(n.type))
                        return (
                          <Button
                            className={styles.paletteNode}
                            key={c.kind}
                            draggable
                            title={replacing ? 'Drop onto the existing trigger to replace it' : c.kind}
                            onDragStart={(e) => {
                              setDragging(true)
                              e.dataTransfer.setData(
                                'application/kasuri-node',
                                c.kind
                              )
                              e.dataTransfer.setData('text/plain', c.kind)
                              e.dataTransfer.effectAllowed = 'copy'
                            }}
                            onDragEnd={() => setDragging(false)}
                            onClick={() => {
                              if (replacing && !isTrigger(node?.type ?? '')) {
                                setError('Select the existing trigger first, or drop the replacement onto it.')
                                return
                              }
                              setError('')
                              setAddType(c.kind)
                            }}
                          >
                            <Tile kind={c.kind} size={22} />
                            <span>{c.kind}</span>
                            <Icon name="plus" size={12} />
                          </Button>
                        )
                      })}
                    </section>
                  ) : null
                })}
                {!connectors.some((c) =>
                  c.kind.toLowerCase().includes(search.toLowerCase())
                ) && (
                  <p className={styles.paletteEmpty}>
                    No matching implemented nodes.
                  </p>
                )}
              </div>
              <div className={styles.paletteNote}>
                <Icon name="lock" />
                Only implemented nodes are shown.
              </div>
            </>
          )}
        </aside>
        <WorkflowCanvas
          workflow={workflow}
          selectedId={nodeId}
          selectedEdgeId={edgeId}
          selectEdge={selectEdge}
          select={setNodeId}
          update={update}
          showPalette={() =>
            document
              .querySelector<HTMLInputElement>('input[data-search]')
              ?.focus()
          }
          dragging={dragging}
          allowedTypes={connectors.map((c) => c.kind)}
          addType={addType}
          onAdded={() => setAddType(undefined)}
          states={simulate ? states : undefined}
          readOnly={simulate}
        />
        <aside className={styles.inspector}>
          <div className={styles.inspectorResize} role="separator" tabIndex={0}
            aria-orientation="vertical" aria-label="Resize node settings"
            aria-valuemin={280} aria-valuemax={680} aria-valuenow={inspectorWidth}
            onPointerDown={event => {
              if (event.button !== 0) return
              const width = event.currentTarget.parentElement?.getBoundingClientRect().width ?? inspectorWidth
              resizeStart.current = { x: event.clientX, width }
              event.currentTarget.setPointerCapture(event.pointerId)
            }}
            onPointerMove={event => {
              if (resizeStart.current && event.currentTarget.hasPointerCapture(event.pointerId)) {
                sizeInspector(resizeStart.current.width + resizeStart.current.x - event.clientX)
              }
            }}
            onLostPointerCapture={() => { resizeStart.current = null }}
            onKeyDown={event => {
              if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return
              event.preventDefault()
              event.stopPropagation()
              const width = event.currentTarget.parentElement?.getBoundingClientRect().width ?? inspectorWidth
              sizeInspector(event.key === 'Home' ? 280 : event.key === 'End' ? 680 : width + (event.key === 'ArrowLeft' ? 24 : -24))
            }} />
          {simulate ? (
            <>
              <div className={styles.panelHeading}>
                <h2>Execution results</h2>
                {result && (
                  <small className={styles.mono}>
                    {result.id.slice(0, 10)}…
                  </small>
                )}
              </div>
              <div className={styles.executionLog}>
                {!result ? (
                  <Empty icon="play" title="Ready to simulate">
                    Choose an input and press Play. External services will not
                    be called.
                  </Empty>
                ) : (
                  result.nodes.slice(0, cursor).map((n, i) => (
                    <Button
                      className={n.id === nodeId ? 'selected' : ''}
                      key={n.id}
                      onClick={() => setNodeId(n.id)}
                    >
                      <span className={styles.stepDot + ' ' + n.status} />
                      <div>
                        <strong>{n.label}</strong>
                        <small>
                          {n.status} · {n.attempts}{' '}
                          {n.attempts === 1 ? 'attempt' : 'attempts'}
                          {n.error ? ' · ' + n.error : ''}
                        </small>
                      </div>
                      <small>{i + 1}</small>
                    </Button>
                  ))
                )}
                {result && cursor === result.nodes.length && (
                  <div className={styles.resultFinished}>
                    <Pill status={result.status} />
                    <small>Simulation complete</small>
                  </div>
                )}
              </div>
              <div className={'panel-body' + ' ' + styles.selectedNodeData}>
                <div className={styles.eyebrow}>SELECTED NODE DATA</div>
                <h3>{node?.label || 'Select a node'}</h3>
                <span className={styles.eyebrow}>INPUT</span>
                <pre>
                  {selectedResult && result?.nodes[0]?.id === selectedResult.id
                    ? JSON.stringify(result.input, null, 2)
                    : '// Step input is not available in this run record'}
                </pre>
                <span className={styles.eyebrow}>OUTPUT</span>
                <pre>
                  {selectedResult &&
                  cursor > result!.nodes.indexOf(selectedResult)
                    ? selectedResult.output !== undefined
                      ? JSON.stringify(selectedResult.output, null, 2)
                      : '// ' + selectedResult.status
                    : '// Not reached in playback yet'}
                </pre>
                {result && result.webhookResponse?.decision && result.webhookResponse.decision.nodeId === node?.id && selectedResult && cursor > result.nodes.indexOf(selectedResult) && (
                  <>
                    <span className={styles.eyebrow}>RESPONSE PREVIEW · {result.webhookResponse.delivery}</span>
                    <pre>{JSON.stringify(result.webhookResponse.decision.response, null, 2)}</pre>
                  </>
                )}
              </div>
            </>
          ) : edge ? (
            <>
              <div className={styles.inspectorHeading}>
                <span className={styles.eyebrow}>CONNECTION</span>
                <strong>Edit connection</strong>
                <small className={styles.mono}>{edge.id}</small>
              </div>
              <div className={styles.inspectorScroll}>
                <label>From node
                  <select value={edge.source} onChange={(e) => editConnection(e.target.value, edge.target, ports(workflow.nodes.find((n) => n.id === e.target.value)!.type,workflow.nodes.find((n) => n.id === e.target.value)!.config)[0])}>
                    {workflow.nodes.map((n) => <option key={n.id} value={n.id}>{n.label}</option>)}
                  </select>
                </label>
                <label>Output
                  <select value={edge.sourcePort || 'success'} onChange={(e) => editConnection(edge.source, edge.target, e.target.value)}>
                    {ports(workflow.nodes.find((n) => n.id === edge.source)!.type,workflow.nodes.find((n) => n.id === edge.source)!.config).map((p) => <option key={p}>{p}</option>)}
                  </select>
                </label>
                <label>To node
                  <select value={edge.target} onChange={(e) => editConnection(edge.source, e.target.value, edge.sourcePort || 'success')}>
                    {workflow.nodes.map((n) => <option key={n.id} value={n.id}>{n.label}</option>)}
                  </select>
                </label>
                <label>Input<input value="input" readOnly /></label>
                <p>Drag either end of the selected connection to reconnect it.</p>
                <Button tone="danger" onClick={() => {
                  update({ ...workflow, edges: workflow.edges.filter((e) => e.id !== edge.id) })
                  setSelectedEdge(undefined)
                }}>Delete connection</Button>
                {error && <p role="alert">{error}</p>}
              </div>
            </>
          ) : node ? (
            <>
              <div className={styles.inspectorHeading}>
                <span className={styles.eyebrow}>NODE CONFIGURATION</span>
                <span
                  className={
                    styles.mono + ' ' + styles.muted + ' ' + styles.nodeId
                  }
                >
                  {node.id}
                </span>
                <div>
                  <Tile kind={node.type} size={26} />
                  <strong>{node.type}</strong>
                  <span className={styles.mono + ' ' + styles.muted}>
                    v{node.version || 1}
                  </span>
                </div>
              </div>
              <div className={styles.underlineTabs}>
                {['Parameters', 'Settings', 'Output'].map((t) => (
                  <Button
                    key={t}
                    aria-pressed={tab === t}
                    onClick={() => setTab(t)}
                  >
                    {t}
                  </Button>
                ))}
              </div>
              <div className={styles.inspectorScroll}>
                {tab === 'Parameters' ? (
                  <>
                    <label>
                      Node name
                      <input
                        value={node.label}
                        onChange={(e) => editNode({ label: e.target.value })}
                      />
                    </label>
                    {isTrigger(node.type)&&Object.keys(node.inputBindings??{}).length>0&&<p role="alert">Triggers expose the original event. <Button onClick={()=>editNode({inputBindings:{}})}>Remove trigger bindings</Button></p>}
                    {!isTrigger(node.type) && Object.keys(node.inputBindings ?? {}).length > 0 && <InputBindingsBuilder key={`bindings:${node.id}`} workflow={workflow} nodeId={node.id} bindings={node.inputBindings??{}} hints={workflowInputHints(workflow,node.id,result,input,false,true)} nodeHints={id=>workflowInputHints(workflow,id,result,input,true)} triggerHints={workflowInputHints(workflow,workflow.nodes.find(n=>isTrigger(n.type))?.id,result,input)} onChange={inputBindings=>editNode({inputBindings})}/>}
                    {node.type==='MCP Client'?<McpClientBuilder key={`mcp:${node.id}`} config={node.config} hints={inputHints} onChange={config=>editNode({config})}/>:node.type==='Clairic'?<ClairicBuilder key={`clairic:${node.id}`} config={node.config} hints={inputHints} onChange={config=>editNode({config})}/>:node.type==='Map / Transform'?<TransformBuilder key={`mapping:${node.id}`} config={node.config} hints={inputHints} onChange={config=>editNode({config})}/>
                    :node.type==='Data Tables'||node.type==='Context Store'?<DataCommandBuilder key={`data:${node.id}`} workflow={workflow} nodeId={node.id} config={node.config}
                      kind={node.type==='Data Tables'?'table':'context'} hints={inputHints} onChange={config=>editNode({config})}/>
                    :node.type==='Webhook'?<WebhookBuilder key={`webhook:${node.id}`} config={node.config} onChange={config=>editNode({config})}/>
                    :node.type==='Validate Schema'?<ValidateSchemaBuilder key={`schema:${node.id}`} config={node.config} hints={inputHints} onChange={config=>editNode({config})}/>
                    :node.type==='Expressions'?<ExpressionBuilder config={node.config} workflow={workflow} nodeId={node.id} hints={inputHints} nodeHints={id=>workflowInputHints(workflow,id,result,input,true)} triggerHints={workflowInputHints(workflow,workflow.nodes.find(n=>isTrigger(n.type))?.id,result,input)} onChange={config=>editNode({config})}/>:node.type==='Switch / Router'||node.type==='Merge'?<RoutingBuilder kind={node.type} config={node.config} hints={inputHints} workflow={workflow} nodeId={node.id} nodeHints={id=>workflowInputHints(workflow,id,result,input,true)} onChange={config=>editNode({config})}/>:node.type==='Each / Loop'||node.type==='Execute Workflow'||node.type==='Execute Workflow Trigger' ? <InvocationBuilder node={node} workflow={workflow} hints={inputHints} onChange={config=>editNode({config})} /> : node.type === 'IF / Condition' ? <ConditionBuilder config={node.config} sample={inputHints.sample} schema={inputHints.schema} onChange={config => editNode({ config })} /> : node.type==='Filter'||node.type==='Sort'||node.type==='Split / Batch'||node.type==='Aggregate' ? <CollectionBuilder kind={node.type} config={node.config} hints={inputHints} onChange={config=>editNode({config})} /> : parameterKeys.map((key) => {
                      const meta = fields[node.type]?.find((f) => f.key === key)
                      const property = (
                        (manifest?.configSchema.properties || {}) as Record<
                          string,
                          { enum?: string[]; type?: string }
                        >
                      )[key]
                      const value =
                        typeof node.config[key] === 'string'
                          ? (node.config[key] as string)
                          : node.config[key] === undefined
                            ? ''
                            : JSON.stringify(node.config[key], null, 2)
                      const multiline =
                        [
                          'mapping',
                          'body',
                          'headers',
                          'query',
                          'simulationResponse',
                          'text'
                        ].includes(key) || property?.type === 'object' || property?.type === 'array'
                      return (
                        <label key={key}>
                          {meta?.label || key}
                          {property?.enum ? (
                            <select
                              value={value || property.enum[0]}
                              onChange={(e) =>
                                editNode({
                                  config: {
                                    ...node.config,
                                    [key]: node.type==='Split / Parallel'&&key==='concurrency'?Number(e.target.value):e.target.value
                                  }
                                })
                              }
                            >
                              {property.enum.map((v) => (
                                <option key={v}>{v}</option>
                              ))}
                            </select>
                          ) : multiline ? (
                            <textarea
                              className={styles.mono}
                              rows={4}
                              value={value}
                              placeholder={meta?.placeholder}
                              spellCheck={false}
                              onChange={(e) =>
                                editNode({
                                  config: {
                                    ...node.config,
                                    [key]: node.type==='Split / Parallel'&&key==='concurrency'?Number(e.target.value):e.target.value
                                  }
                                })
                              }
                            />
                          ) : (
                            <input
                              className={
                                key === 'cron' ||
                                key === 'url' ||
                                key === 'pointer'
                                  ? styles.mono
                                  : ''
                              }
                              value={value}
                              placeholder={meta?.placeholder}
                              onChange={(e) =>
                                editNode({
                                  config: {
                                    ...node.config,
                                    [key]: node.type==='Split / Parallel'&&key==='concurrency'?Number(e.target.value):e.target.value
                                  }
                                })
                              }
                            />
                          )}
                        </label>
                      )
                    })}
                    {node.type === 'Webhook' && (
                      <section className={styles.webhookSettings}>
                        <h2 className={styles.sectionTitle}>
                          Webhook endpoint
                        </h2>
                        <p className={styles.muted}>
                          Save the trigger before generating its endpoint.
                          Publish to accept events; pause to stop new deliveries.
                          Draft edits do not change the live trigger.
                        </p>
                        {webhook?.path && (
                          <label>
                            POST URL
                            <input
                              readOnly
                              value={
                                new URL(webhook.path, window.location.origin)
                                  .href
                              }
                              onFocus={(e) => e.target.select()}
                            />
                          </label>
                        )}
                        {webhook?.token && (
                          <>
                            <label>
                              Secret key (shown once)
                              <input
                                type="password"
                                readOnly
                                value={webhook.token}
                                onFocus={(e) => e.target.select()}
                              />
                            </label>
                            <Button
                              onClick={() =>
                                void navigator.clipboard
                                  .writeText(webhook.token!)
                                  .catch(() =>
                                    setError(
                                      'Could not copy token. Select the token field and copy it manually.'
                                    )
                                  )
                              }
                            >
                              Copy secret key
                            </Button>
                          </>
                        )}
                        <p className={styles.muted}>
                          {webhook?.configured
                            ? webhook.active
                              ? `Published: accepting events (${webhook.authentication}).`
                              : 'Not active: publish the workflow to accept events.'
                            : 'No endpoint configured.'}{' '}
                          Event IDs are optional. Deduplicate explicitly in the workflow when needed.
                          Query authentication uses the configured query parameter;
                          HMAC modes use a named signing secret.
                        </p>
                        <Button
                          disabled={webhookBusy}
                          onClick={() => void rotateWebhook()}
                        >
                          {webhookBusy
                            ? 'Generating…'
                            : webhook?.configured
                              ? 'Rotate endpoint and secret'
                              : 'Generate endpoint'}
                        </Button>
                      </section>
                    )}
                    {node.type === 'Email Trigger' && <section className={styles.infoBox}>
                      <h3>Receive email on this workflow</h3>
                      <ol>
                        <li>Save and publish this workflow.</li>
                        <li>In the Worker’s settings, add or update the <code>EMAIL_ROUTES</code> secret. Add a lowercase recipient address and map it to the workspace and workflow IDs.</li>
                        <li>In Cloudflare Email Routing, add a rule for that address. Choose <strong>Send to a Worker</strong> and select this Kasuri Worker.</li>
                      </ol>
                      <p>Workflow ID <code>{workflow.id}</code></p>
                      <p>The workspace ID is the segment after <code>/mcp/</code> in this workspace’s <a href={path('mcp')} onClick={event => { event.preventDefault(); go(path('mcp')) }}>MCP endpoint</a>.</p>
                      <details>
                        <summary>Example EMAIL_ROUTES value</summary>
                        <pre>{JSON.stringify({ 'booking@example.com': { workspaceId: 'your-workspace-id', workflowId: workflow.id } }, null, 2)}</pre>
                      </details>
                      <a href="https://github.com/haswalt/kasuri/blob/develop/docs/email.md" target="_blank" rel="noreferrer">Email routing setup and JSON example</a>
                    </section>}
                    {!['Email Trigger', 'Webhook', 'Data Tables', 'Context Store', 'Validate Schema', 'Map / Transform', 'Expressions', 'Each / Loop', 'Execute Workflow', 'Execute Workflow Trigger'].includes(node.type) && <div className={styles.infoBox}>
                      {node.type === 'Respond to Webhook'
                        ? 'Responds at this point, then passes input onward. A second response attempt is an error. Run and Simulate only preview the response.'
                        : node.type === 'Send Email'
                        ? 'Enter fixed values or bind to, subject, text and optionally inReplyTo. Simulation previews delivery.'
                        : node.type === 'Error Handler'
                        ? 'Connect a node’s error output to receive its failure code at /error/code. Other outputs, including IF false, pass their data through unchanged.'
                        : node.type === 'MCP Client'
                        ? 'Live runs use an approved connection. Simulations use the saved fixture.'
                        : node.type === 'Clairic'
                        ? 'Live runs send the selected text to Workers AI. Simulations use the saved fixture.'
                        : node.type === 'HTTP Request'
                        ? 'Public HTTPS is allowed by default. Credentials use named secrets. Simulations require a response fixture.'
                          : node.type === 'Schedule'
                            ? 'Five-field cron in an IANA time zone. Publish to register the schedule.'
                            : 'Configuration is validated by the Rust connector before execution.'}
                    </div>}
                  </>
                ) : tab === 'Settings' ? (
                  <>
                    <h3>Execution</h3>
                    <label>
                      {node.type==='Each / Loop'?'Collection deadline (milliseconds)':'Timeout (milliseconds)'}
                      <input
                        type="number"
                        min={1}
                        value={node.timeoutMs || 30000}
                        onChange={(e) =>
                          editNode({
                            timeoutMs: Math.max(
                              1,
                              Number(e.target.value) || 30000
                            )
                          })
                        }
                      />
                    </label>
                    {node.type==='Each / Loop'?<p className={styles.muted}>Configure per-item retries on the selected child workflow nodes. Whole-collection retries are disabled.{node.retry?.maxAttempts!==undefined&&node.retry.maxAttempts!==1&&<Button onClick={()=>editNode({retry:{maxAttempts:1,delayMs:1000,maxDelayMs:60000}})}>Use child retry policy</Button>}</p>:[
                      ['maxAttempts', 'Maximum attempts', 1],
                      ['delayMs', 'Retry delay (ms)', 1000],
                      ['maxDelayMs', 'Maximum retry delay (ms)', 60000]
                    ].map(([key, label, fallback]) => (
                      <label key={key}>
                        {label}
                        <input
                          type="number"
                          min={key === 'maxAttempts' ? 1 : 0}
                          value={
                            node.retry?.[
                              key as keyof NonNullable<typeof node.retry>
                            ] ?? fallback
                          }
                          onChange={(e) =>
                            editNode({
                              retry: {
                                maxAttempts: node.retry?.maxAttempts ?? 1,
                                delayMs: node.retry?.delayMs ?? 1000,
                                maxDelayMs: node.retry?.maxDelayMs ?? 60000,
                                [key]: Number(e.target.value)
                              }
                            })
                          }
                        />
                      </label>
                    ))}
                    <h3>Outgoing connections</h3>
                    {workflow.edges
                      .filter((e) => e.source === node.id)
                      .map((edge) => (
                        <div className={styles.edgeEditor} key={edge.id}>
                          <label>
                            Port
                            <select
                              value={edge.sourcePort ?? 'success'}
                              onChange={(e) =>
                                changeEdge(edge.id, edge.target, e.target.value)
                              }
                            >
                              {ports(node.type,node.config).map((p) => (
                                <option key={p}>{p}</option>
                              ))}
                            </select>
                          </label>
                          <label>
                            Target
                            <select
                              value={edge.target}
                              onChange={(e) =>
                                changeEdge(
                                  edge.id,
                                  e.target.value,
                                  edge.sourcePort ?? 'success'
                                )
                              }
                            >
                              {workflow.nodes
                                .filter(
                                  (n) =>
                                    n.id !== node.id &&
                                    !isTrigger(n.type)
                                )
                                .map((n) => (
                                  <option key={n.id} value={n.id}>
                                    {n.label}
                                  </option>
                                ))}
                            </select>
                          </label>
                          <Button
                            className={styles.iconButton}
                            aria-label="Remove connection"
                            onClick={() =>
                              update({
                                ...workflow,
                                edges: workflow.edges.filter(
                                  (e) => e.id !== edge.id
                                )
                              })
                            }
                          >
                            <Icon name="close" />
                          </Button>
                        </div>
                      ))}
                    <p className={styles.muted}>Drag from an output handle on the canvas to connect another node.</p>
                  </>
                ) : (
                  <>
                    <h3>Last simulation output</h3>
                    <pre>
                      {selectedResult?.output !== undefined
                        ? JSON.stringify(selectedResult.output, null, 2)
                        : '// Simulate this workflow to inspect node output'}
                    </pre>
                  </>
                )}
                <Button
                  visual="ghost"
                  tone="danger"
                  className={styles.removeNode}
                  onClick={() => {
                    update(removeNode(workflow, node.id))
                    setNodeId(undefined)
                  }}
                >
                  Remove node
                </Button>
              </div>
            </>
          ) : (
            <Empty title="Select a node">
              Choose a canvas node to edit its parameters and settings.
            </Empty>
          )}
        </aside>
      </div>
      <footer className={styles.editorStatus}>
        <span style={{ color: errors.length ? 'var(--fail)' : 'var(--ok)' }}>
          <i />
          {errors.length ? errors.length + ' graph issues' : 'Graph valid'}
        </span>
        <span>
          {workflow.nodes.length} nodes · {workflow.edges.length} connections
        </span>
        <span className={styles.statusSpacer} />
        <span>
          {simulate
            ? 'External requests use explicit response fixtures'
            : 'Draft changes take effect after saving and publishing'}
        </span>
      </footer>
    </>
  )
}
