import { useEffect, useState } from 'react'
import { api } from '../lib/api'
import type { McpStatus, OAuthRequest, AuditPage, AuditEvent } from '../lib/api'
import { Button, Dialog, Empty, Icon, PageHeader, Pill } from '../design/ui'
import { styles } from '../design/styles'
import { css } from '../../styled-system/css'
const stack = css({ display: 'flex', flexDirection: 'column', gap: '16px' })
const scopeLabels: Record<string, string> = {
  read: 'Inspect workflows, connector schemas and runs',
  audit: 'Read retained audit records',
  write: 'Create and edit workflow drafts',
  simulate: 'Run offline simulations',
  publish: 'Publish and activate schedules or webhook triggers',
  execute: 'Start real runs with external effects',
  control: 'Pause, resume or cancel runs',
  delete: 'Move workflows to trash and restore them'
}
export function McpManagement({ authorizeId }: { authorizeId: string | null }) {
  const [data, setData] = useState<McpStatus | null>(null),
    [error, setError] = useState(''),
    [busy, setBusy] = useState(false)
  const [creating, setCreating] = useState(false),
    [name, setName] = useState(''),
    [role, setRole] = useState('reader'),
    [days, setDays] = useState(30),
    [token, setToken] = useState(''),
    [copied, setCopied] = useState(false)
  const [consent, setConsent] = useState<OAuthRequest | null>(null),
    [approved, setApproved] = useState<string[]>([])
  async function load() {
    try {
      setData(await api.mcp())
    } catch (e) {
      setError(String(e))
    }
  }
  useEffect(() => {
    void load()
  }, [])
  useEffect(() => {
    if (!authorizeId) return
    let active = true
    void api
      .oauthRequest(authorizeId)
      .then((value) => {
        if (active) {
          setConsent(value)
          setApproved(value.scopes.filter((s) => s === 'read' || s === 'audit'))
        }
      })
      .catch(() => {
        if (active)
          setError(
            'This authorization request expired or belongs to another workspace. Open the consent link in the browser that owns the target workspace.'
          )
      })
    return () => {
      active = false
    }
  }, [authorizeId])
  async function act(action: () => Promise<void>) {
    if (busy) return
    setBusy(true)
    setError('')
    try {
      await action()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setBusy(false)
    }
  }
  async function authorize(allow: boolean) {
    if (!consent) return
    await act(async () => {
      const result = await api.oauthConsent(consent.id, approved, allow)
      location.assign(result.redirect)
    })
  }
  return (
    <div className={stack}>
      <PageHeader
        eyebrow="MODEL CONTEXT PROTOCOL"
        title="MCP server"
        description="Connect an agent to this workspace using scoped credentials or browser-approved OAuth."
        actions={
          <Button
            visual="primary"
            onClick={() => {
              setCreating(true)
              setName('')
              setRole('reader')
            }}
          >
            <Icon name="plus" /> Create credential
          </Button>
        }
      />
      {error && (
        <div className={styles.errorPanel} role="alert">
          {error}
        </div>
      )}
      <section className={styles.panel}>
        <div className={styles.panelHeading}>
          <h2>Workspace endpoint</h2>
          <Pill status="active" label="Available" />
        </div>
        <div className={styles.settingsFields}>
          <label>
            Streamable HTTP MCP URL
            <input
              readOnly
              value={data?.endpoint || 'Loading…'}
              aria-label="MCP endpoint"
              className={styles.mono}
            />
          </label>
          <div className={styles.actions}>
            <Button
              disabled={!data}
              onClick={() =>
                void act(async () => {
                  await navigator.clipboard.writeText(data!.endpoint)
                  setCopied(true)
                })
              }
            >
              <Icon name="copy" />
              {copied ? 'Copied' : 'Copy endpoint'}
            </Button>
            <span className={styles.muted}>
              Rust/Wasm · stateless HTTP · protocol {data?.protocolVersion}
            </span>
          </div>
          <p>
            This URL belongs to this deployment and workspace. Browser cookies
            and installation passwords do not grant MCP access.
          </p>
        </div>
      </section>
      <div className={styles.connectionsLayout}>
        <section className={styles.panel}>
          <div className={styles.panelHeading}>
            <h2>Agent credentials</h2>
            <span className={styles.chip}>
              {data?.credentials.filter((c) => c.active).length || 0} active
            </span>
          </div>
          {!data?.credentials.length ? (
            <Empty icon="key" title="No agents connected">
              Create a scoped token, or add the endpoint to an OAuth-capable MCP
              client and approve its request here.
            </Empty>
          ) : (
            <div className={styles.tableScroll}>
              <table>
                <thead>
                  <tr>
                    <th>Credential</th>
                    <th>Permissions</th>
                    <th>Expires</th>
                    <th>Status</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {data.credentials.map((c) => (
                    <tr key={c.id}>
                      <td>
                        <strong>{c.name}</strong>
                        <br />
                        <small>
                          {c.authentication === 'oauth'
                            ? 'OAuth grant'
                            : 'Bearer token'}
                        </small>
                      </td>
                      <td>
                        <small>{c.scopes.join(', ')}</small>
                      </td>
                      <td>{new Date(c.expiresAt).toLocaleString()}</td>
                      <td>
                        <Pill
                          status={c.active ? 'active' : 'paused'}
                          label={
                            c.revoked
                              ? 'Revoked'
                              : c.active
                                ? 'Active'
                                : c.refreshAvailable
                                  ? 'Refresh available'
                                  : 'Expired'
                          }
                        />
                      </td>
                      <td>
                        <Button
                          tone="danger"
                          disabled={busy || c.revoked}
                          onClick={() =>
                            void act(async () => {
                              await api.revokeMcpCredential(c.id)
                              await load()
                            })
                          }
                        >
                          Revoke
                        </Button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
        <aside className={styles.panel + ' ' + styles.connectionDetail}>
          <Icon name="plug" size={28} />
          <h2>Two ways to connect</h2>
          <p>
            <strong>Bearer token:</strong> configure your MCP client with this
            URL and an <code>Authorization: Bearer …</code> header. The secret
            is displayed only once.
          </p>
          <p>
            <strong>OAuth:</strong> enter the URL in a compatible client or
            Cloudflare MCP portal. The client discovers this workspace's
            authorization server and opens a consent request. PKCE binds
            authorization to that client.
          </p>
          <div className={styles.infoBox}>
            Reader can inspect and audit. Builder adds draft editing and
            simulations. Operator also permits publishing, real execution and
            run control. Manager adds recoverable workflow trash. Credential administration stays in the owner portal.
          </div>
          {Boolean(data?.unusedClients) && (
            <Button
              disabled={busy}
              onClick={() =>
                void act(async () => {
                  await api.resetUnusedOAuthClients()
                  await load()
                })
              }
            >
              Clear {data?.unusedClients} unused OAuth registrations
            </Button>
          )}
          <p>
            Outbound MCP connections are a separate connector capability. This
            endpoint manages Kasuri itself.
          </p>
        </aside>
      </div>
      {creating && (
        <Dialog
          title="Create agent credential"
          onClose={() => setCreating(false)}
        >
          <div className={styles.settingsFields}>
            <label>
              Name
              <input
                value={name}
                maxLength={80}
                onChange={(e) => setName(e.target.value)}
                placeholder="Agent or client name"
                autoComplete="off"
              />
            </label>
            <label>
              Permissions
              <select value={role} onChange={(e) => setRole(e.target.value)}>
                <option value="reader">Reader — inspect and audit</option>
                <option value="builder">
                  Builder — edit drafts and simulate
                </option>
                <option value="operator">Operator — publish and execute</option>
                <option value="manager">Manager — operator plus workflow trash</option>
              </select>
            </label>
            <ul>
              {(data?.roles[role] || []).map((scope) => (
                <li key={scope}>{scopeLabels[scope]}</li>
              ))}
            </ul>
            <label>
              Expires in
              <select
                value={days}
                onChange={(e) => setDays(Number(e.target.value))}
              >
                {[1, 7, 30, 90].map((n) => (
                  <option key={n} value={n}>
                    {n} {n === 1 ? 'day' : 'days'}
                  </option>
                ))}
              </select>
            </label>
            {error && (
              <p className={styles.danger} role="alert">
                {error}
              </p>
            )}
            <div className={styles.actions}>
              <Button onClick={() => setCreating(false)}>Cancel</Button>
              <Button
                visual="primary"
                disabled={busy || !name.trim()}
                onClick={() =>
                  void act(async () => {
                    const result = await api.createMcpCredential(
                      name,
                      role,
                      days
                    )
                    setToken(result.token)
                    setCreating(false)
                    await load()
                  })
                }
              >
                Create credential
              </Button>
            </div>
          </div>
        </Dialog>
      )}
      {token && (
        <Dialog title="Copy your agent token" onClose={() => setToken('')}>
          <div className={styles.settingsFields}>
            <p>
              Store this secret in your MCP client's secure credential storage.
              It will not be shown again.
            </p>
            <label>
              Token
              <input
                readOnly
                value={token}
                aria-label="New MCP token"
                autoComplete="off"
                spellCheck={false}
                className={styles.mono}
              />
            </label>
            <div className={styles.actions}>
              <Button
                onClick={() =>
                  void act(async () => navigator.clipboard.writeText(token))
                }
              >
                <Icon name="copy" />
                Copy token
              </Button>
              <Button visual="primary" onClick={() => setToken('')}>
                Done
              </Button>
            </div>
          </div>
        </Dialog>
      )}
      {consent && (
        <Dialog
          title="Authorize an MCP client"
          onClose={() => void authorize(false)}
        >
          <div className={styles.settingsFields}>
            <h3>{consent.clientName}</h3>
            <p>
              This client requests access to this workspace. Its name is
              supplied by the client; review the callback and permissions before
              approving.
            </p>
            <label>
              Client ID
              <input
                readOnly
                value={consent.clientId}
                className={styles.mono}
              />
            </label>
            <label>
              Callback URL
              <input
                readOnly
                value={consent.redirectUri}
                className={styles.mono}
              />
            </label>
            <label>
              Target workspace
              <input
                readOnly
                value={consent.resource}
                className={styles.mono}
              />
            </label>
            <div className={stack}>
              {consent.scopes.map((scope) => (
                <label
                  key={scope}
                  className={css({
                    display: 'flex',
                    flexDirection: 'row',
                    alignItems: 'center',
                    gap: '10px'
                  })}
                >
                  <input
                    type="checkbox"
                    className={css({ width: '16px', height: '16px' })}
                    checked={approved.includes(scope)}
                    onChange={(e) =>
                      setApproved((v) =>
                        e.target.checked
                          ? [...v, scope]
                          : v.filter((s) => s !== scope)
                      )
                    }
                  />
                  {scopeLabels[scope] || scope}
                </label>
              ))}
            </div>
            <p>
              Access tokens expire in one hour. OAuth refresh access lasts up to
              30 days and can be revoked from this portal.
            </p>
            {error && (
              <p className={styles.danger} role="alert">
                {error}
              </p>
            )}
            <div className={styles.actions}>
              <Button disabled={busy} onClick={() => void authorize(false)}>
                Deny
              </Button>
              <Button
                visual="primary"
                disabled={busy || !approved.includes('read')}
                onClick={() => void authorize(true)}
              >
                Authorize selected permissions
              </Button>
            </div>
          </div>
        </Dialog>
      )}
    </div>
  )
}
export function AuditLog() {
  const [selected,setSelected] = useState<AuditEvent | null>(null)
  const [page, setPage] = useState<AuditPage | null>(null),
    [error, setError] = useState(''),
    [busy, setBusy] = useState(false)
  async function load(after = 0) {
    setBusy(true)
    setError('')
    try {
      setPage(await api.audit(after))
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy(false)
    }
  }
  useEffect(() => {
    void load()
  }, [])
  return (
    <div className={stack}>
      <PageHeader
        eyebrow="GOVERNANCE"
        title="Audit log"
        description="Workflow, workspace and credential changes with context. Run activity and routine reads are excluded."
        actions={
          <Button disabled={busy} onClick={() => void load()}>
            Refresh
          </Button>
        }
      />
      {error && (
        <p className={styles.errorPanel} role="alert">
          {error}
        </p>
      )}
      <div className={styles.infoBox}>
        Retains up to {page?.retentionLimit || 1000} records.{' '}
        {page?.totalRecorded || 0} recorded in total.
        {(page?.oldestRetainedId || 0) > 1
          ? ' Earlier records have aged out of retention.'
          : ''}{' '}
        This log starts with this platform version; earlier activity has no
        audit record.
      </div>
      <section className={styles.panel}>
        {!page?.items.length ? (
          <Empty icon="file" title="No audit records">
            Management actions will appear here as they occur.
          </Empty>
        ) : (
          <>
            <div className={styles.tableScroll}>
              <table>
                <thead>
                  <tr>
                    <th>ID / time</th>
                    <th>Actor</th>
                    <th>Action</th>
                    <th>Target</th>
                    <th>Outcome</th>
                  </tr>
                </thead>
                <tbody>
                  {page.items.map((e) => (
                    <tr key={e.id}>
                      <td>
                        #{e.id}
                        <br />
                        <small>{new Date(e.at).toLocaleString()}</small>
                      </td>
                      <td className={styles.mono}>{e.actor}</td>
                      <td><Button visual="ghost" onClick={() => setSelected(e)}>{e.summary || e.action}</Button><br/><small>{e.action}</small></td>
                      <td>{e.entity?.name || e.target}<br/><small>{e.entity?.type}</small></td>
                      <td>
                        <Pill
                          status={
                            ['succeeded', 'recorded'].includes(e.outcome)
                              ? 'succeeded'
                              : 'failed'
                          }
                          label={e.outcome}
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className={styles.tableFooter}>
              <small>Oldest retained record: {page.oldestRetainedId}</small>
              <Button
                disabled={busy || page.items.length < 100 || !page.nextCursor}
                onClick={() => void load(page.nextCursor!)}
              >
                Next page
              </Button>
            </div>
          </>
        )}
      </section>
      {selected && <Dialog title="Audit detail" onClose={() => setSelected(null)}>
        <div className={styles.settingsFields}>
          <strong>{selected.summary || selected.action}</strong>
          <p>{selected.entity?.type || 'Management'} · {selected.entity?.name || selected.target}</p>
          <dl><dt>Actor</dt><dd>{selected.actor}</dd><dt>Time</dt><dd>{new Date(selected.at).toLocaleString()}</dd><dt>Outcome</dt><dd>{selected.outcome}</dd><dt>Target ID</dt><dd className={styles.mono}>{selected.target}</dd></dl>
          {selected.changes?.length ? <ul>{selected.changes.map((change,index) => <li key={index}><strong>{change.nodeName || change.field}</strong>: {change.description || `${JSON.stringify(change.before)} → ${JSON.stringify(change.after)}`}</li>)}</ul> : <p>No field-level change details were retained for this event.</p>}
          {(selected.before || selected.after) && <><h3>State references</h3><pre>{JSON.stringify({before:selected.before,after:selected.after},null,2)}</pre></>}
          {(selected.reason || selected.context || selected.attemptedAction) && <pre>{JSON.stringify({reason:selected.reason,context:selected.context,attemptedAction:selected.attemptedAction,requiredScope:selected.requiredScope},null,2)}</pre>}
          <small>Configuration values, credentials and workflow payloads are excluded from audit details.</small>
        </div>
      </Dialog>}
    </div>
  )
}
