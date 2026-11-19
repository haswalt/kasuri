import { useEffect, useState } from 'react'
import { api } from '../lib/api'
import type { WorkflowRevision } from '../lib/api'
import type { Workflow } from '../lib/workflows'
import { Button, Dialog } from '../design/ui'
import { styles } from '../design/styles'
import { date } from '../design/ui-model'
export function WorkflowHistory({ workflow, busy, dirty, close, restore }: {
  workflow: Workflow; busy: boolean; dirty: boolean; close: () => void; restore: (revision: number) => void
}) {
  const [revisions, setRevisions] = useState<WorkflowRevision[]>([])
  const [selected, setSelected] = useState<number>()
  const [error, setError] = useState('')
  const [loaded, setLoaded] = useState(false)
  useEffect(() => {
    let active = true
    void api.revisions(workflow.id).then((result) => {
      if (!active) return
      setRevisions(result.revisions)
      setSelected(result.revisions[0]?.revision)
      setLoaded(true)
    }).catch((e) => { if (active) { setError(e instanceof Error ? e.message : 'Could not load history'); setLoaded(true) } })
    return () => { active = false }
  }, [workflow.id])
  const revision = revisions.find((r) => r.revision === selected)
  const before = new Map(revision?.graph.nodes.map((n) => [n.id, n]))
  const after = new Map(workflow.nodes.map((n) => [n.id, n]))
  const changed = [...after].filter(([id,n]) => before.has(id) && JSON.stringify(before.get(id)) !== JSON.stringify(n)).length
  return <Dialog title="Publication history" onClose={close}>
    <div className={styles.settingsFields}>
      <p>Restore a revision into the draft, review it, then publish to make it live. Running workflows keep their original definition.</p>
      {workflow.lifecycle?.needsReview && <p role="alert">This legacy draft was edited after publishing. Its original live definition was not retained. Review and publish it explicitly; older history cannot be reconstructed.</p>}
      {error && <p role="alert">{error}</p>}
      {!loaded && <p>Loading history…</p>}
      {loaded && !revisions.length && <p>No retained publications yet.</p>}
      {revision && <>
        <label>Published revision<select value={selected} onChange={(e) => setSelected(Number(e.target.value))}>
          {revisions.map((r) => <option key={r.revision} value={r.revision}>v{r.revision}{r.revision === workflow.lifecycle?.publishedRevision ? ' · current live definition' : ''} · {r.legacyBaseline ? 'Legacy baseline' : date(r.publishedAt)}</option>)}
        </select></label>
        <p>{revision.legacyBaseline ? 'Imported current baseline. Original publication date and earlier revisions are unavailable.' : `Published by ${revision.publishedBy} · ${date(revision.publishedAt)}`}</p>
        <p>Compared with this draft: {[...after.keys()].filter((id) => !before.has(id)).length} added nodes, {[...before.keys()].filter((id) => !after.has(id)).length} removed nodes, {changed} changed nodes. {JSON.stringify(revision.graph.edges) === JSON.stringify(workflow.edges) ? 'Connections unchanged.' : 'Connections differ.'}{revision.graph.name !== workflow.name ? ' Name differs.' : ''}{revision.graph.description !== workflow.description ? ' Description differs.' : ''}</p>
        <details><summary>Published definition</summary><pre>{JSON.stringify(revision.graph, null, 2)}</pre></details>
        {dirty && <p>Save the current draft before restoring a publication.</p>}
        <Button visual="primary" disabled={busy || dirty} onClick={() => restore(revision.revision)}>Restore v{revision.revision} to draft</Button>
      </>}
    </div>
  </Dialog>
}
