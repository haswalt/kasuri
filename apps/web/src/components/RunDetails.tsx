import { useEffect, useRef, useState } from 'react'
import type { Run } from '../lib/workflows'
import { api } from '../lib/api'

export function RunDetails({ run, onClose, onViewAll, onChanged }: { run: Run; onClose: () => void; onViewAll: () => void; onChanged: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null)
  const [detail, setDetail] = useState(run)
  const [error, setError] = useState('')
  const [pending, setPending] = useState(false)
  useEffect(() => { dialog.current?.showModal() }, [])
  useEffect(() => {
    let active = true
    let timer: ReturnType<typeof setTimeout> | undefined
    async function load() {
      try {
        const result = await api.getRun(run.id)
        if (!active) return
        setDetail(result)
        if (['running', 'paused'].includes(result.status)) timer = setTimeout(() => { void load() }, 1500)
      } catch (e) { if (active) setError(String(e)) }
    }
    void load()
    return () => { active = false; clearTimeout(timer) }
  }, [run.id])
  async function action(name: string) {
    setPending(true); setError('')
    try { setDetail(await api.runAction(run.id, name)); onChanged() } catch (e) { setError(e instanceof Error ? e.message : 'Action failed.') }
    finally { setPending(false) }
  }
  return <dialog ref={dialog} className="run-modal" onClose={onClose} onClick={e => { if (e.target === dialog.current) onClose() }} aria-labelledby="run-title">
    <div className="panel-heading"><h2 id="run-title">{detail.name}</h2><button autoFocus aria-label="Close run details" onClick={onClose}>×</button></div>
    <span className={`badge ${detail.status}`}>{detail.mode} · {detail.status}</span>
    <p className="muted">{new Date(detail.createdAt).toLocaleString('en-GB')} · Revision {detail.revision}</p>
    <div className="info-box">{detail.mode === 'simulation' ? 'Core logic executed by the Rust backend with a virtual clock. Waits advance instantly.' : 'Core logic executed by the Rust backend. Waits persist and resume through durable alarms.'} No external services are called.</div>
    {(error || detail.error) && <p role="alert" className="danger">{error || detail.error}</p>}
    <details><summary>Run input</summary><pre>{JSON.stringify(detail.input, null, 2)}</pre></details>
    <ol className="timeline">{detail.nodes.map((node, i) => <li key={node.id}><span>{i + 1}</span><div><strong>{node.label}</strong><small>{node.type} · {node.status} · {node.attempts} attempts</small>{node.error && <p className="danger">{node.error}</p>}{node.untilMs && <small>Wake: {new Date(node.untilMs).toLocaleString('en-GB')}</small>}{node.output !== undefined && <details><summary>Output</summary><pre>{JSON.stringify(node.output, null, 2)}</pre></details>}</div></li>)}</ol>
    <div className="actions">{['running', 'paused'].includes(detail.status) && <><button disabled={pending} onClick={() => void action(detail.status === 'paused' ? 'resume' : 'pause')}>{detail.status === 'paused' ? 'Resume' : 'Pause'} run</button><button disabled={pending} onClick={() => void action('cancel')}>Cancel run</button></>}<button onClick={onViewAll}>View all runs →</button></div>
  </dialog>
}
