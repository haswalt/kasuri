import { useEffect, useState } from 'react'
import { api } from '../lib/api'
import type { Workflow } from '../lib/workflows'
import { Button, Dialog, Empty, PageHeader } from '../design/ui'
import { styles } from '../design/styles'
import { date } from '../design/ui-model'

export function WorkflowTrash({
  back,
  restored,
  deleted
}: {
  back: () => void
  restored: (workflow: Workflow) => void
  deleted: (workflow: Workflow) => void
}) {
  const [entries, setEntries] = useState<Workflow[]>([])
  const [loading, setLoading] = useState(true)
  const [reload, setReload] = useState(0)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [deleteTarget, setDeleteTarget] = useState<Workflow | null>(null)

  useEffect(() => {
    let active = true
    void api.trash().then(result => {
      if (active) setEntries(result.workflows)
    }).catch(e => {
      if (active) setError(e instanceof Error ? e.message : 'Could not load trash.')
    }).finally(() => {
      if (active) setLoading(false)
    })
    return () => { active = false }
  }, [reload])

  function retry() {
    setLoading(true)
    setError('')
    setReload(value => value + 1)
  }

  async function update(workflow: Workflow, permanent: boolean) {
    setBusy(true)
    setError('')
    try {
      if (workflow.version === undefined) {
        throw new Error('Reload trash to get the current workflow version.')
      }
      if (permanent) {
        await api.deleteWorkflow(workflow.id, workflow.version)
        deleted(workflow)
      } else {
        restored(await api.workflowAction(workflow.id, 'restore', workflow.version))
      }
      setEntries(list => list.filter(entry => entry.id !== workflow.id))
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not update the workflow.')
    } finally {
      setDeleteTarget(null)
      setBusy(false)
    }
  }

  return <>
    <PageHeader
      eyebrow="WORKFLOWS"
      title="Trash"
      description="Restore a workflow or permanently delete its draft and publication history."
      actions={<Button onClick={back}>All workflows</Button>}
    />
    <section className={styles.panel}>
      {error && <div className={styles.listToolbar} role="alert">
        <p>{error}</p>
        <Button onClick={retry} disabled={loading || busy}>Reload trash</Button>
      </div>}
      {loading ? <Empty title="Loading trash…">Fetching archived workflows.</Empty>
        : entries.length ? <div className={styles.tableScroll}>
          <table>
            <thead><tr><th>Workflow</th><th>Moved to trash</th><th>Actions</th></tr></thead>
            <tbody>
              {entries.map(workflow => <tr key={workflow.id}>
                <td><strong>{workflow.name}</strong></td>
                <td>{date(workflow.archivedAt || '')}</td>
                <td>
                  <div className={styles.actions}>
                    <Button disabled={busy} aria-label={`Restore ${workflow.name}`} onClick={() => void update(workflow, false)}>Restore</Button>
                    <Button disabled={busy} tone="danger" aria-label={`Delete ${workflow.name} permanently`} onClick={() => setDeleteTarget(workflow)}>Delete permanently</Button>
                  </div>
                </td>
              </tr>)}
            </tbody>
          </table>
        </div> : !error && <Empty title="Trash is empty">Workflows you move to trash will appear here.</Empty>}
    </section>
    {deleteTarget && <Dialog title="Permanently delete workflow?" onClose={() => { if (!busy) setDeleteTarget(null) }}>
      <div className={styles.settingsFields}>
        <p>“{deleteTarget.name}” and its publication history will be permanently deleted. Run records follow the workspace’s retention policy.</p>
        <div className={styles.actions}>
          <Button disabled={busy} onClick={() => setDeleteTarget(null)}>Cancel</Button>
          <Button disabled={busy} tone="danger" onClick={() => void update(deleteTarget, true)}>Delete permanently</Button>
        </div>
      </div>
    </Dialog>}
  </>
}
