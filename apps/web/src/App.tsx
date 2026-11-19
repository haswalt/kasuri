import { McpManagement, AuditLog } from './components/McpManagement'
import { Button } from './design/ui'
import { styles } from './design/styles'
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
import type { WorkspaceMembership } from './lib/api'
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
import { DataTables } from './components/DataTables'
import { WorkflowTrash } from './components/WorkflowTrash'
import { WorkflowHistory } from './components/WorkflowHistory'
import { Settings } from './components/Settings'
import { RunDetails } from './components/RunDetails'
const WorkflowEditor = lazy(() =>
  import('./components/WorkflowEditor').then((m) => ({
    default: m.WorkflowEditor
  }))
)
const nav = [
  ['workflows', 'Workflows', 'diamond'],
  ['runs', 'Runs', 'runs'],
  ['data', 'Data tables', 'database'],
  ['connectors', 'Connectors', 'grid'],
  ['connections', 'Connections', 'key'],
  ['mcp', 'MCP server', 'plug'],
  ['audit', 'Audit log', 'file']
]
const titles = Object.fromEntries(nav.map(([key, title]) => [key, title]))
export default function WorkspaceApp() {
  const route = useRoute()
  return <App key={route.workspaceKey} />
}
function App() {
  const route = useRoute()
  const [memberships, setMemberships] = useState<WorkspaceMembership[]>([])
  const [workspaceDialog, setWorkspaceDialog] = useState<'create' | 'rename' | null>(null)
  const [workspaceName, setWorkspaceName] = useState('')
  const [historyTarget, setHistoryTarget] = useState<Workflow | null>(null)
  const [archiveTarget, setArchiveTarget] = useState<Workflow | null>(null)
  const [showTrash, setShowTrash] = useState(false)
  const workspaceLabel = memberships.find((w) => w.id === route.workspaceKey)?.name || (route.workspaceKey === 'personal' ? 'Personal workspace' : 'Workspace')
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
    [timeRange, setTimeRange] = useState('all')
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
      const directory = await api.workspaces()
      if (!mounted.current || at !== generation.current) return
      setWorkflows((previous) =>
        data.workflows.map((w) =>
          dirty.current.has(w.id)
            ? (previous.find((p) => p.id === w.id) ?? w)
            : w
        )
      )
      setMemberships(directory.workspaces)
      setRuns(data.runs)
      setClock(Date.now())
      setConnectors(data.connectors)
      setConnected(true)
      setLoaded(true)
    } catch (e) {
      if (mounted.current && at === generation.current) {
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
    if (!mounted.current) return
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
        v.id === w.id ? { ...w, status: v.status, lifecycle: v.lifecycle, nextScheduledAt: v.nextScheduledAt, hasUnpublishedChanges: true } : v
      )
    )
  }
  async function save(w: Workflow) {
    const saved = await api.save(w)
    replace(saved)
    return saved
  }
  function open(w: Workflow) {
    if (!mounted.current) return
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
      const current = dirty.current.has(w.id) ? await save(w) : w
      const changed = await api.workflowAction(
        current.id,
        action,
        current.version
      )
      replace(changed)
      setNotice(
        action === 'publish'
          ? changed.revision === current.revision ? 'No publication changes. The live revision and activation state are unchanged.' : 'Workflow published and activated.'
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
              action === 'pause' ? 'resume' : 'pause',
              changed.version
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
    const current = dirty.current.has(target.id) ? await save(target) : target
    const run = await api.run(current.id, mode, parsed, current.version)
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
  const categoryFilter = route.query.get('category') || 'All'
  const connectorSearch = route.query.get('q') || ''
  const connectorStatus = route.query.get('status') || 'all'
  const setConnectorQuery = (key: string, value: string) => {
    const query = new URLSearchParams(route.query)
    if (value && value !== 'All' && value !== 'all') query.set(key, value)
    else query.delete(key)
    go(path('connectors') + (query.size ? '?' + query.toString() : ''))
  }
  const visibleConnectors = catalogue.filter(
    (c) =>
      (categoryFilter === 'All' || group(c.name) === categoryFilter) &&
      c.name.toLowerCase().includes(connectorSearch.toLowerCase()) &&
      (connectorStatus === 'all' ||
        (connectorStatus === 'built-in' ? connectors.some((m) => m.kind === c.name) : !connectors.some((m) => m.kind === c.name)))
  )
  const editorToolbar = workflow && (
    <>
      <div className={styles.editorTopbar}>
        <Button
          className={styles.brandBack}
          aria-label="All workflows"
          onClick={() => navigate('workflows')}
        >
          <Brand compact />
          <Icon name="arrow" />
          <span>All workflows</span>
        </Button>
        <div className={styles.editorTitle}>
          <input
            aria-label="Workflow name"
            value={workflow.name}
            onChange={(e) => update({ ...workflow, name: e.target.value })}
          />
          <Pill
            status={workflow.status}
            label={
              workflow.status[0].toUpperCase() + workflow.status.slice(1) +
              (workflow.lifecycle?.publishedRevision ? ' · live v' + workflow.lifecycle.publishedRevision : '')
            }
          />
          <small>
            {dirtyIds.has(workflow.id) ? 'Unsaved draft' : workflow.hasUnpublishedChanges ? 'Draft changes saved' : 'Saved'}
          </small>
        </div>
        <div className={styles.editorActions}>
          <span className={styles.segmented}>
            {(['build', 'simulate'] as const).map((mode) => (
              <Button
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
              </Button>
            ))}
          </span>
          <Button
            visual="ghost"
            className={'live-control'}
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
                styles.switchTrack +
                ' ' +
                (workflow.status === 'active' ? 'on' : '')
              }
            />
            {workflow.status === 'active' ? 'Live active' : 'Not active'}
          </Button>
          <Button
            disabled={disabled || !dirtyIds.has(workflow.id)}
            onClick={() =>
              void perform(async () => {
                await save(workflow)
                setNotice('Draft saved.')
              })
            }
          >
            Save
          </Button>
          <Button onClick={() => download(workflow.id + '.json', workflow)}>
            <Icon name="download" />
            Export
          </Button>
          <Button onClick={() => setHistoryTarget(workflow)}>History</Button>
          <Button visual="primary" disabled={disabled || (!dirtyIds.has(workflow.id) && workflow.hasUnpublishedChanges === false)} onClick={publish}>
            {workflow.hasUnpublishedChanges === false && !dirtyIds.has(workflow.id) ? 'Published' : 'Publish & activate v' + (workflow.revision + 1)}
          </Button>
        </div>
      </div>
    </>
  )
  if (route.page === 'login' || route.page === 'setup')
    return (
      <div className={styles.loginScreen}>
        <section className={styles.loginArt}>
          <Brand />
          <div className={styles.decorativeWorkflow} aria-hidden="true">
            {['Webhook', 'Map / Transform', 'HTTP Request', 'Merge'].map(
              (kind, i) => (
                <div className={styles.decorativeStep} key={kind}>
                  <span
                    className={styles.decorativeNode + ' ' + 'decorative-' + i}
                  >
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
                  {i < 3 && <span className={styles.decorativeLine} />}
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
          <div className={styles.loginBenefits}>
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
        <section className={styles.loginForm}>
          <div>
            <span className={styles.eyebrow}>YOUR WORKFLOW WORKSPACE</span>
            <h1>
              {route.page === 'setup' ? 'Set up Kasuri' : 'Sign in to Kasuri'}
            </h1>
            <p className={styles.mono}>{location.host}</p>
            <div className={styles.infoBox}>
              <Icon name="lock" />
              <h3>Host-managed access</h3>
              <p>
                This version uses private browser sessions and optional
                installation Basic authentication or Cloudflare Access. Account
                sign-in and owner bootstrap are planned.
              </p>
            </div>
            <Button visual="primary" onClick={() => navigate('workflows')}>
              Continue to workspace <Icon name="runs" />
            </Button>
            <a
              href="https://github.com/haswalt/kasuri/tree/develop/docs"
              target="_blank"
              rel="noreferrer"
            >
              Deployment documentation <Icon name="external" size={12} />
            </a>
            <div className={styles.loginFooter}>
              <span className={styles.connectionStatus}>
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
    <div className={editor ? styles.editorShell : styles.appShell}>
      {!editor && (
        <aside className={styles.sidebar}>
          <a
            href={path('workflows')}
            className={styles.brandLink}
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
                <span className={styles.workspaceAvatar}>P</span>
                <span className={styles.accountText}>
                  <strong>{workspaceLabel}</strong>
                  <small>Session access · {workflows.length} workflows</small>
                </span>
                <Icon name="chevrons" />
              </>
            }
          >
            <div className={styles.menuCaption}>WORKSPACES</div>
            {memberships.map((entry) => (
              <Button key={entry.id} disabled={busy || dirtyIds.size > 0} title={dirtyIds.size ? 'Save your draft before switching workspaces' : undefined} onClick={() => go('/w/' + entry.id + '/workflows')}>
                <span className={styles.workspaceAvatar}>{entry.name[0]}</span>{entry.name}
                {entry.id === route.workspaceKey && <Icon name="check" />}
              </Button>
            ))}
            <hr />
            <Button disabled={busy} onClick={() => { setWorkspaceName(''); setWorkspaceDialog('create') }}>Create workspace</Button>
            <Button disabled={busy} onClick={() => { setWorkspaceName(workspaceLabel); setWorkspaceDialog('rename') }}>Rename workspace</Button>
            <Button onClick={() => navigate('settings/workspaces')}>
              Manage workspaces
            </Button>
          </Menu>
          <nav aria-label="Main">
            {nav.map(([key, label, icon]) => (
              <a
                key={key}
                href={path(key)}
                aria-current={route.page === key ? 'page' : undefined}
                className={
                  route.page === key
                    ? styles.navItem + ' ' + 'selected'
                    : styles.navItem
                }
                onClick={(e) => {
                  e.preventDefault()
                  navigate(key)
                }}
              >
                <Icon name={icon} />
                {label}
                {key === 'workflows' && (
                  <span className={styles.navCount}>{workflows.length}</span>
                )}
              </a>
            ))}
          </nav>
          <div className={styles.sidebarBottom}>
            <div className={styles.connectionStatus}>
              <i className={connected ? 'online' : ''} />
              {connected ? 'Backend connected' : 'Connecting to backend'}
            </div>
            <Menu
              up
              label={
                <>
                  <span className={styles.avatar}>P</span>
                  <span className={styles.accountText}>
                    <strong>Your workspace</strong>
                    <small>Private browser session</small>
                  </span>
                  <Icon name="chevrons" />
                </>
              }
            >
              <div className={styles.menuCaption}>
                <strong>Browser session</strong>
                <small>{workspaceLabel}</small>
              </div>
              <Button onClick={() => navigate('settings/profile')}>
                <Icon name="sliders" />
                Settings
              </Button>
              <Button onClick={() => navigate('settings/members')}>
                <Icon name="key" />
                Members & roles
              </Button>
              <Button onClick={() => setTheme(resolvedDark ? 'light' : 'dark')}>
                <Icon name="moon" />
                {resolvedDark ? 'Light' : 'Dark'} theme
              </Button>
              <Button onClick={() => setHelp(true)}>
                <Icon name="command" />
                Keyboard shortcuts<kbd>?</kbd>
              </Button>
              <a
                href="https://github.com/haswalt/kasuri/tree/develop/docs"
                target="_blank"
                rel="noreferrer"
              >
                <Icon name="book" />
                Documentation
              </a>
              <hr />
              <Button
                disabled
                title="Account sessions are managed by your host"
              >
                Sign out <small>Host managed</small>
              </Button>
            </Menu>
          </div>
        </aside>
      )}
      <main className={editor ? styles.editorMain : styles.appMain}>
        {editor ? (
          editorToolbar
        ) : (
          <header className={styles.topbar}>
            <span className={styles.muted}>{workspaceLabel}</span>
            <span className={styles.slash}>/</span>
            <span>
              {route.page === 'settings'
                ? 'Settings'
                : titles[route.page] || 'Workflows'}
            </span>
            <span className={styles.topbarRight}>
              <span className={styles.connectionStatus}>
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
            className={styles.notice}
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
              <Button visual="ghost" onClick={() => void undo.current?.()}>
                Undo
              </Button>
            )}
            <Button
              className={styles.iconButton}
              aria-label="Dismiss notification"
              onClick={() => {
                setNotice('')
                undo.current = null
                setUndoAvailable(false)
              }}
            >
              <Icon name="close" />
            </Button>
          </div>
        )}
        {editor ? (
          workflow ? (
            <Suspense
              fallback={
                <div className={styles.empty}>Loading workflow editor…</div>
              }
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
                  const current = dirtyIds.has(workflow.id)
                    ? await save(workflow)
                    : workflow
                  return api.run(
                    current.id,
                    'simulation',
                    JSON.parse(input),
                    current.version
                  )
                }}
                runLive={() => showRun(workflow)}
              />
            </Suspense>
          ) : (
            <Empty
              title={loaded ? 'Workflow not found' : 'Loading workflow…'}
              action={
                <Button onClick={() => navigate('workflows')}>
                  All workflows
                </Button>
              }
            >
              Your workflow data is loaded from this browser's workspace.
            </Empty>
          )
        ) : (
          <div className={styles.pageContent}>
            {!loaded ? (
              <>
                <div className={styles.skeleton + ' ' + styles.skeletonTitle} />
                <div className={styles.panel + ' ' + styles.skeletonList}>
                  {Array.from({ length: 6 }, (_, i) => (
                    <div className={styles.skeleton} key={i} />
                  ))}
                </div>
              </>
            ) : !connected ? (
              <section className={styles.errorPanel} role="alert">
                <Icon name="warning" />
                <h2>Could not connect to your workspace</h2>
                <p>{notice || 'The backend is unavailable.'}</p>
                <Button onClick={() => void refresh()}>
                  <Icon name="refresh" />
                  Try again
                </Button>
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
                      <Button disabled={disabled} onClick={() => setShowTrash(true)}>Trash</Button>
                      <Button
                        disabled={disabled}
                        onClick={() => fileInput.current?.click()}
                      >
                        <Icon name="upload" />
                        Import JSON
                      </Button>
                      <Button
                        visual="primary"
                        disabled={disabled}
                        onClick={create}
                      >
                        <Icon name="plus" />
                        New workflow
                      </Button>
                    </>
                  }
                />
                <section className={styles.statStrip}>
                  <div className={styles.statGrid}>
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
                      <div className={styles.stat} key={label}>
                        <span>{label}</span>
                        <strong>{value}</strong>
                        <small>{sub}</small>
                      </div>
                    ))}
                  </div>
                  <div className={styles.stateBar}>
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
                  <div className={styles.stateLegend}>
                    {statuses.map((s) => (
                      <span key={s}>
                        <i style={{ background: tone(s).fg }} />
                        {tone(s).label}
                        <b>{runCounts[s]}</b>
                      </span>
                    ))}
                  </div>
                </section>
                <section className={styles.panel}>
                  <div className={styles.listToolbar}>
                    <div className={styles.tabs}>
                      {['All', 'Active', 'Draft', 'Paused'].map((tab) => (
                        <Button
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
                        </Button>
                      ))}
                    </div>
                    <Search
                      value={search}
                      onChange={setSearch}
                      placeholder="Search workflows…"
                    />
                  </div>
                  <div className={styles.tableScroll}>
                    <table>
                      <thead>
                        <tr>
                          <th>Workflow</th>
                          <th>Status</th>
                          <th>Last run</th>
                          <th className={styles.numeric}>Runs 24h</th>
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
                                <Button
                                  className={
                                    styles.nameCell + ' ' + styles.workflowLink
                                  }
                                  onClick={() => open(w)}
                                >
                                  <span className={styles.workflowTile}>
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
                                </Button>
                              </td>
                              <td>
                                <Pill status={w.status} />
                                {w.status !== 'draft' && w.hasUnpublishedChanges && <small>Draft changes</small>}
                                {w.lifecycle?.needsReview && <small>Publication review required</small>}
                              </td>
                              <td>
                                {latest ? (
                                  <Button
                                    visual="ghost"
                                    className={styles.lastRun}
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
                                  </Button>
                                ) : (
                                  <span className={styles.muted}>
                                    No runs yet
                                  </span>
                                )}
                              </td>
                              <td className={styles.numeric}>
                                {
                                  recent.filter((r) => r.workflowId === w.id)
                                    .length
                                }
                              </td>
                              <td>
                                <code className={styles.triggerLabel}>
                                  {trigger?.type === 'Schedule'
                                    ? 'cron ' +
                                      String(trigger.config.cron || '')
                                    : trigger?.type === 'Webhook'
                                      ? 'POST · webhook'
                                      : 'Manual'}
                                </code>
                              </td>
                              <td
                                className={styles.muted + ' ' + styles.nowrap}
                              >
                                {date(w.updatedAt)}
                              </td>
                              <td>
                                <Menu iconOnly label={<Icon name="more" />}>
                                  {(w.status === 'draft' || w.hasUnpublishedChanges) && <Button onClick={() => act(w, 'publish')}>Publish & activate</Button>}
                                  {w.status === 'draft' ? null : (
                                    <Button
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
                                    </Button>
                                  )}
                                  <Button onClick={() => showRun(w)}>
                                    Run now…
                                  </Button>
                                  <Button onClick={() => setHistoryTarget(w)}>Publication history</Button>
                                  <Button onClick={() => clone(w)}>
                                    Duplicate
                                  </Button>
                                  <Button
                                    onClick={() => download(w.id + '.json', w)}
                                  >
                                    Export JSON
                                  </Button>
                                  <hr />
                                  <Button
                                    tone="danger"
                                    disabled={disabled}
                                    onClick={() => setArchiveTarget(w)}
                                  >
                                    Move to trash
                                  </Button>
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
                          <Button visual="primary" onClick={create}>
                            <Icon name="plus" />
                            Create a workflow
                          </Button>
                        ) : undefined
                      }
                    >
                      Connect a trigger, add your logic, and put it to work.
                    </Empty>
                  )}
                  <div className={styles.tableFooter}>
                    <span>
                      Showing {visibleWorkflows.length} of {workflows.length}{' '}
                      workflows
                    </span>
                    <Button visual="ghost" onClick={() => navigate('runs')}>
                      View run history <Icon name="runs" />
                    </Button>
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
                    <Button
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
                    </Button>
                  }
                />
                <section className={styles.panel}>
                  <div className={styles.listToolbar}>
                    <div className={styles.tabs}>
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
                        <Button
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
                        </Button>
                      ))}
                    </div>
                  </div>
                  <div className={styles.filterRow}>
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
                        <th className={styles.numeric}>Steps</th>
                        <th className={styles.numeric}>Retries</th>
                      </tr>
                    </thead>
                    <tbody>
                      {visibleRuns.map((r) => (
                        <tr key={r.id}>
                          <td>
                            <Button
                              visual="ghost"
                              className={styles.mono}
                              onClick={() => inspect(r)}
                            >
                              {r.id.slice(0, 12)}…
                            </Button>
                          </td>
                          <td>
                            <Button
                              visual="ghost"
                              className={styles.textLink}
                              onClick={() => inspect(r)}
                            >
                              {r.name}
                            </Button>
                            <small className={styles.block}>
                              Revision v{r.revision}
                            </small>
                          </td>
                          <td>
                            <Pill status={runState(r)} />
                          </td>
                          <td className={styles.muted}>{r.mode}</td>
                          <td className={styles.muted + ' ' + styles.nowrap}>
                            {date(r.createdAt)}
                          </td>
                          <td className={styles.numeric + ' ' + styles.mono}>
                            {
                              r.nodes.filter((n) =>
                                ['succeeded', 'failed', 'skipped'].includes(
                                  n.status
                                )
                              ).length
                            }{' '}
                            / {r.nodes.length}
                          </td>
                          <td className={styles.numeric + ' ' + styles.mono}>
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
                        <Button onClick={() => navigate('workflows')}>
                          View workflows
                        </Button>
                      }
                    >
                      Open a workflow and run it, or simulate it with test
                      input.
                    </Empty>
                  )}
                  <div className={styles.tableFooter}>
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
                    <Button onClick={() => create()}>
                      <Icon name="plus" />
                      New workflow
                    </Button>
                  }
                />
                <div className={styles.catalogueToolbar}>
                  <div className={styles.tabs}>
                    {categories.map((c) => (
                      <Button
                        key={c}
                        aria-pressed={categoryFilter === c}
                        onClick={() => setConnectorQuery('category', c)}
                      >
                        {c}
                        <small>
                          {
                            catalogue.filter(
                              (x) => c === 'All' || group(x.name) === c
                            ).length
                          }
                        </small>
                      </Button>
                    ))}
                  </div>
                  <label>
                    Availability{' '}
                    <select aria-label="Connector availability" value={connectorStatus} onChange={(e) => setConnectorQuery('status', e.target.value)}>
                      <option value="all">All connectors</option>
                      <option value="built-in">Built in</option>
                      <option value="planned">Planned</option>
                    </select>
                  </label>
                  <Search
                    value={connectorSearch}
                    onChange={(value) => setConnectorQuery('q', value)}
                    placeholder="Search connectors…"
                  />
                </div>
                <div className={styles.catalogueGrid}>
                  {visibleConnectors.map((c) => {
                    const manifest = connectors.find((m) => m.kind === c.name)
                    return (
                      <Button
                        className={styles.catalogueCard}
                        key={c.name}
                        onClick={() => {
                          go(
                            path('connections') +
                              '?connector=' +
                              encodeURIComponent(c.name)
                          )
                        }}
                      >
                        <div className={styles.catalogueCardTop}>
                          <Tile kind={c.name} />
                          {manifest ? (
                            <span className={styles.builtIn}>Built in</span>
                          ) : (
                            <span className={styles.chip}>Planned</span>
                          )}
                        </div>
                        <div className={styles.eyebrow}>{group(c.name)}</div>
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
                        <div className={styles.capabilityChips}>
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
                      </Button>
                    )
                  })}
                </div>
                <div className={styles.catalogueFooter}>
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
            ) : route.page === 'data' ? (
              <DataTables key={route.tableId || 'index'} tableId={route.tableId} />
            ) : route.page === 'settings' ? (
              <Settings
                workspaceName={workspaceLabel}
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
                    <Button
                      disabled
                      title="The credential vault API is planned"
                    >
                      <Icon name="plus" />
                      New connection
                    </Button>
                  }
                />
                <div className={styles.connectionsLayout}>
                  <section className={styles.panel}>
                    <div className={styles.listToolbar}>
                      <div className={styles.tabs}>
                        {['All', 'Needs attention', 'OAuth', 'MCP servers'].map(
                          (t) => (
                            <Button
                              key={t}
                              aria-pressed={filter === t}
                              onClick={() => setFilter(t)}
                            >
                              {t}
                            </Button>
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
                  <aside
                    className={styles.panel + ' ' + styles.connectionDetail}
                  >
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
                    <div className={styles.infoBox}>
                      Secret values stay on the host and are redacted from
                      echoed responses.
                    </div>
                    <Button onClick={() => navigate('workflows')}>
                      Open workflows <Icon name="runs" />
                    </Button>
                  </aside>
                </div>
              </>
            ) : route.page === 'mcp' ? (
              <McpManagement authorizeId={route.query.get('authorize')} />
            ) : route.page === 'audit' ? (
              <AuditLog />
            ) : route.page === 'login' || route.page === 'setup' ? (
              <section className={styles.panel}>
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
                  <Button onClick={() => navigate('workflows')}>
                    All workflows
                  </Button>
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
          <div className={styles.settingsFields}>
            <p role="alert">{runError}</p>
            <Button
              onClick={() => {
                setRunError('')
                void api
                  .getRun(selectedId)
                  .then(setSelectedRun)
                  .catch((e) => setRunError(String(e)))
              }}
            >
              Try again
            </Button>
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
      {showTrash && <WorkflowTrash close={() => setShowTrash(false)} restored={(restored) => { replace(restored); undo.current = null; setUndoAvailable(false); setNotice('Workflow restored. Triggers remain paused.'); void refresh() }} />}
      {archiveTarget && <Dialog title="Move workflow to trash" onClose={() => {if (!busy) setArchiveTarget(null)}}>
        <div className={styles.settingsFields}>
          <p>Move “{archiveTarget.name}” to trash? This stops new triggers and schedules. Existing runs finish, and the workflow, history and runs remain recoverable.</p>
          <Button tone="danger" disabled={busy} onClick={() => void perform(async () => {
            const current = dirty.current.has(archiveTarget.id) ? await save(archiveTarget) : archiveTarget
            const archived = await api.workflowAction(current.id, 'archive', current.version)
            setWorkflows((entries) => entries.filter((w) => w.id !== archived.id))
            setArchiveTarget(null)
            setNotice('Workflow moved to trash. Existing runs continue.')
            setUndoAvailable(true)
            undo.current = async () => {
              replace(await api.workflowAction(archived.id, 'restore', archived.version))
              setNotice('Workflow restored. Triggers remain paused.')
              setUndoAvailable(false)
              undo.current = null
            }
          })}>Move to trash</Button>
        </div>
      </Dialog>}
      {historyTarget && <WorkflowHistory workflow={historyTarget} busy={busy} dirty={dirtyIds.has(historyTarget.id)} close={() => setHistoryTarget(null)} restore={(revision) => {
        void perform(async () => {
          const restored = await api.restoreRevision(historyTarget.id, revision, historyTarget.version)
          replace(restored)
          setHistoryTarget(null)
          open(restored)
          setNotice('Publication restored to draft. Review and publish to change the live definition.')
        })
      }} />}
      {workspaceDialog && (
        <Dialog title={workspaceDialog === 'create' ? 'Create workspace' : 'Rename workspace'} onClose={() => { if (!busy) setWorkspaceDialog(null) }}>
          <form className={styles.settingsFields} onSubmit={(e) => {
            e.preventDefault()
            void perform(async () => {
              const entry = workspaceDialog === 'create' ? await api.createWorkspace(workspaceName) : await api.renameWorkspace(route.workspaceKey, workspaceName)
              if (!mounted.current) return
              setMemberships((entries) => [...entries.filter((w) => w.id !== entry.id), entry])
              setWorkspaceDialog(null)
              setNotice(workspaceDialog === 'create' ? 'Workspace created. Select it in the workspace menu.' : 'Workspace renamed.')
            })
          }}>
            <label>Workspace name<input required maxLength={80} value={workspaceName} onChange={(e) => setWorkspaceName(e.target.value)} /></label>
            <p>Workflows, runs, credentials and public trigger endpoints are isolated per workspace. Access belongs to this browser session.</p>
            <Button visual="primary" disabled={busy || !workspaceName.trim()} type="submit">{workspaceDialog === 'create' ? 'Create workspace' : 'Save name'}</Button>
          </form>
        </Dialog>
      )}
      {runTarget && (
        <Dialog
          title={'Run ' + runTarget.name}
          onClose={() => {
            if (!busy) setRunTarget(null)
          }}
        >
          <div className={styles.settingsFields}>
            <p>
              This starts a real run. Configured HTTP nodes can call external
              services; waits use durable timers.
            </p>
            <label>
              Run input · JSON
              <textarea
                aria-label="Run input JSON"
                className={styles.mono}
                rows={10}
                value={input}
                onChange={(e) => setInput(e.target.value)}
                spellCheck={false}
              />
            </label>
            {runFormError && (
              <p className={styles.danger} role="alert">
                {runFormError}
              </p>
            )}
            <div className={styles.actions}>
              <Button disabled={busy} onClick={() => setRunTarget(null)}>
                Cancel
              </Button>
              <Button
                visual="primary"
                disabled={busy}
                onClick={() => void startLive()}
              >
                <Icon name="play" />
                {busy ? 'Starting…' : 'Start run'}
              </Button>
            </div>
          </div>
        </Dialog>
      )}
      {help && (
        <Dialog title="Keyboard shortcuts" onClose={() => setHelp(false)}>
          <div className={styles.shortcutList}>
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
