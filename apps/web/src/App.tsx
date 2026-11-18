import { useRef, useState } from 'react'
import { catalogue, fields } from './lib/catalogue'
import { appendNode, createWorkflow, demoWorkflow, parseWorkflow, removeNode, uid, validateWorkflow } from './lib/workflows'
import type { Run, Workflow } from './lib/workflows'
import { RunDetails } from './components/RunDetails'
import './App.css'

type Page = 'Workflows' | 'Runs' | 'Connectors' | 'Connections'
const storageKey = 'kasuri-prototype-v1'
function readSaved(): { workflows: Workflow[]; runs: Run[] } {
  try {
    const saved = JSON.parse(localStorage.getItem(storageKey) || 'null')
    if (saved && Array.isArray(saved.workflows) && Array.isArray(saved.runs)) {
      // Validate persisted graph shape through the same boundary as imports.
      const workflows = saved.workflows.map((w: Workflow) => ({ ...parseWorkflow(JSON.stringify(w), false), id: w.id, status: ['draft', 'active', 'paused'].includes(w.status) ? w.status : 'draft', revision: Number.isInteger(w.revision) ? w.revision : 0, updatedAt: typeof w.updatedAt === 'string' && !Number.isNaN(Date.parse(w.updatedAt)) ? w.updatedAt : new Date().toISOString() }))
      const runs = saved.runs.filter((r: Run) => r && typeof r.id === 'string' && typeof r.name === 'string' && typeof r.createdAt === 'string' && Array.isArray(r.nodes) && r.nodes.every(n => n && typeof n.label === 'string' && typeof n.type === 'string'))
      return { workflows, runs }
    }
  } catch { /* Start with a demo when saved data is incompatible. */ }
  return { workflows: [demoWorkflow()], runs: [] }
}
const date = (value: string) => new Date(value).toLocaleString('en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })
function App() {
  const [initial] = useState(readSaved)
  const [workflows, setWorkflows] = useState(initial.workflows)
  const [runs, setRuns] = useState(initial.runs)
  const [page, setPage] = useState<Page>('Workflows')
  const [workflowId, setWorkflowId] = useState<string | null>(null)
  const [nodeId, setNodeId] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  const [filter, setFilter] = useState('All')
  const [palette, setPalette] = useState(false)
  const [notice, setNotice] = useState('')
  const [selectedRun, setSelectedRun] = useState<Run | null>(null)
  const fileInput = useRef<HTMLInputElement>(null)
  const workflow = workflows.find(w => w.id === workflowId)
  const node = workflow?.nodes.find(n => n.id === nodeId)
  function persist(nextWorkflows = workflows, nextRuns = runs) {
    setWorkflows(nextWorkflows); setRuns(nextRuns)
    try { localStorage.setItem(storageKey, JSON.stringify({ workflows: nextWorkflows, runs: nextRuns })) }
    catch { setNotice('Browser storage is unavailable or full. Export your workflow to keep a copy.') }
  }
  function update(next: Workflow) { persist(workflows.map(w => w.id === next.id ? { ...next, updatedAt: new Date().toISOString() } : w)) }
  function open(w: Workflow) { setWorkflowId(w.id); setNodeId(w.nodes[0]?.id ?? null); setPalette(false); setNotice('') }
  function create() { const w = createWorkflow(); persist([...workflows, w]); setPage('Workflows'); open(w) }
  function navigate(next: Page) { setPage(next); setWorkflowId(null); setSearch(''); setFilter('All'); setNotice('') }
  function simulate() {
    if (!workflow) return
    const errors = validateWorkflow(workflow)
    if (errors.length) { setNotice(errors.join(' ')); return }
    const run: Run = { id: uid(), workflowId: workflow.id, name: workflow.name, revision: workflow.revision, createdAt: new Date().toISOString(), nodes: workflow.nodes.map(n => ({ label: n.label, type: n.type })) }
    persist(workflows, [run, ...runs].slice(0, 100)); setSelectedRun(run); setNotice('Simulation complete. No requests were sent and no workflow code was executed.')
  }
  function publish() {
    if (!workflow) return
    const errors = validateWorkflow(workflow)
    if (errors.length) { setNotice(errors.join(' ')); return }
    update({ ...workflow, revision: workflow.revision + 1, status: 'active' }); setNotice('Published locally. Triggers remain disconnected in this prototype.')
  }
  function exportWorkflow() {
    if (!workflow) return
    const url = URL.createObjectURL(new Blob([JSON.stringify(workflow, null, 2)], { type: 'application/json' }))
    const a = document.createElement('a'); a.href = url; a.download = `${workflow.name.replace(/[^a-z0-9]/gi, '-').toLowerCase() || 'workflow'}.json`; a.click(); URL.revokeObjectURL(url)
  }
  return <div className="app-shell">
    <aside className="sidebar">
      <a className="brand" href="#" onClick={e => { e.preventDefault(); navigate('Workflows') }}><span className="brand-mark">k</span> kasuri<span className="alpha">alpha</span></a>
      <div className="workspace"><span className="workspace-icon">P</span><div>Personal workspace<small>Local prototype</small></div></div>
      <nav aria-label="Main navigation">{(['Workflows', 'Runs', 'Connectors', 'Connections'] as Page[]).map((item, i) => <button key={item} className={page === item ? 'nav-item selected' : 'nav-item'} onClick={() => navigate(item)}><span aria-hidden="true">{['◇', '↗', '⊞', '⚿'][i]}</span>{item}{item === 'Workflows' && <small>{workflows.length}</small>}</button>)}</nav>
      <div className="sidebar-bottom"><span className="local-dot" />Browser-only mode<p>Changes stay on this device.<br />No Cloudflare account connected.</p><div className="profile"><span className="avatar">Y</span><div>Your workspace<small>Single-user preview</small></div></div></div>
    </aside>
    <main>
      <header className="topbar"><span>Personal workspace <span className="slash">/</span> {page}{workflow && <> <span className="slash">/</span> {workflow.name || 'Untitled workflow'}</>}</span><span className="prototype-label">UI prototype</span></header>
      <div className="content">
        {notice && <div className="notice" role="status"><span>{notice}</span><button aria-label="Dismiss notification" onClick={() => setNotice('')}>×</button></div>}
        {workflow ? <>
          <div className="page-heading"><div><button className="back" onClick={() => setWorkflowId(null)}>← All workflows</button><h1>{workflow.name || 'Untitled workflow'} <span className={`badge ${workflow.status}`}>{workflow.status}</span></h1><p>Build your workflow one step at a time.</p></div><div className="actions"><button onClick={exportWorkflow}>Export</button><button onClick={simulate}>▷ Simulate run</button><button className="primary" onClick={publish}>Publish locally</button></div></div>
          <section className="workflow-settings"><label>Workflow name<input value={workflow.name} onChange={e => update({ ...workflow, name: e.target.value, status: 'draft' })} /></label><label>Description<input value={workflow.description} placeholder="What does this workflow do?" onChange={e => update({ ...workflow, description: e.target.value, status: 'draft' })} /></label><span>Revision {workflow.revision} · Saved in browser</span></section>
          <div className="editor-layout"><section className="canvas" aria-label="Workflow steps"><div className="canvas-caption">SEQUENTIAL WORKFLOW <span>{workflow.nodes.length} steps</span></div><div className="steps">{workflow.nodes.map((n, i) => <div key={n.id} className="step-wrapper">{i > 0 && <div className="connector-line">↓</div>}<button className={`node-card ${nodeId === n.id ? 'node-selected' : ''}`} onClick={() => { setNodeId(n.id); setPalette(false) }}><span className={`node-icon tone-${i % 4}`}>{i === 0 ? 'ϟ' : n.type === 'HTTP Request' ? '↗' : n.type === 'Slack' ? '#' : '⌘'}</span><div><small>{i === 0 ? 'TRIGGER' : `STEP ${i}`}</small><strong>{n.label}</strong><span>{n.type}</span></div><span className="node-options">···</span></button></div>)}<div className="connector-line">↓</div><button className="add-step" onClick={() => setPalette(true)}>＋ Add step</button></div><p className="canvas-note">Select a step to configure it. Branching and canvas editing come next.</p></section>
          <aside className="inspector">{palette ? <><div className="panel-heading"><h2>Add a step</h2><button onClick={() => setPalette(false)} aria-label="Close node palette">×</button></div><input aria-label="Search nodes" placeholder="Search nodes…" value={search} onChange={e => setSearch(e.target.value)} /><p className="muted">Catalogue entries are configuration placeholders.</p><div className="node-palette">{catalogue.filter(c => c.name.toLowerCase().includes(search.toLowerCase())).map(c => <button key={c.name} onClick={() => { const next = appendNode(workflow, c.name); update(next); setNodeId(next.nodes.at(-1)!.id); setPalette(false); setSearch('') }}><strong>{c.name}</strong><small>{c.category}</small><span>＋</span></button>)}</div></> : node ? <><div className="panel-heading"><h2>Step configuration</h2><span className="badge">{workflow.nodes.indexOf(node) + 1}</span></div><div className="inspector-type">{node.type}</div><label>Step name<input value={node.label} onChange={e => update({ ...workflow, status: 'draft', nodes: workflow.nodes.map(n => n.id === node.id ? { ...n, label: e.target.value } : n) })} /></label>{(fields[node.type] || []).map(field => <label key={field.key}>{field.label}<input value={node.config[field.key] || ''} placeholder={field.placeholder} onChange={e => update({ ...workflow, status: 'draft', nodes: workflow.nodes.map(n => n.id === node.id ? { ...n, config: { ...n.config, [field.key]: e.target.value } } : n) })} /></label>)}<div className="info-box">Configuration is saved locally. This step has no live execution adapter yet.</div><button className="danger subtle" onClick={() => { update(removeNode(workflow, node.id)); setNodeId(null) }}>Remove step</button></> : <div className="empty"><h2>Select a step</h2><p>Choose a step on the canvas to edit its configuration.</p></div>}</aside></div>
        </> : page === 'Workflows' ? <>
          <div className="page-heading"><div><div className="eyebrow">BUILD & AUTOMATE</div><h1>Workflows</h1><p>Your ideas, connected. Create and manage your automations.</p></div><div className="actions"><button onClick={() => fileInput.current?.click()}>↑ Import JSON</button><button className="primary" onClick={create}>＋ New workflow</button></div></div>
          <div className="stats">{[['Total workflows', workflows.length], ['Published locally', workflows.filter(w => w.status === 'active').length], ['Simulated runs', runs.length]].map(([title, value]) => <div className="stat" key={title}><span>{title}</span><strong>{value}<small>{title === 'Simulated runs' ? 'No live executions' : 'In this workspace'}</small></strong></div>)}</div>
          <section className="list-panel"><div className="list-toolbar"><div className="tabs">{['All', 'active', 'draft', 'paused'].map(tab => <button key={tab} className={filter === tab ? 'tab active-tab' : 'tab'} onClick={() => setFilter(tab)}>{tab === 'All' ? 'All workflows' : tab[0].toUpperCase() + tab.slice(1)}</button>)}</div><input className="search" aria-label="Search workflows" placeholder="Search workflows…" value={search} onChange={e => setSearch(e.target.value)} /></div><div className="table-scroll"><table><thead><tr><th>Workflow</th><th>Status</th><th>Steps</th><th>Updated</th><th>Actions</th></tr></thead><tbody>{workflows.filter(w => (filter === 'All' || w.status === filter) && w.name.toLowerCase().includes(search.toLowerCase())).map(w => <tr key={w.id}><td><button className="workflow-link" onClick={() => open(w)}><span className="workflow-symbol">◇</span><span><strong>{w.name || 'Untitled workflow'}</strong><small>{w.description || 'No description yet'}</small></span></button></td><td><span className={`badge ${w.status}`}><i />{w.status}</span></td><td>{w.nodes.length} steps</td><td className="muted">{date(w.updatedAt)}</td><td><div className="row-actions"><button onClick={() => { const clone = { ...structuredClone(w), id: uid(), name: `${w.name} (copy)`, status: 'draft' as const, revision: 0, updatedAt: new Date().toISOString() }; persist([...workflows, clone]); open(clone) }}>Clone</button>{w.status !== 'draft' && <button onClick={() => update({ ...w, status: w.status === 'active' ? 'paused' : 'active' })}>{w.status === 'active' ? 'Pause' : 'Resume'}</button>}</div></td></tr>)}</tbody></table></div>{!workflows.some(w => (filter === 'All' || w.status === filter) && w.name.toLowerCase().includes(search.toLowerCase())) && <div className="empty"><h2>No workflows found</h2><p>Try a different search or create your first workflow.</p><button onClick={create}>＋ New workflow</button></div>}<div className="table-footer">{workflows.length} workflows in this workspace<span>Stored on this device</span></div></section>
          <section className="starter"><span className="starter-icon">⌘</span><div><h2>Start with a simple flow</h2><p>Add a trigger, transform your data, then connect an action.</p></div><button onClick={create}>Create a workflow →</button></section>
        </> : page === 'Runs' ? <><div className="page-heading"><div><div className="eyebrow">OBSERVE</div><h1>Run history</h1><p>Inspect simulated step timelines. Live execution is not connected.</p></div></div><section className="list-panel">{runs.length ? runs.map(run => <button className="run-row" key={run.id} onClick={() => setSelectedRun(run)}><span className="run-icon">▷</span><div><strong>{run.name}</strong><small>{date(run.createdAt)} · Revision {run.revision}</small></div><span className="badge">Simulated</span><span>{run.nodes.length} steps →</span></button>) : <div className="empty"><h2>No runs yet</h2><p>Open a workflow and select “Simulate run” to preview its timeline.</p><button onClick={() => navigate('Workflows')}>View workflows</button></div>}</section></> : page === 'Connectors' ? <><div className="page-heading"><div><div className="eyebrow">EXTEND</div><h1>Node catalogue <span className="count">{catalogue.length}</span></h1><p>The 50 initial selections from the spec. Adapters are planned.</p></div><input className="search" aria-label="Search connectors" placeholder="Search nodes…" value={search} onChange={e => setSearch(e.target.value)} /></div><div className="catalogue-grid">{catalogue.filter(c => c.name.toLowerCase().includes(search.toLowerCase())).map(c => <article className="catalogue-card" key={c.name}><span className="catalogue-symbol">{c.name.slice(0, 1)}</span><span className="badge">Planned</span><h2>{c.name}</h2><p>{c.category}</p></article>)}</div>{!catalogue.some(c => c.name.toLowerCase().includes(search.toLowerCase())) && <div className="empty"><h2>No nodes found</h2><p>Try another search.</p></div>}</> : <><div className="page-heading"><div><div className="eyebrow">CONNECT</div><h1>Connections</h1><p>Manage the accounts your workflows will use.</p></div></div><section className="list-panel empty"><div className="large-icon">⚿</div><h2>No connected accounts</h2><p>Credential management will be added with the backend.<br />This preview does not collect or store API keys.</p><button onClick={() => navigate('Connectors')}>Explore planned connectors →</button></section></>}
      </div>
    </main>
    <input ref={fileInput} hidden type="file" accept=".json,application/json" onChange={async e => { const file = e.target.files?.[0]; if (!file) return; try { if (file.size > 1_000_000) throw new Error('Import must be smaller than 1 MB.'); const w = parseWorkflow(await file.text()); if (w.nodes.some(n => !catalogue.some(c => c.name === n.type))) throw new Error('The workflow contains an unknown node type.'); persist([...workflows, w]); open(w) } catch (error) { setNotice(error instanceof Error ? error.message : 'Could not import workflow.') } e.target.value = '' }} />
    {selectedRun && <RunDetails run={selectedRun} onClose={() => setSelectedRun(null)} onViewAll={() => { setSelectedRun(null); navigate('Runs') }} />}
  </div>
}
export default App
