import { lazy, Suspense, useEffect, useRef, useState } from 'react'
import { catalogue } from './lib/catalogue'
import {
  createWorkflow,
  parseWorkflow,
  uid,
  validateWorkflow
} from './lib/workflows'
import type { Workflow, Run } from './lib/workflows'
import type { Json, NodeManifest } from '../../../packages/contracts/src/index'
import { api } from './lib/api'
import { useRoute, go, path, savedTheme, applyTheme } from './lib/navigation'
import type { Theme } from './lib/navigation'
import {
  Brand,
  Icon,
  Tile,
  Pill,
  PageHeader,
  Empty,
  Search,
  Menu,
  Dialog
} from './design/ui'
import {
  date,
  download,
  runState,
  tone,
  category,
  csvCell
} from './design/ui-model'
import { Settings } from './components/Settings'
import { RunDetails } from './components/RunDetails'
const WorkflowEditor = lazy(() =>
  import('./components/WorkflowEditor').then((m) => ({
    default: m.WorkflowEditor
  }))
)
import './App.css'
const nav = [
  ['workflows', 'Workflows', 'diamond'],
  ['runs', 'Runs', 'runs'],
  ['connectors', 'Connectors', 'grid'],
  ['connections', 'Connections', 'key'],
  ['mcp', 'MCP server', 'plug'],
  ['audit', 'Audit log', 'file']
]
const titles = Object.fromEntries(nav.map(([key, title]) => [key, title]))
export default function App() {
  const route = useRoute()
  const [workflows, setWorkflows] = useState<Workflow[]>([]),
    [runs, setRuns] = useState<Run[]>([]),
    [connectors, setConnectors] = useState<NodeManifest[]>([])
  const [connected, setConnected] = useState(false),
    [loaded, setLoaded] = useState(false),
    [busy, setBusy] = useState(false),
    [notice, setNotice] = useState('')
  const [search, setSearch] = useState(''),
    [filter, setFilter] = useState('All'),
    [runWorkflow, setRunWorkflow] = useState('all'),
    [trigger, setTrigger] = useState('all'),
    [timeRange, setTimeRange] = useState('all'),
    [categoryFilter, setCategoryFilter] = useState('All')
  const [theme, setThemeState] = useState<Theme>(savedTheme),
    [help, setHelp] = useState(false),
    [input, setInput] = useState('{}'),
    [selectedRun, setSelectedRun] = useState<Run | null>(null),
    [runError, setRunError] = useState('')
  const [runTarget, setRunTarget] = useState<Workflow | null>(null),
    [runFormError, setRunFormError] = useState('')
  const [dirtyIds, setDirtyIds] = useState<Set<string>>(() => new Set()),
    [undoAvailable, setUndoAvailable] = useState(false),
    [clock, setClock] = useState(() => Date.now())
  const [fixtures, setFixtures] = useState<Record<string, Json>>({})
  const toastTimer = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined
  )
  const mounted = useRef(false),
    generation = useRef(0),
    dirty = useRef(new Set<string>()),
    fileInput = useRef<HTMLInputElement>(null),
    undo = useRef<(() => Promise<void>) | null>(null)
  const workflow = workflows.find((w) => w.id === route.workflowId)
  const editor = Boolean(route.workflowId)
  const selectedId = route.runId || route.query.get('run')
  async function refresh() {
    const at = generation.current
    try {
      const data = await api.workspace()
      if (!mounted.current || at !== generation.current) return
      setWorkflows((previous) =>
        data.workflows.map((w) =>
          dirty.current.has(w.id)
            ? (previous.find((p) => p.id === w.id) ?? w)
            : w
        )
      )
      setRuns(data.runs)
      setClock(Date.now())
      setConnectors(data.connectors)
      setConnected(true)
      setLoaded(true)
    } catch (e) {
      if (mounted.current) {
        setConnected(false)
        setLoaded(true)
        setNotice(
          e instanceof Error ? e.message : 'Could not reach the backend.'
        )
      }
    }
  }
  useEffect(() => {
    mounted.current = true
    void refresh()
    const timer = setInterval(() => {
      if (!document.querySelector('[role=menu]')) void refresh()
    }, 5000)
    return () => {
      mounted.current = false
      clearInterval(timer)
    }
  }, [])
  useEffect(() => {
    setSearch('')
    setFilter('All')
  }, [route.page])
  useEffect(() => {
    if (!notice) return
    toastTimer.current = setTimeout(() => {
      setNotice('')
      undo.current = null
      setUndoAvailable(false)
    }, 6000)
    return () => clearTimeout(toastTimer.current)
  }, [notice])
  useEffect(() => {
    if (!selectedId) {
      setSelectedRun(null)
      setRunError('')
      return
    }
    let active = true
    setRunError('')
    void api
      .getRun(selectedId)
      .then((run) => {
        if (active) setSelectedRun(run)
      })
      .catch((e) => {
        if (active)
          setRunError(e instanceof Error ? e.message : 'Could not load run.')
      })
    return () => {
      active = false
    }
  }, [selectedId])
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (
        (e.target as HTMLElement).closest(
          'input,textarea,select,[contenteditable=true]'
        )
      )
        return
      if (e.key === '/') {
        e.preventDefault()
        document.querySelector<HTMLInputElement>('input[data-search]')?.focus()
      }
      if (e.key === '?') {
        e.preventDefault()
        setHelp(true)
      }
    }
    addEventListener('keydown', key)
    return () => removeEventListener('keydown', key)
  }, [])
  const resolvedDark =
    theme === 'dark' ||
    (theme === 'system' && matchMedia('(prefers-color-scheme: dark)').matches)
  function setTheme(value: Theme) {
    setThemeState(value)
    applyTheme(value)
  }
  async function perform(action: () => Promise<void>) {
    if (busy) return
    setBusy(true)
    setNotice('')
    undo.current = null
    setUndoAvailable(false)
    generation.current++
    try {
      await action()
    } catch (e) {
      setNotice(e instanceof Error ? e.message : 'Action failed.')
    } finally {
      generation.current++
      setBusy(false)
      void refresh()
    }
  }
  function replace(w: Workflow) {
    dirty.current.delete(w.id)
    setDirtyIds(new Set(dirty.current))
    setWorkflows((ws) =>
      ws.some((v) => v.id === w.id)
        ? ws.map((v) => (v.id === w.id ? w : v))
        : [...ws, w]
    )
  }
  function update(w: Workflow) {
    generation.current++
    dirty.current.add(w.id)
    setDirtyIds(new Set(dirty.current))
    setWorkflows((ws) =>
      ws.map((v) =>
        v.id === w.id ? { ...w, status: 'draft', nextScheduledAt: null } : v
      )
    )
  }
  async function save(w: Workflow) {
    const saved = await api.save(w)
    replace(saved)
    return saved
  }
  function open(w: Workflow) {
    go(path('workflows/' + w.id + '/edit'))
    setNotice('')
    setInput(JSON.stringify(fixtures[w.id] ?? {}, null, 2))
  }
  function create() {
    void perform(async () => {
      const w = await save(createWorkflow())
      open(w)
    })
  }
  function navigate(page: string) {
    go(path(page))
    setNotice('')
  }
  function inspect(run: Run) {
    const q = new URLSearchParams(route.query)
    q.set('run', run.id)
    q.delete('step')
    go(location.pathname + '?' + q)
  }
  function closeRun() {
    const q = new URLSearchParams(route.query)
    q.delete('run')
    q.delete('step')
    go(location.pathname + (q.size ? '?' + q : ''), true)
  }
  function changeStep(index: number) {
    const q = new URLSearchParams(location.search)
    q.set('step', String(index))
    go(location.pathname + '?' + q, true)
  }
  function act(w: Workflow, action: string) {
    void perform(async () => {
      if (dirty.current.has(w.id)) await save(w)
      replace(await api.workflowAction(w.id, action))
      setNotice(
        action === 'publish'
          ? 'Workflow published.'
          : action === 'pause'
            ? 'Workflow paused. New trigger events are rejected.'
            : 'Workflow resumed.'
      )
      if (action === 'pause' || action === 'resume') {
        setUndoAvailable(true)
        undo.current = async () => {
          replace(
            await api.workflowAction(
              w.id,
              action === 'pause' ? 'resume' : 'pause'
            )
          )
          setNotice('Change undone.')
          undo.current = null
          setUndoAvailable(false)
        }
      }
    })
  }
  function publish() {
    if (!workflow) return
    const errors = validateWorkflow(workflow)
    if (errors.length) {
      setNotice(errors.join(' '))
      return
    }
    act(workflow, 'publish')
  }
  function clone(w: Workflow) {
    void perform(async () => {
      const copy = await save({
        ...structuredClone(w),
        id: uid(),
        name: w.name + ' (copy)',
        status: 'draft',
        revision: 0
      })
      open(copy)
    })
  }
  async function execute(mode: 'run' | 'simulation', target = workflow) {
    if (!target) return
    const parsed = JSON.parse(input)
    if (dirty.current.has(target.id)) await save(target)
    const run = await api.run(target.id, mode, parsed)
    inspect(run)
  }
  function showRun(target: Workflow) {
    setRunFormError('')
    setRunTarget(target)
    if (target.id !== workflow?.id)
      setInput(JSON.stringify(fixtures[target.id] ?? {}, null, 2))
  }
  async function startLive() {
    if (!runTarget || busy) return
    setBusy(true)
    generation.current++
    setRunFormError('')
    try {
      await execute('run', runTarget)
      setRunTarget(null)
    } catch (e) {
      setRunFormError(e instanceof Error ? e.message : 'Could not start run.')
    } finally {
      generation.current++
      setBusy(false)
      void refresh()
    }
  }
  const disabled = busy || !connected
  const recent = runs.filter(
    (r) => clock - new Date(r.createdAt).getTime() < 86400000
  )
  const completed = recent.filter((r) =>
    ['succeeded', 'failed'].includes(r.status)
  )
  const statuses = [
    'queued',
    'running',
    'waiting',
    'retrying',
    'succeeded',
    'failed',
    'cancelled'
  ]
  const runCounts = Object.fromEntries(
    statuses.map((s) => [s, recent.filter((r) => runState(r) === s).length])
  )
  const visibleWorkflows = workflows.filter(
    (w) =>
      (filter === 'All' || w.status === filter.toLowerCase()) &&
      (w.name + ' ' + w.description)
        .toLowerCase()
        .includes(search.toLowerCase())
  )
  const visibleRuns = runs.filter(
    (r) =>
      (filter === 'All' || runState(r) === filter.toLowerCase()) &&
      (runWorkflow === 'all' || r.workflowId === runWorkflow) &&
      (trigger === 'all' || r.mode === trigger) &&
      (timeRange === 'all' ||
        clock - new Date(r.createdAt).getTime() < Number(timeRange)) &&
      (r.id + ' ' + r.name + ' ' + r.nodes.map((n) => n.label).join(' '))
        .toLowerCase()
        .includes(search.toLowerCase())
  )
  const categories = [
    'All',
    'Core',
    'Logic & data',
    'AI',
    'Cloudflare',
    'Apps',
    'Monitoring'
  ]
  const group = (name: string) => {
    const c = category(name)
    return c === 'trigger' || name === 'HTTP Request'
      ? 'Core'
      : c === 'logic'
        ? 'Logic & data'
        : c === 'http'
          ? 'AI'
          : c === 'cf'
            ? 'Cloudflare'
            : c === 'error'
              ? 'Monitoring'
              : 'Apps'
  }
  const visibleConnectors = catalogue.filter(
    (c) =>
      (categoryFilter === 'All' || group(c.name) === categoryFilter) &&
      c.name.toLowerCase().includes(search.toLowerCase())
  )
  const editorToolbar = workflow && (
    <>
      <div className="editor-topbar">
        <button
          className="brand-back"
          aria-label="All workflows"
          onClick={() => navigate('workflows')}
        >
          <Brand compact />
          <Icon name="arrow" />
          <span>All workflows</span>
        </button>
        <div className="editor-title">
          <input
            aria-label="Workflow name"
            value={workflow.name}
            onChange={(e) => update({ ...workflow, name: e.target.value })}
          />
          <Pill
            status={workflow.status}
            label={
              (dirtyIds.has(workflow.id)
                ? 'Draft'
                : workflow.status[0].toUpperCase() + workflow.status.slice(1)) +
              ' v' +
              workflow.revision
            }
          />
          <small>
            {dirtyIds.has(workflow.id) ? 'Unsaved changes' : 'Saved'}
          </small>
        </div>
        <div className="editor-actions">
          <span className="segmented">
            {(['build', 'simulate'] as const).map((mode) => (
              <button
                key={mode}
                aria-pressed={route.mode === mode}
                onClick={() =>
                  go(
                    path('workflows/' + workflow.id + '/edit') +
                      (mode === 'simulate' ? '?mode=simulate' : '')
                  )
                }
              >
                <Icon name={mode === 'build' ? 'pencil' : 'play'} />
                {mode === 'build' ? 'Build' : 'Simulate'}
              </button>
            ))}
          </span>
          <button
            className="ghost live-control"
            role="switch"
            aria-checked={workflow.status === 'active'}
            disabled={disabled || workflow.status === 'draft'}
            title={
              workflow.status === 'draft'
                ? 'Publish this draft to activate triggers'
                : undefined
            }
            onClick={() =>
              act(workflow, workflow.status === 'active' ? 'pause' : 'resume')
            }
          >
            <span
              className={
                'switch-track ' + (workflow.status === 'active' ? 'on' : '')
              }
            />
            {workflow.status === 'active' ? 'Live active' : 'Not active'}
          </button>
          <button
            disabled={disabled || !dirtyIds.has(workflow.id)}
            onClick={() =>
              void perform(async () => {
                await save(workflow)
                setNotice('Draft saved.')
              })
            }
          >
            Save
          </button>
          <button onClick={() => download(workflow.id + '.json', workflow)}>
            <Icon name="download" />
            Export
          </button>
          <button className="primary" disabled={disabled} onClick={publish}>
            Publish v{workflow.revision + 1}
          </button>
        </div>
      </div>
    </>
  )
  if (route.page === 'login' || route.page === 'setup')
    return (
      <div className="login-screen">
        <section className="login-art">
          <Brand />
          <div className="decorative-workflow" aria-hidden="true">
            {['Webhook', 'Map / Transform', 'HTTP Request', 'Merge'].map(
              (kind, i) => (
                <div className="decorative-step" key={kind}>
                  <span className={'decorative-node decorative-' + i}>
                    <Tile kind={kind} />
                    <div>
                      <small>
                        {i < 2 ? 'SUCCEEDED' : i === 2 ? 'RUNNING' : 'PENDING'}
                      </small>
                      <strong>{kind}</strong>
                    </div>
                    <Icon
                      name={i < 2 ? 'check' : i === 2 ? 'refresh' : 'more'}
                    />
                  </span>
                  {i < 3 && <span className="decorative-line" />}
                </div>
              )
            )}
          </div>
          <h1>
            Durable workflows.
            <br />
            On your own infrastructure.
          </h1>
          <p>Build visually. Inspect every step. Keep control of your data.</p>
          <div className="login-benefits">
            {[
              'Persistent runs and checkpoint recovery',
              'Declarative connectors and safe simulations',
              'Rust + WebAssembly on Cloudflare or workerd'
            ].map((t) => (
              <span key={t}>
                <Icon name="check" />
                {t}
              </span>
            ))}
          </div>
        </section>
        <section className="login-form">
          <div>
            <span className="eyebrow">YOUR WORKFLOW WORKSPACE</span>
            <h1>
              {route.page === 'setup' ? 'Set up Kasuri' : 'Sign in to Kasuri'}
            </h1>
            <p className="mono">{location.host}</p>
            <div className="info-box">
              <Icon name="lock" />
              <h3>Host-managed access</h3>
              <p>
                This version uses private browser sessions and optional
                installation Basic authentication or Cloudflare Access. Account
                sign-in and owner bootstrap are planned.
              </p>
            </div>
            <button className="primary" onClick={() => navigate('workflows')}>
              Continue to workspace <Icon name="runs" />
            </button>
            <a
              href="https://github.com/haswalt/kasuri/tree/develop/docs"
              target="_blank"
              rel="noreferrer"
            >
              Deployment documentation <Icon name="external" size={12} />
            </a>
            <div className="login-footer">
              <span className="connection-status">
                <i className={connected ? 'online' : ''} />
                {connected
                  ? 'API connected'
                  : 'Host authentication may be required'}
              </span>
              <span>Open source · alpha</span>
            </div>
          </div>
        </section>
      </div>
    )
  return (
    <div className={editor ? 'editor-shell' : 'app-shell'}>
      {!editor && (
        <aside className="sidebar">
          <a
            href={path('workflows')}
            className="brand-link"
            onClick={(e) => {
              e.preventDefault()
              navigate('workflows')
            }}
          >
            <Brand />
          </a>
          <Menu
            label={
              <>
                <span className="workspace-avatar">P</span>
                <span className="account-text">
                  <strong>Personal workspace</strong>
                  <small>Session access · {workflows.length} workflows</small>
                </span>
                <Icon name="chevrons" />
              </>
            }
          >
            <div className="menu-caption">WORKSPACES</div>
            <button onClick={() => navigate('workflows')}>
              <span className="workspace-avatar">P</span>Personal workspace
              <Icon name="check" />
            </button>
            <hr />
            <button
              disabled
              title="Named workspace creation is not implemented"
            >
              Create workspace <small>Planned</small>
            </button>
            <button onClick={() => navigate('settings/workspaces')}>
              Manage workspaces
            </button>
          </Menu>
          <nav aria-label="Main">
            {nav.map(([key, label, icon]) => (
              <a
                key={key}
                href={path(key)}
                aria-current={route.page === key ? 'page' : undefined}
                className={
                  route.page === key ? 'nav-item selected' : 'nav-item'
                }
                onClick={(e) => {
                  e.preventDefault()
                  navigate(key)
                }}
              >
                <Icon name={icon} />
                {label}
                {key === 'workflows' && (
                  <span className="nav-count">{workflows.length}</span>
                )}
              </a>
            ))}
          </nav>
          <div className="sidebar-bottom">
            <div className="connection-status">
              <i className={connected ? 'online' : ''} />
              {connected ? 'Backend connected' : 'Connecting to backend'}
            </div>
            <Menu
              up
              label={
                <>
                  <span className="avatar">P</span>
                  <span className="account-text">
                    <strong>Your workspace</strong>
                    <small>Private browser session</small>
                  </span>
                  <Icon name="chevrons" />
                </>
              }
            >
              <div className="menu-caption">
                <strong>Browser session</strong>
                <small>Personal workspace</small>
              </div>
              <button onClick={() => navigate('settings/profile')}>
                <Icon name="sliders" />
                Settings
              </button>
              <button onClick={() => navigate('settings/members')}>
                <Icon name="key" />
                Members & roles
              </button>
              <button onClick={() => setTheme(resolvedDark ? 'light' : 'dark')}>
                <Icon name="moon" />
                {resolvedDark ? 'Light' : 'Dark'} theme
              </button>
              <button onClick={() => setHelp(true)}>
                <Icon name="command" />
                Keyboard shortcuts<kbd>?</kbd>
              </button>
              <a
                href="https://github.com/haswalt/kasuri/tree/develop/docs"
                target="_blank"
                rel="noreferrer"
              >
                <Icon name="book" />
                Documentation
              </a>
              <hr />
              <button
                disabled
                title="Account sessions are managed by your host"
              >
                Sign out <small>Host managed</small>
              </button>
            </Menu>
          </div>
        </aside>
      )}
      <main className={editor ? 'editor-main' : 'app-main'}>
        {editor ? (
          editorToolbar
        ) : (
          <header className="topbar">
            <span className="muted">Personal workspace</span>
            <span className="slash">/</span>
            <span>
              {route.page === 'settings'
                ? 'Settings'
                : titles[route.page] || 'Workflows'}
            </span>
            <span className="topbar-right">
              <span className="connection-status">
                <i className={connected ? 'online' : ''} />
                {route.page === 'runs'
                  ? 'Live · updates every 5 s'
                  : location.host}
              </span>
            </span>
          </header>
        )}
        {notice && (
          <div
            className="notice"
            role="status"
            onMouseEnter={() => clearTimeout(toastTimer.current)}
            onMouseLeave={() => {
              toastTimer.current = setTimeout(() => {
                setNotice('')
                undo.current = null
                setUndoAvailable(false)
              }, 6000)
            }}
          >
            <Icon name="warning" />
            <span>{notice}</span>
            {undoAvailable && (
              <button className="ghost" onClick={() => void undo.current?.()}>
                Undo
              </button>
            )}
            <button
              className="icon-button"
              aria-label="Dismiss notification"
              onClick={() => {
                setNotice('')
                undo.current = null
                setUndoAvailable(false)
              }}
            >
              <Icon name="close" />
            </button>
          </div>
        )}
        {editor ? (
          workflow ? (
            <Suspense
              fallback={<div className="empty">Loading workflow editor…</div>}
            >
              <WorkflowEditor
                key={workflow.id}
                workflow={workflow}
                connectors={connectors}
                update={update}
                mode={route.mode}
                input={input}
                setInput={setInput}
                busy={disabled}
                runSimulation={async () => {
                  if (dirtyIds.has(workflow.id)) await save(workflow)
                  return api.run(workflow.id, 'simulation', JSON.parse(input))
                }}
                runLive={() => showRun(workflow)}
              />
            </Suspense>
          ) : (
            <Empty
              title={loaded ? 'Workflow not found' : 'Loading workflow…'}
              action={
                <button onClick={() => navigate('workflows')}>
                  All workflows
                </button>
              }
            >
              Your workflow data is loaded from this browser's workspace.
            </Empty>
          )
        ) : (
          <div className="page-content">
            {!loaded ? (
              <>
                <div className="skeleton skeleton-title" />
                <div className="panel skeleton-list">
                  {Array.from({ length: 6 }, (_, i) => (
                    <div className="skeleton" key={i} />
                  ))}
                </div>
              </>
            ) : !connected ? (
              <section className="error-panel" role="alert">
                <Icon name="warning" />
                <h2>Could not connect to your workspace</h2>
                <p>{notice || 'The backend is unavailable.'}</p>
                <button onClick={() => void refresh()}>
                  <Icon name="refresh" />
                  Try again
                </button>
              </section>
            ) : route.page === 'workflows' ? (
              <>
                <PageHeader
                  eyebrow="BUILD & AUTOMATE"
                  title="Workflows"
                  description={
                    workflows.length +
                    ' workflows · ' +
                    workflows.filter((w) => w.status === 'active').length +
                    ' active · run states for the last 24 hours'
                  }
                  actions={
                    <>
                      <button
                        disabled={disabled}
                        onClick={() => fileInput.current?.click()}
                      >
                        <Icon name="upload" />
                        Import JSON
                      </button>
                      <button
                        className="primary"
                        disabled={disabled}
                        onClick={create}
                      >
                        <Icon name="plus" />
                        New workflow
                      </button>
                    </>
                  }
                />
                <section className="stat-strip">
                  <div className="stat-grid">
                    {[
                      [
                        'Runs (24h)',
                        recent.length,
                        'From retained run history'
                      ],
                      [
                        'Success rate',
                        completed.length
                          ? Math.round(
                              (completed.filter((r) => r.status === 'succeeded')
                                .length /
                                completed.length) *
                                100
                            ) + '%'
                          : '—',
                        'Completed runs'
                      ],
                      ['Failed', runCounts.failed, 'Runs needing attention'],
                      [
                        'Waiting',
                        runCounts.waiting,
                        'Durable waits in progress'
                      ]
                    ].map(([label, value, sub]) => (
                      <div className="stat" key={label}>
                        <span>{label}</span>
                        <strong>{value}</strong>
                        <small>{sub}</small>
                      </div>
                    ))}
                  </div>
                  <div className="state-bar">
                    {statuses
                      .filter((s) => runCounts[s])
                      .map((s) => (
                        <span
                          key={s}
                          style={{ flex: runCounts[s], background: tone(s).fg }}
                          title={s + ': ' + runCounts[s]}
                        />
                      ))}
                  </div>
                  <div className="state-legend">
                    {statuses.map((s) => (
                      <span key={s}>
                        <i style={{ background: tone(s).fg }} />
                        {tone(s).label}
                        <b>{runCounts[s]}</b>
                      </span>
                    ))}
                  </div>
                </section>
                <section className="panel">
                  <div className="list-toolbar">
                    <div className="tabs">
                      {['All', 'Active', 'Draft', 'Paused'].map((tab) => (
                        <button
                          key={tab}
                          aria-pressed={filter === tab}
                          onClick={() => setFilter(tab)}
                        >
                          {tab}
                          <small>
                            {tab === 'All'
                              ? workflows.length
                              : workflows.filter(
                                  (w) => w.status === tab.toLowerCase()
                                ).length}
                          </small>
                        </button>
                      ))}
                    </div>
                    <Search
                      value={search}
                      onChange={setSearch}
                      placeholder="Search workflows…"
                    />
                  </div>
                  <div className="table-scroll">
                    <table>
                      <thead>
                        <tr>
                          <th>Workflow</th>
                          <th>Status</th>
                          <th>Last run</th>
                          <th className="numeric">Runs 24h</th>
                          <th>Trigger</th>
                          <th>Updated</th>
                          <th />
                        </tr>
                      </thead>
                      <tbody>
                        {visibleWorkflows.map((w) => {
                          const latest = runs.find((r) => r.workflowId === w.id)
                          const trigger = w.nodes.find((n) =>
                            ['Manual Trigger', 'Webhook', 'Schedule'].includes(
                              n.type
                            )
                          )
                          return (
                            <tr key={w.id}>
                              <td>
                                <button
                                  className="name-cell workflow-link"
                                  onClick={() => open(w)}
                                >
                                  <span className="workflow-tile">
                                    <Icon name="diamond" />
                                  </span>
                                  <span>
                                    <strong>
                                      {w.name || 'Untitled workflow'}
                                      {dirtyIds.has(w.id) && ' *'}
                                    </strong>
                                    <small>
                                      {w.description ||
                                        w.nodes.length +
                                          ' nodes · revision ' +
                                          w.revision}
                                    </small>
                                  </span>
                                </button>
                              </td>
                              <td>
                                <Pill status={w.status} />
                              </td>
                              <td>
                                {latest ? (
                                  <button
                                    className="last-run ghost"
                                    onClick={() => inspect(latest)}
                                  >
                                    <span>
                                      <i
                                        style={{
                                          background: tone(runState(latest)).fg
                                        }}
                                      />
                                      {tone(runState(latest)).label}
                                    </span>
                                    <small>{date(latest.createdAt)}</small>
                                  </button>
                                ) : (
                                  <span className="muted">No runs yet</span>
                                )}
                              </td>
                              <td className="numeric">
                                {
                                  recent.filter((r) => r.workflowId === w.id)
                                    .length
                                }
                              </td>
                              <td>
                                <code className="trigger-label">
                                  {trigger?.type === 'Schedule'
                                    ? 'cron ' +
                                      String(trigger.config.cron || '')
                                    : trigger?.type === 'Webhook'
                                      ? 'POST · webhook'
                                      : 'Manual'}
                                </code>
                              </td>
                              <td className="muted nowrap">
                                {date(w.updatedAt)}
                              </td>
                              <td>
                                <Menu iconOnly label={<Icon name="more" />}>
                                  {w.status === 'draft' ? (
                                    <button onClick={() => act(w, 'publish')}>
                                      Publish & activate
                                    </button>
                                  ) : (
                                    <button
                                      onClick={() =>
                                        act(
                                          w,
                                          w.status === 'active'
                                            ? 'pause'
                                            : 'resume'
                                        )
                                      }
                                    >
                                      {w.status === 'active'
                                        ? 'Pause'
                                        : 'Resume'}
                                    </button>
                                  )}
                                  <button onClick={() => showRun(w)}>
                                    Run now…
                                  </button>
                                  <button onClick={() => clone(w)}>
                                    Duplicate
                                  </button>
                                  <button
                                    onClick={() => download(w.id + '.json', w)}
                                  >
                                    Export JSON
                                  </button>
                                  <hr />
                                  <button
                                    className="danger"
                                    disabled
                                    title="Deletion and restore are not implemented"
                                  >
                                    Delete <small>Planned</small>
                                  </button>
                                </Menu>
                              </td>
                            </tr>
                          )
                        })}
                      </tbody>
                    </table>
                  </div>
                  {!visibleWorkflows.length && (
                    <Empty
                      title={
                        search
                          ? 'No matching workflows'
                          : 'Your next workflow starts here'
                      }
                      action={
                        !search ? (
                          <button className="primary" onClick={create}>
                            <Icon name="plus" />
                            Create a workflow
                          </button>
                        ) : undefined
                      }
                    >
                      Connect a trigger, add your logic, and put it to work.
                    </Empty>
                  )}
                  <div className="table-footer">
                    <span>
                      Showing {visibleWorkflows.length} of {workflows.length}{' '}
                      workflows
                    </span>
                    <button className="ghost" onClick={() => navigate('runs')}>
                      View run history <Icon name="runs" />
                    </button>
                  </div>
                </section>
              </>
            ) : route.page === 'runs' && route.runId ? (
              <div aria-live="polite">{!selectedRun && 'Loading run…'}</div>
            ) : route.page === 'runs' ? (
              <>
                <PageHeader
                  eyebrow="OBSERVE"
                  title="Runs"
                  description="Every retained execution across your workflows. Select a run to inspect it step by step."
                  actions={
                    <button
                      onClick={() =>
                        download(
                          'kasuri-runs.csv',
                          [
                            'Run ID,Workflow,State,Trigger,Started',
                            ...visibleRuns.map((r) =>
                              [r.id, r.name, runState(r), r.mode, r.createdAt]
                                .map(csvCell)
                                .join(',')
                            )
                          ].join('\n'),
                          'text/csv'
                        )
                      }
                    >
                      <Icon name="download" />
                      Export CSV
                    </button>
                  }
                />
                <section className="panel">
                  <div className="list-toolbar">
                    <div className="tabs">
                      {[
                        'All',
                        'Running',
                        'Waiting',
                        'Retrying',
                        'Paused',
                        'Failed',
                        'Succeeded',
                        'Cancelled'
                      ].map((tab) => (
                        <button
                          key={tab}
                          aria-pressed={filter === tab}
                          onClick={() => setFilter(tab)}
                        >
                          {tab}
                          <small>
                            {tab === 'All'
                              ? runs.length
                              : runs.filter(
                                  (r) => runState(r) === tab.toLowerCase()
                                ).length}
                          </small>
                        </button>
                      ))}
                    </div>
                  </div>
                  <div className="filter-row">
                    <select
                      aria-label="Filter by workflow"
                      value={runWorkflow}
                      onChange={(e) => setRunWorkflow(e.target.value)}
                    >
                      <option value="all">All workflows</option>
                      {workflows.map((w) => (
                        <option key={w.id} value={w.id}>
                          {w.name}
                        </option>
                      ))}
                    </select>
                    <select
                      aria-label="Filter by trigger"
                      value={trigger}
                      onChange={(e) => setTrigger(e.target.value)}
                    >
                      <option value="all">All triggers</option>
                      {['run', 'simulation', 'schedule', 'webhook'].map((t) => (
                        <option key={t}>{t}</option>
                      ))}
                    </select>
                    <select
                      aria-label="Filter by time"
                      value={timeRange}
                      onChange={(e) => setTimeRange(e.target.value)}
                    >
                      <option value="all">All retained history</option>
                      <option value="86400000">Last 24 hours</option>
                      <option value="604800000">Last 7 days</option>
                    </select>
                    <Search
                      value={search}
                      onChange={setSearch}
                      placeholder="Run ID, workflow or node…"
                    />
                  </div>
                  <table>
                    <thead>
                      <tr>
                        <th>Run</th>
                        <th>Workflow</th>
                        <th>Status</th>
                        <th>Trigger</th>
                        <th>Started</th>
                        <th className="numeric">Steps</th>
                        <th className="numeric">Retries</th>
                      </tr>
                    </thead>
                    <tbody>
                      {visibleRuns.map((r) => (
                        <tr key={r.id}>
                          <td>
                            <button
                              className="ghost mono"
                              onClick={() => inspect(r)}
                            >
                              {r.id.slice(0, 12)}…
                            </button>
                          </td>
                          <td>
                            <button
                              className="ghost text-link"
                              onClick={() => inspect(r)}
                            >
                              {r.name}
                            </button>
                            <small className="block">
                              Revision v{r.revision}
                            </small>
                          </td>
                          <td>
                            <Pill status={runState(r)} />
                          </td>
                          <td className="muted">{r.mode}</td>
                          <td className="muted nowrap">{date(r.createdAt)}</td>
                          <td className="numeric mono">
                            {
                              r.nodes.filter((n) =>
                                ['succeeded', 'failed', 'skipped'].includes(
                                  n.status
                                )
                              ).length
                            }{' '}
                            / {r.nodes.length}
                          </td>
                          <td className="numeric mono">
                            {r.nodes.reduce(
                              (sum, n) => sum + Math.max(0, n.attempts - 1),
                              0
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  {!visibleRuns.length && (
                    <Empty
                      icon="runs"
                      title={runs.length ? 'No matching runs' : 'No runs yet'}
                      action={
                        <button onClick={() => navigate('workflows')}>
                          View workflows
                        </button>
                      }
                    >
                      Open a workflow and run it, or simulate it with test
                      input.
                    </Empty>
                  )}
                  <div className="table-footer">
                    Showing {visibleRuns.length} of {runs.length} retained runs
                    · newest first
                  </div>
                </section>
              </>
            ) : route.page === 'connectors' ? (
              <>
                <PageHeader
                  eyebrow="LIBRARY"
                  title="Connectors"
                  description="Versioned nodes and integrations for your workflows."
                  actions={
                    <button onClick={() => create()}>
                      <Icon name="plus" />
                      New workflow
                    </button>
                  }
                />
                <div className="catalogue-toolbar">
                  <div className="tabs">
                    {categories.map((c) => (
                      <button
                        key={c}
                        aria-pressed={categoryFilter === c}
                        onClick={() => setCategoryFilter(c)}
                      >
                        {c}
                        <small>
                          {
                            catalogue.filter(
                              (x) => c === 'All' || group(x.name) === c
                            ).length
                          }
                        </small>
                      </button>
                    ))}
                  </div>
                  <Search
                    value={search}
                    onChange={setSearch}
                    placeholder="Search connectors…"
                  />
                </div>
                <div className="catalogue-grid">
                  {visibleConnectors.map((c) => {
                    const manifest = connectors.find((m) => m.kind === c.name)
                    return (
                      <button
                        className="catalogue-card"
                        key={c.name}
                        onClick={() => {
                          go(
                            path('connections') +
                              '?connector=' +
                              encodeURIComponent(c.name)
                          )
                        }}
                      >
                        <div className="catalogue-card-top">
                          <Tile kind={c.name} />
                          {manifest ? (
                            <span className="built-in">Built in</span>
                          ) : (
                            <span className="chip">Planned</span>
                          )}
                        </div>
                        <div className="eyebrow">{group(c.name)}</div>
                        <h2>{c.name}</h2>
                        <p>
                          {manifest
                            ? c.name === 'HTTP Request'
                              ? 'Public HTTPS, Basic auth and custom headers.'
                              : manifest.hasExternalSideEffects
                                ? 'Durable execution with explicit side effects.'
                                : 'Ready to use in your workflow.'
                            : 'On the connector roadmap.'}
                        </p>
                        <div className="capability-chips">
                          {manifest ? (
                            <>
                              <span>
                                {category(c.name) === 'trigger'
                                  ? 'Trigger'
                                  : 'Actions'}
                              </span>
                              <span>v{manifest.version}</span>
                              {!manifest.hasExternalSideEffects && (
                                <span>No credentials</span>
                              )}
                            </>
                          ) : (
                            <span>Not implemented</span>
                          )}
                        </div>
                      </button>
                    )
                  })}
                </div>
                <div className="catalogue-footer">
                  <Icon name="code" />
                  <span>
                    Need a service that isn't listed? The HTTP Request node
                    connects to public HTTPS APIs.
                  </span>
                  <small>
                    Showing {visibleConnectors.length} of {catalogue.length}
                  </small>
                </div>
              </>
            ) : route.page === 'settings' ? (
              <Settings
                section={route.section}
                theme={theme}
                setTheme={setTheme}
                workflowCount={workflows.length}
                connected={connected}
              />
            ) : route.page === 'connections' ? (
              <>
                <PageHeader
                  eyebrow="CREDENTIALS"
                  title="Connections"
                  description="Accounts and credentials your workflows use."
                  actions={
                    <button
                      disabled
                      title="The credential vault API is planned"
                    >
                      <Icon name="plus" />
                      New connection
                    </button>
                  }
                />
                <div className="connections-layout">
                  <section className="panel">
                    <div className="list-toolbar">
                      <div className="tabs">
                        {['All', 'Needs attention', 'OAuth', 'MCP servers'].map(
                          (t) => (
                            <button
                              key={t}
                              aria-pressed={filter === t}
                              onClick={() => setFilter(t)}
                            >
                              {t}
                            </button>
                          )
                        )}
                      </div>
                    </div>
                    <Empty
                      icon="key"
                      title={
                        filter === 'MCP servers'
                          ? 'MCP connections are planned'
                          : 'Host-managed credentials'
                      }
                    >
                      HTTP Basic authentication and sensitive custom headers use
                      named secrets configured on your installation. A saved
                      connection vault and OAuth management are not available
                      yet.
                    </Empty>
                  </section>
                  <aside className="panel connection-detail">
                    <Tile
                      kind={route.query.get('connector') || 'HTTP Request'}
                      size={36}
                    />
                    <h2>{route.query.get('connector') || 'HTTP Request'}</h2>
                    <Pill
                      status={
                        connectors.some(
                          (c) =>
                            c.kind ===
                            (route.query.get('connector') || 'HTTP Request')
                        )
                          ? 'active'
                          : 'planned'
                      }
                      label={
                        connectors.some(
                          (c) =>
                            c.kind ===
                            (route.query.get('connector') || 'HTTP Request')
                        )
                          ? 'Available'
                          : 'Planned'
                      }
                    />
                    <p>
                      For HTTP Request, select Basic authentication and
                      reference a password secret by name. Custom credential
                      headers use a <code>$secret</code> reference.
                    </p>
                    <div className="info-box">
                      Secret values stay on the host and are redacted from
                      echoed responses.
                    </div>
                    <button onClick={() => navigate('workflows')}>
                      Open workflows <Icon name="runs" />
                    </button>
                  </aside>
                </div>
              </>
            ) : route.page === 'mcp' ? (
              <>
                <PageHeader
                  eyebrow="MODEL CONTEXT PROTOCOL"
                  title="MCP server"
                  description="Let external agents connect to Kasuri to build, manage and audit workflows."
                  actions={
                    <button disabled title="The MCP server is not implemented">
                      <Icon name="plus" />
                      Expose a workflow
                    </button>
                  }
                />
                <section className="stat-strip">
                  <div className="stat-grid">
                    {[
                      ['Endpoint', 'Not configured'],
                      ['Transport', 'Planned'],
                      ['Authentication', 'Scoped credentials'],
                      ['Status', 'Not implemented']
                    ].map(([label, value]) => (
                      <div className="stat" key={label}>
                        <span>{label}</span>
                        <strong className="small-stat">{value}</strong>
                      </div>
                    ))}
                  </div>
                </section>
                <div className="connections-layout">
                  <section className="panel">
                    <div className="panel-heading">
                      <h2>Workflows exposed as tools</h2>
                      <span className="chip">Planned</span>
                    </div>
                    <Empty
                      icon="plug"
                      title="Agent management is on the roadmap"
                    >
                      The MCP endpoint will share workflow validation and
                      execution with this app. No management endpoint or client
                      tokens have been issued.
                    </Empty>
                  </section>
                  <aside className="panel connection-detail">
                    <Icon name="plug" size={28} />
                    <h2>Two directions, distinct access</h2>
                    <p>
                      This page is for agents connecting to Kasuri. Outbound MCP
                      servers and Cloudflare MCP Portals will live under
                      Connections.
                    </p>
                    <button onClick={() => navigate('connections')}>
                      View connections <Icon name="runs" />
                    </button>
                  </aside>
                </div>
              </>
            ) : route.page === 'audit' ? (
              <>
                <PageHeader
                  eyebrow="GOVERNANCE"
                  title="Audit log"
                  description="Attributable workflow changes and agent activity."
                />
                <section className="panel">
                  <Empty
                    icon="file"
                    title="Audit records are not available yet"
                    action={
                      <button onClick={() => navigate('runs')}>
                        View run history
                      </button>
                    }
                  >
                    An append-only management audit API is planned. Current
                    execution outcomes are available in Runs.
                  </Empty>
                </section>
              </>
            ) : route.page === 'login' || route.page === 'setup' ? (
              <section className="panel">
                <Empty
                  icon="lock"
                  title="Authentication is managed by this host"
                >
                  This version uses browser workspace sessions and optional
                  installation Basic authentication or Cloudflare Access.
                  Account sign-in and owner bootstrap screens require the
                  planned account API.
                </Empty>
              </section>
            ) : (
              <Empty
                title="Page not found"
                action={
                  <button onClick={() => navigate('workflows')}>
                    All workflows
                  </button>
                }
              >
                Choose a page from the sidebar.
              </Empty>
            )}
          </div>
        )}
      </main>
      <input
        ref={fileInput}
        hidden
        type="file"
        accept=".json,application/json"
        onChange={(e) => {
          const file = e.target.files?.[0]
          if (!file) return
          e.target.value = ''
          void perform(async () => {
            if (file.size > 100000)
              throw new Error('Workflow imports must be smaller than 100 KB.')
            const w = await save(parseWorkflow(await file.text()))
            open(w)
          })
        }}
      />
      {selectedId && runError && (
        <Dialog title="Could not load run" onClose={closeRun}>
          <div className="settings-fields">
            <p role="alert">{runError}</p>
            <button
              onClick={() => {
                setRunError('')
                void api
                  .getRun(selectedId)
                  .then(setSelectedRun)
                  .catch((e) => setRunError(String(e)))
              }}
            >
              Try again
            </button>
          </div>
        </Dialog>
      )}
      {selectedId && selectedRun?.id === selectedId && !runError && (
        <RunDetails
          key={selectedId}
          run={selectedRun}
          onChanged={() => void refresh()}
          onClose={route.runId ? () => navigate('runs') : closeRun}
          onViewAll={() => navigate('runs')}
          onOpenPage={() => go(path('runs/' + selectedId))}
          fullPage={Boolean(route.runId)}
          initialStep={Number(route.query.get('step') || '-1')}
          onStep={changeStep}
          onPin={(value) => {
            setFixtures((f) => ({ ...f, [selectedRun.workflowId]: value }))
            setInput(JSON.stringify(value, null, 2))
            setNotice('Run input pinned for this browser session.')
            go(
              path('workflows/' + selectedRun.workflowId + '/edit') +
                '?mode=simulate'
            )
          }}
        />
      )}
      {runTarget && (
        <Dialog
          title={'Run ' + runTarget.name}
          onClose={() => {
            if (!busy) setRunTarget(null)
          }}
        >
          <div className="settings-fields">
            <p>
              This starts a real run. Configured HTTP nodes can call external
              services; waits use durable timers.
            </p>
            <label>
              Run input · JSON
              <textarea
                aria-label="Run input JSON"
                className="mono"
                rows={10}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                spellCheck={false}
              />
            </label>
            {runFormError && (
              <p className="danger" role="alert">
                {runFormError}
              </p>
            )}
            <div className="actions">
              <button disabled={busy} onClick={() => setRunTarget(null)}>
                Cancel
              </button>
              <button
                className="primary"
                disabled={busy}
                onClick={() => void startLive()}
              >
                <Icon name="play" />
                {busy ? 'Starting…' : 'Start run'}
              </button>
            </div>
          </div>
        </Dialog>
      )}
      {help && (
        <Dialog title="Keyboard shortcuts" onClose={() => setHelp(false)}>
          <div className="shortcut-list">
            {[
              ['/', 'Search this page or the node palette'],
              ['?', 'Open keyboard shortcuts'],
              ['Esc', 'Close a menu or dialog'],
              ['← / →', 'Previous / next step in the run debugger'],
              ['Delete', 'Remove selected canvas nodes or edges']
            ].map(([key, label]) => (
              <div key={key}>
                <kbd>{key}</kbd>
                <span>{label}</span>
              </div>
            ))}
          </div>
        </Dialog>
      )}
    </div>
  )
}
