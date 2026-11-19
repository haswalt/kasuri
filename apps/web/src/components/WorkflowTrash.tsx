import { useEffect, useState } from 'react'
import { api } from '../lib/api'
import type { Workflow } from '../lib/workflows'
import { Button, Dialog } from '../design/ui'
import { styles } from '../design/styles'
import { date } from '../design/ui-model'
export function WorkflowTrash({close, restored}: {close:()=>void; restored:(workflow:Workflow)=>void}) {
  const [entries,setEntries] = useState<Workflow[]>([])
  const [busy,setBusy] = useState(false)
  const [error,setError] = useState('')
  const [loaded,setLoaded] = useState(false)
  useEffect(()=>{
    let active=true
    void api.trash().then(result=>{if(active){setEntries(result.workflows);setLoaded(true)}}).catch(e=>{if(active){setError(String(e));setLoaded(true)}})
    return()=>{active=false}
  },[])
  async function restore(workflow:Workflow) {
    setBusy(true);setError('')
    try {
      const result=await api.workflowAction(workflow.id,'restore',workflow.version)
      setEntries(list=>list.filter(w=>w.id!==workflow.id))
      restored(result)
    } catch(e) {setError(e instanceof Error?e.message:'Could not restore workflow')}
    finally {setBusy(false)}
  }
  return <Dialog title="Workflow trash" onClose={()=>{if(!busy)close()}}>
    <div className={styles.settingsFields}>
      <p>Archived workflows retain their drafts, publication history and runs. Restoring keeps triggers paused until you explicitly resume or publish.</p>
      {error&&<p role="alert">{error}</p>}
      {!loaded&&<p>Loading trash…</p>}
      {loaded&&!entries.length&&<p>Trash is empty.</p>}
      {entries.map(workflow=><div className={styles.settingRow} key={workflow.id}>
        <div><strong>{workflow.name}</strong><br/><small>Archived {date(workflow.archivedAt||'')}</small></div>
        <Button disabled={busy} onClick={()=>void restore(workflow)}>Restore {workflow.name}</Button>
      </div>)}
    </div>
  </Dialog>
}
