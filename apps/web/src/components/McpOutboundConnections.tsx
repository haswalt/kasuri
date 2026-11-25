import { useEffect, useState } from 'react'
import { Empty, Pill } from '../design/ui'
import { styles } from '../design/styles'
import { api } from '../lib/api'
import type { McpClientConnection } from '../lib/api'

const method = {
  none: 'Anonymous',
  bearer: 'Bearer token',
  cloudflare_access: 'Cloudflare MCP Portal'
}

export function McpOutboundConnections() {
  const [connections, setConnections] = useState<McpClientConnection[] | null>(null)
  const [error, setError] = useState(false)
  useEffect(() => {
    let active = true
    api.mcpClientConnections().then(result => {
      if (active) setConnections(result.connections)
    }).catch(() => {
      if (active) setError(true)
    })
    return () => { active = false }
  }, [])

  return <section aria-label="Outbound MCP connections">
    <div className={styles.panelHeading}>
      <h2>Outbound MCP connections</h2>
    </div>
    {error ? <p role="alert">Could not load MCP connections.</p>
      : connections === null ? <p role="status">Loading connections…</p>
      : connections.length === 0 ? (
        <Empty icon="plug" title="No MCP connections">
          An operator can add a Cloudflare MCP Portal or direct MCP server for this workspace.
        </Empty>
      ) : (
        <div className={styles.tableScroll}>
          <table>
            <thead><tr><th>Connection</th><th>Authentication</th><th>Approved tools</th><th>Setup</th></tr></thead>
            <tbody>{connections.map(connection => <tr key={connection.alias}>
              <td>{connection.alias}</td>
              <td>{method[connection.authentication]}</td>
              <td>{connection.tools.join(', ') || 'None'}</td>
              <td><Pill status={connection.configured ? 'active' : 'draft'}
                label={connection.configured ? 'Configured' : 'Needs setup'} /></td>
            </tr>)}</tbody>
          </table>
        </div>
      )}
    <p>Manage portal access and upstream credentials in <a href="https://one.dash.cloudflare.com/" target="_blank" rel="noreferrer">Cloudflare One</a>. Workflow authors select the connection by name.</p>
  </section>
}
