import { useState } from 'react'
import type { Workflow } from '../lib/workflows'
import { removeNode } from '../lib/workflows'
import { connect, connectionError, ports } from '../lib/canvas'
import { WorkflowCanvas } from './WorkflowCanvas'
import { fields } from '../lib/catalogue'
import type { NodeManifest } from '../../../../packages/contracts/src/index'

export function WorkflowEditor({ workflow, connectors, update }: { workflow: Workflow; connectors: NodeManifest[]; update: (workflow: Workflow) => void }) {
  const [nodeId, setNodeId] = useState<string | undefined>(workflow.nodes[0]?.id)
  const [palette, setPalette] = useState(true)
  const [search, setSearch] = useState('')
  const [dragging, setDragging] = useState(false)
  const [addType, setAddType] = useState<string>()
  const [error, setError] = useState('')
  const node = workflow.nodes.find(n => n.id === nodeId)
  const triggers = ['Manual Trigger', 'Webhook', 'Schedule']
  function editNode(patch: Partial<Workflow['nodes'][number]>) { if (node) update({ ...workflow, status: 'draft', nodes: workflow.nodes.map(n => n.id === node.id ? { ...n, ...patch } : n) }) }
  function changeEdge(id: string, target: string, port: string) {
    try { update(connect(workflow, node!.id, target, port, id)); setError('') }
    catch (e) { setError(e instanceof Error ? e.message : 'Invalid connection.') }
  }
  function addConnection() {
    if (!node) return
    for (const port of ports(node.type)) for (const target of workflow.nodes) {
      if (!connectionError(workflow, node.id, target.id, port)) { update(connect(workflow, node.id, target.id, port)); setError(''); return }
    }
    setError('There are no additional valid targets for this node.')
  }
  return <>
    <section className="workflow-settings"><label>Workflow name<input value={workflow.name} onChange={e => update({ ...workflow, name: e.target.value, status: 'draft' })} /></label><label>Description<input value={workflow.description} placeholder="What does this workflow do?" onChange={e => update({ ...workflow, description: e.target.value, status: 'draft' })} /></label><span>Revision {workflow.revision}</span></section>
    {workflow.nextScheduledAt && <div className="info-box">Next scheduled run: {new Date(workflow.nextScheduledAt).toLocaleString('en-GB')}. Saving a draft pauses the schedule until you publish again.</div>}
    <div className="editor-layout"><WorkflowCanvas workflow={workflow} selectedId={nodeId} select={id => { setNodeId(id); if (id) setPalette(false) }} update={update} showPalette={() => setPalette(true)} dragging={dragging} allowedTypes={connectors.map(c => c.kind)} addType={addType} onAdded={() => setAddType(undefined)} />
    <aside className="inspector">{palette ? <><div className="panel-heading"><h2>Add a step</h2><button onClick={() => setPalette(false)} aria-label="Close node palette">×</button></div><input aria-label="Search nodes" placeholder="Search implemented nodes…" value={search} onChange={e => setSearch(e.target.value)} /><p className="muted">Drag a node onto the canvas, or click to add it. Drop on a line to insert between steps.</p><div className="node-palette">{connectors.filter(c => (!triggers.includes(c.kind) || !workflow.nodes.some(n => triggers.includes(n.type))) && c.kind.toLowerCase().includes(search.toLowerCase())).map(c => <button key={c.kind} draggable onDragStart={event => { setDragging(true); event.dataTransfer.setData('application/kasuri-node', c.kind); event.dataTransfer.setData('text/plain', c.kind); event.dataTransfer.effectAllowed = 'copy' }} onDragEnd={() => setDragging(false)} onClick={() => setAddType(c.kind)} title={`Drag ${c.kind} onto the canvas, or click to add`}><strong>{c.kind}</strong><small>Version {c.version}</small><span>＋</span></button>)}</div></> : node ? <><div className="panel-heading"><h2>Step configuration</h2><span className="badge">v{node.version ?? 1}</span></div><div className="inspector-type">{node.type}</div>{triggers.includes(node.type) && <label>Trigger type<select value={node.type} onChange={e => editNode({ type: e.target.value, label: e.target.value, config: e.target.value === 'Schedule' ? { cron: '* * * * *', timezone: 'UTC' } : {} })}>{triggers.map(t => <option key={t}>{t}</option>)}</select></label>}<label>Step name<input value={node.label} onChange={e => editNode({ label: e.target.value })} /></label>{(fields[node.type] || []).map(field => <label key={field.key}>{field.label}<input value={typeof node.config[field.key] === 'string' ? node.config[field.key] as string : node.config[field.key] === undefined ? '' : JSON.stringify(node.config[field.key])} placeholder={field.placeholder} onChange={e => editNode({ config: { ...node.config, [field.key]: e.target.value } })} /></label>)}
      <h2 className="section-title">Outgoing connections</h2>{error && <p role="alert" className="danger">{error}</p>}{workflow.edges.filter(e => e.source === node.id).map(edge => <div className="edge-editor" key={edge.id}><label>Port<select aria-label="Output port" value={edge.sourcePort ?? 'success'} onChange={e => changeEdge(edge.id, edge.target, e.target.value)}>{(node.type === 'IF / Condition' ? ['true', 'false', 'error'] : ['success', 'error']).map(port => <option key={port}>{port}</option>)}</select></label><label>Target<select aria-label="Target step" value={edge.target} onChange={e => changeEdge(edge.id, e.target.value, edge.sourcePort ?? 'success')}>{workflow.nodes.filter(n => n.id !== node.id && !triggers.includes(n.type)).map(n => <option key={n.id} value={n.id}>{n.label}</option>)}</select></label><button aria-label="Remove connection" onClick={() => update({ ...workflow, status: 'draft', edges: workflow.edges.filter(e => e.id !== edge.id) })}>×</button></div>)}<button className="subtle" disabled={!workflow.nodes.some(n => n.id !== node.id && !triggers.includes(n.type))} onClick={addConnection}>＋ Add connection</button>
      <div className="info-box">{node.type === 'Map / Transform' ? 'Use {"$input":"/name"} to reference an input field. Other values are JSON literals.' : node.type === 'Schedule' ? 'Five-field cron, minimum one minute. Publish to register; pause to unregister. Timezone uses IANA names.' : 'Runs execute this node in the Rust backend. Simulations use the same logic with a virtual clock.'}</div><button className="danger subtle" onClick={() => { update(removeNode(workflow, node.id)); setNodeId(undefined) }}>Remove step</button>
    </> : <div className="empty"><h2>Select a step</h2><p>Choose a step to edit its configuration.</p></div>}</aside></div>
  </>
}
