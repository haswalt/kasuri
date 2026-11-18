import { useEffect, useRef } from 'react'
import type { Run } from '../lib/workflows'

export function RunDetails({ run, onClose, onViewAll }: { run: Run; onClose: () => void; onViewAll: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null)
  useEffect(() => { dialog.current?.showModal() }, [])
  return <dialog ref={dialog} className="run-modal" onClose={onClose} onClick={event => { if (event.target === dialog.current) onClose() }} aria-labelledby="run-title">
    <div className="panel-heading"><h2 id="run-title">{run.name}</h2><button autoFocus aria-label="Close run details" onClick={onClose}>×</button></div>
    <span className="badge">Simulation only</span>
    <p className="muted">{new Date(run.createdAt).toLocaleString('en-GB')} · Revision {run.revision}</p>
    <div className="info-box">This previews the step order. No node code, HTTP requests, credentials or external side effects are executed.</div>
    <ol className="timeline">{run.nodes.map((node, i) => <li key={i}><span>{i + 1}</span><div><strong>{node.label}</strong><small>{node.type} · Previewed</small></div></li>)}</ol>
    <button onClick={onViewAll}>View all runs →</button>
  </dialog>
}
