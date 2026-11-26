import { useEffect, useState, type FormEvent } from 'react'
import { Button, Dialog, Empty, Icon } from '../design/ui'
import { styles } from '../design/styles'
import { api, type WorkspaceSecret } from '../lib/api'

type Edit = { action: 'create' } | { action: 'rotate' | 'delete'; secret: WorkspaceSecret }

export function WorkspaceSecrets() {
  const [vault, setVault] = useState<Awaited<ReturnType<typeof api.secrets>> | null>(null)
  const [edit, setEdit] = useState<Edit | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let active = true
    void api.secrets().then(result => {
      if (active) setVault(result)
    }).catch(() => {
      if (active) setError('Could not load workspace secrets.')
    })
    return () => { active = false }
  }, [])

  function open(next: Edit) {
    setError('')
    setEdit(next)
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!edit || !vault || busy) return
    const fields = new FormData(event.currentTarget)
    const value = String(fields.get('value') ?? '')
    setError('')
    setBusy(true)
    try {
      if (edit.action === 'delete') {
        await api.deleteSecret(edit.secret.alias)
        setVault({ ...vault, secrets: vault.secrets.filter(secret => secret.alias !== edit.secret.alias) })
      } else {
        const secret = edit.action === 'create'
          ? await api.createSecret(String(fields.get('alias')).trim(), String(fields.get('origin')).trim(), value)
          : await api.rotateSecret(edit.secret.alias, value)
        setVault({ ...vault, secrets: [...vault.secrets.filter(row => row.alias !== secret.alias), secret]
          .sort((first, second) => first.alias.localeCompare(second.alias)) })
      }
      setEdit(null)
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not save the secret.')
    } finally {
      setBusy(false)
    }
  }

  const dialogTitle = edit?.action === 'rotate' ? 'Rotate secret' : edit?.action === 'delete' ? 'Delete secret' : 'New secret'
  return (
    <section aria-label="Workspace secrets">
      <div className={styles.panelHeading}>
        <h2>Workspace secrets</h2>
        <Button disabled={!vault?.configured} onClick={() => open({ action: 'create' })}>
          <Icon name="plus" /> New secret
        </Button>
      </div>
      {error && !edit && <p role="alert">{error}</p>}
      {!vault && !error && <p role="status">Loading secrets…</p>}
      {vault && !vault.configured && (
        <p className={styles.infoBox}>
          Set <code>WORKSPACE_SECRETS_KEY</code> on the Worker to enable the vault.
        </p>
      )}
      {vault && (
        vault.secrets.length === 0 ? (
          <Empty icon="key" title="No workspace secrets">
            Add a credential for an HTTPS origin your workflows need to access.
          </Empty>
        ) : (
          <div className={styles.tableScroll}>
            <table>
              <thead><tr><th>Reference</th><th>Allowed HTTPS origin</th><th>Actions</th></tr></thead>
              <tbody>{vault.secrets.map(secret => (
                <tr key={secret.alias}>
                  <td><code className={styles.mono}>{`{{ secrets.${secret.alias} }}`}</code></td>
                  <td>{secret.origin}</td>
                  <td><div className={styles.actions}>
                    <Button disabled={!vault.configured} onClick={() => open({ action: 'rotate', secret })}>Rotate</Button>
                    <Button onClick={() => open({ action: 'delete', secret })}>Delete</Button>
                  </div></td>
                </tr>
              ))}</tbody>
            </table>
          </div>
        )
      )}
      {edit && (
        <Dialog title={dialogTitle} onClose={() => { if (!busy) setEdit(null) }}>
          <form className={styles.settingsFields} onSubmit={event => void submit(event)}>
            {edit.action === 'create' ? <>
              <label>Alias
                <input name="alias" required maxLength={128} spellCheck={false} autoComplete="off" placeholder="shipping_api" />
              </label>
              <label>Allowed HTTPS origin
                <input name="origin" type="url" required placeholder="https://api.example.com" />
              </label>
              <p>Choose a service you trust with this credential.</p>
            </> : <p><code>{edit.secret.alias}</code> · {edit.secret.origin}</p>}
            {edit.action === 'delete' ? (
              <p>Requests using this secret will fail after it is deleted.</p>
            ) : (
              <label>{edit.action === 'create' ? 'Secret value' : 'Replacement value'}
                <input name="value" type="password" required autoComplete="new-password" />
              </label>
            )}
            {error && <p role="alert">{error}</p>}
            <div className={styles.actions}>
              <Button type="button" disabled={busy} onClick={() => setEdit(null)}>Cancel</Button>
              <Button type="submit" visual="primary" disabled={busy}>{edit.action === 'delete' ? 'Delete secret' : 'Save secret'}</Button>
            </div>
          </form>
        </Dialog>
      )}
    </section>
  )
}
