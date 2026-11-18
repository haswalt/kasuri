import { lazy, Suspense, useEffect, useRef, useState } from 'react'
import { catalogue } from './lib/catalogue'
import { createWorkflow, parseWorkflow, uid, validateWorkflow } from './lib/workflows'
import type { Run, Workflow } from './lib/workflows'
import type { NodeManifest, RunMode } from '../../../packages/contracts/src/index'
import { api } from './lib/api'
import { RunDetails } from './components/RunDetails'
const WorkflowEditor = lazy(() => import('./components/WorkflowEditor').then(module => ({ default: module.WorkflowEditor })))
import './App.css'

type Page = 'Workflows' | 'Runs' | 'Connectors' | 'Connections'
const date = (value: string | number) => new Date(value).toLocaleString('en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })
const sample = JSON.stringify({ name: 'Ada', priority: 'urgent', title: 'Demo event', scheduledAt: new Date().toISOString() }, null, 2)
function App() {
  const [workflows, setWorkflows] = useState<Workflow[]>([])
  const [runs, setRuns] = useState<Run[]>([])
  const [connectors, setConnectors] = useState<NodeManifest[]>([])
  const [connected, setConnected] = useState(false)
  const [expiresAt, setExpiresAt] = useState<number | null>()
  const [page, setPage] = useState<Page>('Workflows')
  const [workflowId, setWorkflowId] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  const [filter, setFilter] = useState('All')
  const [notice, setNotice] = useState('')
  const [busy, setBusy] = useState(false)
  const [selectedRun, setSelectedRun] = useState<Run | null>(null)
  const [input, setInput] = useState(sample)
  const [dirty, setDirty] = useState(false)
  const dirtyIds = useRef(new Set<string>())
  const generation = useRef(0)
  const mounted = useRef(false)
  const fileInput = useRef<HTMLInputElement>(null)
  const workflow = workflows.find(w => w.id === workflowId)
  async function refresh() {
    const atGeneration = generation.current
    try {
      const data = await api.workspace()
      if (!mounted.current || atGeneration !== generation.current) return
      setWorkflows(previous => data.workflows.map(w => dirtyIds.current.has(w.id) ? previous.find(p => p.id === w.id) ?? w : w))
      setRuns(data.runs); setConnectors(data.connectors); setExpiresAt(data.expiresAt); setConnected(true)
    } catch (error) {
      if (!mounted.current) return
      setConnected(false); setNotice(error instanceof Error ? error.message : 'Could not reach the backend.')
    }
  }
  useEffect(() => {
    mounted.current = true; void refresh()
    const timer = setInterval(() => { void refresh() }, 3000)
    return () => { mounted.current = false; clearInterval(timer) }
  }, [])
  async function perform(action: () => Promise<void>) {
    if (busy) return
    setBusy(true); setNotice(''); generation.current++
    try { await action() } catch (error) { setNotice(error instanceof Error ? error.message : 'Action failed.') }
    finally { generation.current++; setBusy(false); void refresh() }
  }
  function replace(saved: Workflow) { dirtyIds.current.delete(saved.id); setWorkflows(ws => ws.some(w => w.id === saved.id) ? ws.map(w => w.id === saved.id ? saved : w) : [...ws, saved]); setDirty(false) }
  function update(next: Workflow) { generation.current++; dirtyIds.current.add(next.id); setDirty(true); setWorkflows(ws => ws.map(w => w.id === next.id ? { ...next, status: 'draft', nextScheduledAt: null } : w)) }
  async function save(w: Workflow) { const saved = await api.save(w); replace(saved); return saved }
  function open(w: Workflow) { setWorkflowId(w.id); setDirty(dirtyIds.current.has(w.id)); setNotice('') }
  function navigate(next: Page) { setPage(next); setWorkflowId(null); setSearch(''); setFilter('All'); setNotice('') }
  function create() { void perform(async () => { const w = await save(createWorkflow()); setPage('Workflows'); open(w) }) }
  function run(mode: RunMode) {
    if (!workflow) return
    void perform(async () => {
      const errors = validateWorkflow(workflow); if (errors.length) throw new Error(errors.join(' '))
      let payload
      try { payload = JSON.parse(input) } catch { throw new Error('Enter valid JSON in the test input.') }
      if (dirtyIds.current.has(workflow.id)) await save(workflow)
      const result = await api.run(workflow.id, mode, payload)
      setRuns(previous => [result, ...previous].slice(0, 100)); setSelectedRun(result)
    })
  }
  function publish() { if (workflow) void perform(async () => { const errors = validateWorkflow(workflow); if (errors.length) throw new Error(errors.join(' ')); if (dirtyIds.current.has(workflow.id)) await save(workflow); replace(await api.workflowAction(workflow.id, 'publish')); setNotice('Published. Schedule triggers are registered; webhook ingress is not enabled yet.') }) }
  function workflowAction(w: Workflow, action: string) { void perform(async () => { replace(await api.workflowAction(w.id, action)) }) }
  function exportWorkflow() {
    if (!workflow) return
    const url = URL.createObjectURL(new Blob([JSON.stringify(workflow, null, 2)], { type: 'application/json' }))
    const a = document.createElement('a'); a.href = url; a.download = `${workflow.name.replace(/[^a-z0-9]/gi, '-') || 'workflow'}.json`; a.click(); URL.revokeObjectURL(url)
  }
  function inspect(run: Run) { void perform(async () => { setSelectedRun(await api.getRun(run.id)) }) }
  const controlsDisabled = busy || !connected
  return <div className="app-shell">
    <aside className="sidebar">
      <a className="brand" href="#" onClick={e => { e.preventDefault(); navigate('Workflows') }}><span className="brand-mark">k</span> kasuri<span className="alpha">demo</span></a>
      <div className="workspace"><span className="workspace-icon">P</span><div>Demo workspace<small>{connected ? 'Connected to Rust engine' : 'Connecting to backend…'}</small></div></div>
      <nav aria-label="Main navigation">{(['Workflows', 'Runs', 'Connectors', 'Connections'] as Page[]).map((item, i) => <button key={item} className={page === item ? 'nav-item selected' : 'nav-item'} onClick={() => navigate(item)}><span aria-hidden="true">{['◇', '↗', '⊞', '⚿'][i]}</span>{item}{item === 'Workflows' && <small>{workflows.length}</small>}</button>)}</nav>
      <div className="sidebar-bottom"><span className={`local-dot ${connected ? 'online' : ''}`} />{connected ? 'Backend connected' : 'Backend offline'}<p>Private to this browser session.<br />{expiresAt ? `Demo expires ${date(expiresAt)}.` : expiresAt === null ? 'Workspace data persists on this host.' : 'Connecting…'}</p><div className="profile"><span className="avatar">Y</span><div>Your demo workspace<small>Core connectors · Rust + Wasm</small></div></div></div>
    </aside>
    <main>
      <header className="topbar"><span>Demo workspace <span className="slash">/</span> {page}{workflow && <> <span className="slash">/</span> {workflow.name || 'Untitled workflow'}</>}</span><span className="prototype-label">{connected ? 'Rust engine online' : 'Connecting'}</span></header>
      <div className="content">
        {notice && <div className="notice" role="status"><span>{notice}</span><button aria-label="Dismiss notification" onClick={() => setNotice('')}>×</button></div>}
        {workflow ? <>
          <div className="page-heading"><div><button className="back" onClick={() => setWorkflowId(null)}>← All workflows</button><h1>{workflow.name || 'Untitled workflow'} <span className={`badge ${workflow.status}`}>{workflow.status}</span></h1><p>{dirty ? 'Unsaved changes. Published schedules continue until you save this draft.' : 'Saved in your backend workspace.'}</p></div><div className="actions"><button onClick={exportWorkflow}>Export</button><button disabled={controlsDisabled || !dirty} onClick={() => void perform(async () => { await save(workflow); setNotice('Workflow saved.') })}>Save draft</button><button disabled={controlsDisabled} onClick={() => run('simulation')}>▷ Simulate</button><button disabled={controlsDisabled} onClick={() => run('run')}>▶ Run</button><button disabled={controlsDisabled} className="primary" onClick={publish}>Publish</button></div></div>
          <details className="test-input"><summary>Test input · JSON</summary><textarea aria-label="Test input JSON" value={input} onChange={e => setInput(e.target.value)} spellCheck={false} /><p>Run uses real waits. Simulate executes the same core logic with a virtual clock.</p></details>
          <fieldset disabled={busy} className="editor-fieldset"><Suspense fallback={<div className="empty">Loading workflow canvas…</div>}><WorkflowEditor key={workflow.id} workflow={workflow} connectors={connectors} update={update} /></Suspense></fieldset>
        </> : page === 'Workflows' ? <>
          <div className="page-heading"><div><div className="eyebrow">BUILD & AUTOMATE</div><h1>Workflows</h1><p>Build, run and test your automations with the Rust engine.</p></div><div className="actions"><button disabled={controlsDisabled} onClick={() => fileInput.current?.click()}>↑ Import JSON</button><button disabled={controlsDisabled} className="primary" onClick={create}>＋ New workflow</button></div></div>
          <div className="stats">{[['Total workflows', workflows.length], ['Published', workflows.filter(w => w.status === 'active').length], ['Backend runs', runs.length]].map(([title, value]) => <div className="stat" key={title}><span>{title}</span><strong>{value}<small>{title === 'Backend runs' ? 'Runs & simulations' : 'In this workspace'}</small></strong></div>)}</div>
          <section className="list-panel"><div className="list-toolbar"><div className="tabs">{['All', 'active', 'draft', 'paused'].map(tab => <button key={tab} className={filter === tab ? 'tab active-tab' : 'tab'} onClick={() => setFilter(tab)}>{tab === 'All' ? 'All workflows' : tab[0].toUpperCase() + tab.slice(1)}</button>)}</div><input className="search" aria-label="Search workflows" placeholder="Search workflows…" value={search} onChange={e => setSearch(e.target.value)} /></div><div className="table-scroll"><table><thead><tr><th>Workflow</th><th>Status</th><th>Steps</th><th>Updated / next run</th><th>Actions</th></tr></thead><tbody>{workflows.filter(w => (filter === 'All' || w.status === filter) && w.name.toLowerCase().includes(search.toLowerCase())).map(w => <tr key={w.id}><td><button className="workflow-link" onClick={() => open(w)}><span className="workflow-symbol">◇</span><span><strong>{w.name || 'Untitled workflow'}{dirtyIds.current.has(w.id) ? ' *' : ''}</strong><small>{w.description || 'No description yet'}</small></span></button></td><td><span className={`badge ${w.status}`}><i />{w.status}</span></td><td>{w.nodes.length} steps</td><td className="muted">{date(w.updatedAt)}{w.nextScheduledAt && <small className="next-run">Next: {date(w.nextScheduledAt)}</small>}</td><td><div className="row-actions"><button disabled={controlsDisabled} onClick={() => void perform(async () => { const clone = { ...structuredClone(w), id: uid(), name: `${w.name} (copy)`, status: 'draft' as const, revision: 0 }; const saved = await save(clone); open(saved) })}>Clone</button>{w.status !== 'draft' && <button disabled={controlsDisabled} onClick={() => workflowAction(w, w.status === 'active' ? 'pause' : 'resume')}>{w.status === 'active' ? 'Pause' : 'Resume'}</button>}</div></td></tr>)}</tbody></table></div>{!workflows.length && <div className="empty"><h2>{connected ? 'No workflows yet' : 'Connecting to your workspace'}</h2><p>{connected ? 'Create your first workflow.' : 'The backend provides demo workflows when connected.'}</p>{!connected && <button onClick={() => void refresh()}>Retry connection</button>}</div>}<div className="table-footer">{workflows.length} workflows in this workspace<span>Saved in backend storage</span></div></section>
          <section className="starter"><span className="starter-icon">⌘</span><div><h2>Start with the core demos</h2><p>Test mapping and JSON, conditional routing, or publish a scheduled heartbeat.</p></div><button disabled={controlsDisabled} onClick={create}>Create a workflow →</button></section>
        </> : page === 'Runs' ? <><div className="page-heading"><div><div className="eyebrow">OBSERVE</div><h1>Run history</h1><p>Backend execution, simulations and scheduled events. Updates every three seconds.</p></div></div><section className="list-panel">{runs.length ? runs.map(run => <button className="run-row" key={run.id} onClick={() => inspect(run)}><span className="run-icon">▷</span><div><strong>{run.name}</strong><small>{date(run.createdAt)} · Revision {run.revision} · {run.mode}</small></div><span className={`badge ${run.status}`}>{run.status}</span><span>{run.nodes.length} steps →</span></button>) : <div className="empty"><h2>No runs yet</h2><p>Open a workflow and select Run or Simulate.</p><button onClick={() => navigate('Workflows')}>View workflows</button></div>}</section></> : page === 'Connectors' ? <><div className="page-heading"><div><div className="eyebrow">EXTEND</div><h1>Node catalogue <span className="count">{catalogue.length}</span></h1><p>{connectors.length} implemented core nodes. SaaS and HTTP adapters are planned.</p></div><input className="search" aria-label="Search connectors" placeholder="Search nodes…" value={search} onChange={e => setSearch(e.target.value)} /></div><div className="catalogue-grid">{catalogue.filter(c => c.name.toLowerCase().includes(search.toLowerCase())).map(c => <article className="catalogue-card" key={c.name}><span className="catalogue-symbol">{c.name.slice(0, 1)}</span><span className={`badge ${connectors.some(m => m.kind === c.name) ? 'active' : ''}`}>{connectors.some(m => m.kind === c.name) ? 'Ready' : 'Planned'}</span><h2>{c.name}</h2><p>{c.category}</p></article>)}</div></> : <><div className="page-heading"><div><div className="eyebrow">CONNECT</div><h1>Connections</h1><p>External accounts will be added with the next connector adapters.</p></div></div><section className="list-panel empty"><div className="large-icon">⚿</div><h2>Core nodes need no credentials</h2><p>The demo executes only declarative core logic.<br />There are no live HTTP, OAuth or SaaS adapters yet.</p><button onClick={() => navigate('Connectors')}>Explore connectors →</button></section></>}
      </div>
    </main>
    <input ref={fileInput} hidden type="file" accept=".json,application/json" onChange={e => { const file = e.target.files?.[0]; if (!file) return; e.target.value = ''; void perform(async () => { if (file.size > 100_000) throw new Error('Demo imports must be smaller than 100 KB.'); const w = parseWorkflow(await file.text()); const saved = await save(w); setPage('Workflows'); open(saved) }) }} />
    {selectedRun && <RunDetails key={selectedRun.id} run={selectedRun} onChanged={() => void refresh()} onClose={() => { setSelectedRun(null); void refresh() }} onViewAll={() => { setSelectedRun(null); navigate('Runs') }} />}
  </div>
}
export default App
